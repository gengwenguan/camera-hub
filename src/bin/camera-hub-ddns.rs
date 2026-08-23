use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use clap::Parser;
use hmac::{Hmac, Mac};
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::Ipv6Addr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const API_HOST: &str = "dnspod.tencentcloudapi.com";
const API_ENDPOINT: &str = "https://dnspod.tencentcloudapi.com/";
const API_SERVICE: &str = "dnspod";
const API_VERSION: &str = "2021-03-23";
const CONTENT_TYPE_JSON: &str = "application/json; charset=utf-8";
const SIGNED_HEADERS: &str = "content-type;host;x-tc-action";
const ADDRESS_FLAG_TEMPORARY: u32 = 0x01;
const ADDRESS_FLAG_DAD_FAILED: u32 = 0x08;
const ADDRESS_FLAG_DEPRECATED: u32 = 0x20;
const ADDRESS_FLAG_TENTATIVE: u32 = 0x40;

#[derive(Parser)]
#[command(
    name = "camera-hub-ddns",
    about = "Synchronize one IPv6 /64 prefix to multiple DNSPod AAAA records"
)]
struct Args {
    #[arg(long, env = "CAMERA_HUB_DDNS_DOMAIN", default_value = "gwghome.site")]
    domain: String,

    #[arg(long, env = "CAMERA_HUB_DDNS_SECRET_ID", default_value = "")]
    secret_id: String,

    #[arg(long, env = "CAMERA_HUB_DDNS_SECRET_KEY", default_value = "")]
    secret_key: String,

    #[arg(long, env = "CAMERA_HUB_DDNS_INTERFACE", default_value = "wlan0")]
    interface: String,

    #[arg(
        long,
        env = "CAMERA_HUB_DDNS_RECORDS",
        default_value = "@=528f:4cff:feef:dd90,mi6=528f:4cff:feef:dd90,v831=a22c:36ff:febd:4feb,lecoo=8647:09ff:fe45:35a0,lecoo-wifi=72c9:12ff:fe1c:2f67,huawei=1a56:80ff:fe82:816a"
    )]
    records: String,

    #[arg(long, env = "CAMERA_HUB_DDNS_TTL", default_value_t = 600)]
    ttl: u64,

    #[arg(long, env = "CAMERA_HUB_DDNS_INTERVAL_SECONDS", default_value_t = 60)]
    interval_seconds: u64,

    #[arg(long, env = "CAMERA_HUB_DDNS_FORCE_SECONDS", default_value_t = 21_600)]
    force_seconds: u64,

    #[arg(
        long,
        env = "CAMERA_HUB_DDNS_STATE_FILE",
        default_value = "/home/android/.config/camera-hub-ddns.state"
    )]
    state_file: PathBuf,

    #[arg(long, env = "CAMERA_HUB_DDNS_DRY_RUN", default_value_t = false)]
    dry_run: bool,

    #[arg(long, default_value_t = false)]
    once: bool,

    #[arg(long, hide = true, default_value = API_ENDPOINT)]
    endpoint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecordTarget {
    name: String,
    iid: u64,
}

#[derive(Clone, Debug)]
struct PlannedRecord {
    name: String,
    address: Ipv6Addr,
}

#[derive(Default, Deserialize, Serialize)]
struct DdnsState {
    prefix: String,
    last_verified_epoch: i64,
}

#[derive(Deserialize)]
struct RecordListItem {
    #[serde(rename = "RecordId")]
    record_id: u64,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Type")]
    record_type: String,
    #[serde(rename = "Line")]
    line: String,
    #[serde(rename = "Value")]
    value: String,
    #[serde(rename = "TTL")]
    ttl: u64,
}

struct DnspodClient {
    http: reqwest::Client,
    endpoint: String,
    secret_id: String,
    secret_key: String,
}

impl DnspodClient {
    fn new(args: &Args) -> Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("camera-hub-ddns/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("build DNSPod HTTP client")?;
        Ok(Self {
            http,
            endpoint: args.endpoint.clone(),
            secret_id: args.secret_id.clone(),
            secret_key: args.secret_key.clone(),
        })
    }

    async fn call(&self, action: &str, payload: Value) -> Result<Value> {
        let body = serde_json::to_string(&payload)?;
        let timestamp = epoch_seconds();
        let authorization =
            authorization(&self.secret_id, &self.secret_key, action, timestamp, &body)?;
        let response = self
            .http
            .post(&self.endpoint)
            .header(CONTENT_TYPE, CONTENT_TYPE_JSON)
            .header("X-TC-Action", action)
            .header("X-TC-Version", API_VERSION)
            .header("X-TC-Timestamp", timestamp)
            .header("Authorization", authorization)
            .body(body)
            .send()
            .await
            .with_context(|| format!("call DNSPod {action}"))?;
        let status = response.status();
        let text = response.text().await.context("read DNSPod response")?;
        let value = serde_json::from_str::<Value>(&text)
            .with_context(|| format!("decode DNSPod {action} response: {text}"))?;
        if !status.is_success() {
            bail!("DNSPod {action} returned HTTP {status}: {value}");
        }
        let response = value
            .get("Response")
            .context("DNSPod response is missing Response")?;
        if let Some(error) = response.get("Error") {
            let code = error
                .get("Code")
                .and_then(Value::as_str)
                .unwrap_or("Unknown");
            let message = error
                .get("Message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            bail!("DNSPod {action} failed: {code}: {message}");
        }
        Ok(response.clone())
    }

    async fn records(&self, domain: &str) -> Result<Vec<RecordListItem>> {
        let response = self
            .call(
                "DescribeRecordList",
                json!({
                    "Domain": domain,
                    "RecordType": "AAAA",
                    "RecordLine": "默认",
                    "Limit": 3000,
                    "ErrorOnEmpty": "no",
                }),
            )
            .await?;
        serde_json::from_value(
            response
                .get("RecordList")
                .cloned()
                .unwrap_or_else(|| json!([])),
        )
        .context("decode DNSPod record list")
    }

    async fn update(
        &self,
        domain: &str,
        current: &RecordListItem,
        target: &PlannedRecord,
        ttl: u64,
    ) -> Result<()> {
        self.call(
            "ModifyRecord",
            json!({
                "Domain": domain,
                "RecordId": current.record_id,
                "SubDomain": target.name,
                "RecordType": "AAAA",
                "RecordLine": current.line,
                "Value": target.address.to_string(),
                "TTL": ttl,
            }),
        )
        .await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args = Args::parse();
    validate_args(&args)?;
    let targets = parse_record_targets(&args.records)?;
    let stable_address = read_stable_global_ipv6(&args.interface)?;
    let prefix = ipv6_prefix_64(stable_address);
    let plan = compose_plan(prefix, &targets);

    if args.dry_run {
        print_plan(stable_address, prefix, &args.domain, &plan);
        return Ok(());
    }
    if args.secret_id.trim().is_empty() || args.secret_key.trim().is_empty() {
        bail!(
            "DNSPod credentials are missing; set CAMERA_HUB_DDNS_SECRET_ID and CAMERA_HUB_DDNS_SECRET_KEY"
        );
    }

    let client = DnspodClient::new(&args)?;
    let mut state = load_state(&args.state_file);
    let normal_delay = Duration::from_secs(args.interval_seconds);
    let mut retry_delay = normal_delay;

    loop {
        let result = sync_if_needed(&args, &targets, &client, &mut state).await;
        match result {
            Ok(SyncResult::Skipped) => retry_delay = normal_delay,
            Ok(SyncResult::Verified { changed }) => {
                log_line(&format!(
                    "DNSPod synchronization completed: changed={changed}"
                ));
                retry_delay = normal_delay;
            }
            Err(error) => {
                log_line(&format!("DNSPod synchronization failed: {error:#}"));
                if args.once {
                    return Err(error);
                }
                retry_delay = (retry_delay * 2).min(Duration::from_secs(900));
            }
        }

        if args.once {
            return Ok(());
        }
        tokio::select! {
            _ = tokio::time::sleep(retry_delay) => {}
            _ = tokio::signal::ctrl_c() => {
                log_line("shutdown requested");
                return Ok(());
            }
        }
    }
}

enum SyncResult {
    Skipped,
    Verified { changed: usize },
}

async fn sync_if_needed(
    args: &Args,
    targets: &[RecordTarget],
    client: &DnspodClient,
    state: &mut DdnsState,
) -> Result<SyncResult> {
    let stable_address = read_stable_global_ipv6(&args.interface)?;
    let prefix = ipv6_prefix_64(stable_address);
    let prefix_text = Ipv6Addr::from(prefix).to_string();
    let now = epoch_seconds();
    let force_due = now.saturating_sub(state.last_verified_epoch) >= args.force_seconds as i64;
    if !args.once && state.prefix == prefix_text && !force_due {
        return Ok(SyncResult::Skipped);
    }

    let plan = compose_plan(prefix, targets);
    log_line(&format!(
        "reconciling {} AAAA records from stable address {stable_address}",
        plan.len()
    ));
    let records = client.records(&args.domain).await?;
    let mut current_by_name = HashMap::<&str, &RecordListItem>::new();
    for record in &records {
        if record.record_type == "AAAA" && record.line == "默认" {
            if current_by_name
                .insert(record.name.as_str(), record)
                .is_some()
            {
                bail!(
                    "multiple default-line AAAA records exist for {}; remove duplicates first",
                    record.name
                );
            }
        }
    }
    for target in &plan {
        if !current_by_name.contains_key(target.name.as_str()) {
            bail!(
                "DNSPod AAAA record {}.{} does not exist",
                target.name,
                args.domain
            );
        }
    }

    let mut changed = 0usize;
    for target in &plan {
        let current = current_by_name[target.name.as_str()];
        let address_matches = current.value.parse::<Ipv6Addr>().ok() == Some(target.address);
        if address_matches && current.ttl == args.ttl {
            log_line(&format!(
                "{}.{} unchanged at {}",
                target.name, args.domain, target.address
            ));
            continue;
        }
        client
            .update(&args.domain, current, target, args.ttl)
            .await?;
        changed += 1;
        log_line(&format!(
            "{}.{} updated: {} -> {}",
            target.name, args.domain, current.value, target.address
        ));
    }

    state.prefix = prefix_text;
    state.last_verified_epoch = now;
    save_state(&args.state_file, state)?;
    Ok(SyncResult::Verified { changed })
}

fn validate_args(args: &Args) -> Result<()> {
    if args.domain.trim().is_empty() || args.domain.contains(char::is_whitespace) {
        bail!("invalid DNSPod domain");
    }
    if args.interface.trim().is_empty() {
        bail!("network interface is empty");
    }
    if !(1..=604_800).contains(&args.ttl) {
        bail!("TTL must be between 1 and 604800 seconds");
    }
    if args.interval_seconds < 10 {
        bail!("DDNS interval must be at least 10 seconds");
    }
    if args.force_seconds < args.interval_seconds {
        bail!("force interval must not be shorter than normal interval");
    }
    Ok(())
}

fn parse_record_targets(text: &str) -> Result<Vec<RecordTarget>> {
    let mut records = Vec::new();
    let mut names = HashSet::new();
    for item in text.split([',', ';', '\n']) {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let (name, iid) = item
            .split_once('=')
            .with_context(|| format!("invalid DDNS record mapping: {item}"))?;
        let name = name.trim();
        if !valid_record_name(name) {
            bail!("invalid DNS record name: {name}");
        }
        if !names.insert(name.to_ascii_lowercase()) {
            bail!("duplicate DNS record name: {name}");
        }
        let iid_text = iid.trim();
        let address = if iid_text.starts_with("::") {
            Ipv6Addr::from_str(iid_text)
        } else {
            Ipv6Addr::from_str(&format!("::{iid_text}"))
        }
        .with_context(|| format!("invalid IPv6 IID for {name}: {iid_text}"))?;
        let value = u128::from(address);
        if value >> 64 != 0 {
            bail!("record {name} must contain only the lower 64 IPv6 bits");
        }
        records.push(RecordTarget {
            name: name.to_owned(),
            iid: value as u64,
        });
    }
    if records.is_empty() {
        bail!("no DDNS record mappings configured");
    }
    Ok(records)
}

fn valid_record_name(name: &str) -> bool {
    name == "@"
        || (!name.is_empty()
            && name.len() <= 253
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')))
}

fn read_stable_global_ipv6(interface: &str) -> Result<Ipv6Addr> {
    let contents = fs::read_to_string("/proc/net/if_inet6").context("read /proc/net/if_inet6")?;
    stable_global_ipv6(&contents, interface)
}

fn stable_global_ipv6(contents: &str, interface: &str) -> Result<Ipv6Addr> {
    for line in contents.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 6 || fields[5] != interface {
            continue;
        }
        let value = u128::from_str_radix(fields[0], 16)
            .with_context(|| format!("invalid IPv6 address in /proc: {}", fields[0]))?;
        let prefix_len = u8::from_str_radix(fields[2], 16)?;
        let scope = u8::from_str_radix(fields[3], 16)?;
        let flags = u32::from_str_radix(fields[4], 16)?;
        let rejected_flags = ADDRESS_FLAG_TEMPORARY
            | ADDRESS_FLAG_DAD_FAILED
            | ADDRESS_FLAG_DEPRECATED
            | ADDRESS_FLAG_TENTATIVE;
        if prefix_len == 64 && scope == 0 && flags & rejected_flags == 0 && value >> 125 == 1 {
            return Ok(Ipv6Addr::from(value));
        }
    }
    bail!("no stable global /64 IPv6 address found on {interface}")
}

fn ipv6_prefix_64(address: Ipv6Addr) -> u128 {
    u128::from(address) & (u128::MAX << 64)
}

fn compose_plan(prefix: u128, records: &[RecordTarget]) -> Vec<PlannedRecord> {
    records
        .iter()
        .map(|record| PlannedRecord {
            name: record.name.clone(),
            address: Ipv6Addr::from(prefix | u128::from(record.iid)),
        })
        .collect()
}

fn print_plan(stable_address: Ipv6Addr, prefix: u128, domain: &str, plan: &[PlannedRecord]) {
    println!("stable_ipv6={stable_address}");
    println!("prefix={}/64", Ipv6Addr::from(prefix));
    for record in plan {
        println!(
            "{} AAAA {}",
            record_fqdn(&record.name, domain),
            record.address
        );
    }
}

fn record_fqdn(name: &str, domain: &str) -> String {
    if name == "@" {
        domain.to_owned()
    } else {
        format!("{name}.{domain}")
    }
}

fn authorization(
    secret_id: &str,
    secret_key: &str,
    action: &str,
    timestamp: i64,
    payload: &str,
) -> Result<String> {
    let date = DateTime::<Utc>::from_timestamp(timestamp, 0)
        .context("invalid API timestamp")?
        .format("%Y-%m-%d")
        .to_string();
    let canonical_headers = format!(
        "content-type:{CONTENT_TYPE_JSON}\nhost:{API_HOST}\nx-tc-action:{}\n",
        action.to_ascii_lowercase()
    );
    let hashed_payload = sha256_hex(payload.as_bytes());
    let canonical_request =
        format!("POST\n/\n\n{canonical_headers}\n{SIGNED_HEADERS}\n{hashed_payload}");
    let credential_scope = format!("{date}/{API_SERVICE}/tc3_request");
    let string_to_sign = format!(
        "TC3-HMAC-SHA256\n{timestamp}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let secret_date = hmac_sha256(format!("TC3{secret_key}").as_bytes(), date.as_bytes())?;
    let secret_service = hmac_sha256(&secret_date, API_SERVICE.as_bytes())?;
    let secret_signing = hmac_sha256(&secret_service, b"tc3_request")?;
    let signature = hex::encode(hmac_sha256(&secret_signing, string_to_sign.as_bytes())?);
    Ok(format!(
        "TC3-HMAC-SHA256 Credential={secret_id}/{credential_scope}, \
         SignedHeaders={SIGNED_HEADERS}, Signature={signature}"
    ))
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).context("initialize HMAC-SHA256")?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn load_state(path: &Path) -> DdnsState {
    fs::read(path)
        .ok()
        .and_then(|contents| serde_json::from_slice(&contents).ok())
        .unwrap_or_default()
}

fn save_state(path: &Path, state: &DdnsState) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create DDNS state directory {}", parent.display()))?;
    }
    let temporary = path.with_extension("state.new");
    fs::write(&temporary, serde_json::to_vec(state)?)
        .with_context(|| format!("write DDNS state {}", temporary.display()))?;
    fs::rename(&temporary, path)
        .with_context(|| format!("replace DDNS state {}", path.display()))?;
    Ok(())
}

fn epoch_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn log_line(message: &str) {
    eprintln!("{} {message}", Utc::now().format("%Y-%m-%dT%H:%M:%SZ"));
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROC_IPV6: &str = "\
00000000000000000000000000000001 01 80 10 80 lo
24098a1e7a52c9b0a1258e36f9cb955d 1a 40 00 21 wlan0
fe80000000000000528f4cfffeefdd90 1a 40 20 80 wlan0
24098a1e7a52c9b0528f4cfffeefdd90 1a 40 00 00 wlan0
";

    #[test]
    fn selects_stable_global_address_and_ignores_temporary_deprecated() {
        assert_eq!(
            stable_global_ipv6(PROC_IPV6, "wlan0").unwrap(),
            "2409:8a1e:7a52:c9b0:528f:4cff:feef:dd90"
                .parse::<Ipv6Addr>()
                .unwrap()
        );
    }

    #[test]
    fn composes_all_device_addresses_from_one_prefix() {
        let targets = parse_record_targets(
            "@=528f:4cff:feef:dd90,mi6=528f:4cff:feef:dd90,\
             v831=a22c:36ff:febd:4feb,lecoo=8647:09ff:fe45:35a0,\
             lecoo-wifi=72c9:12ff:fe1c:2f67,huawei=1a56:80ff:fe82:816a",
        )
        .unwrap();
        let prefix = ipv6_prefix_64("2409:8a1e:7a52:c9b0:528f:4cff:feef:dd90".parse().unwrap());
        let plan = compose_plan(prefix, &targets);
        assert_eq!(
            plan[0].address.to_string(),
            "2409:8a1e:7a52:c9b0:528f:4cff:feef:dd90"
        );
        assert_eq!(
            plan[2].address.to_string(),
            "2409:8a1e:7a52:c9b0:a22c:36ff:febd:4feb"
        );
        assert_eq!(
            plan[3].address.to_string(),
            "2409:8a1e:7a52:c9b0:8647:9ff:fe45:35a0"
        );
        assert_eq!(
            plan[4].address.to_string(),
            "2409:8a1e:7a52:c9b0:72c9:12ff:fe1c:2f67"
        );
        assert_eq!(
            plan[5].address.to_string(),
            "2409:8a1e:7a52:c9b0:1a56:80ff:fe82:816a"
        );
    }

    #[test]
    fn rejects_duplicate_names_and_full_addresses() {
        assert!(parse_record_targets("mi6=1,mi6=2").is_err());
        assert!(parse_record_targets("v831=2409:8a1e:7a52:c9b0:a22c:36ff:febd:4feb").is_err());
    }

    #[test]
    fn formats_root_and_subdomain_record_names() {
        assert_eq!(record_fqdn("@", "gwghome.site"), "gwghome.site");
        assert_eq!(record_fqdn("v831", "gwghome.site"), "v831.gwghome.site");
    }

    #[test]
    fn tc3_signature_matches_independent_vector() {
        let payload = serde_json::to_string(&json!({
            "Domain": "gwghome.site",
            "RecordType": "AAAA",
            "RecordLine": "默认",
            "Limit": 3000,
            "ErrorOnEmpty": "no",
        }))
        .unwrap();
        assert_eq!(
            sha256_hex(payload.as_bytes()),
            "88e64d5b7aa88032aa93cea2b292382c37917798d98578352774122da5afb3f4"
        );
        let auth = authorization(
            "test-id",
            "test-secret",
            "DescribeRecordList",
            1_786_924_800,
            &payload,
        )
        .unwrap();
        assert!(auth.ends_with(
            "Signature=513d518eaa6d386b86df3510c1720e8b77665f9577b52dda61fb9a939b85f288"
        ));
    }
}
