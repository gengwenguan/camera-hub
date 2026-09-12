use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
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

pub const CONFIG_VERSION: u32 = 1;
pub const STATUS_VERSION: u32 = 1;
pub const API_ENDPOINT: &str = "https://dnspod.tencentcloudapi.com/";

const API_HOST: &str = "dnspod.tencentcloudapi.com";
const API_SERVICE: &str = "dnspod";
const API_VERSION: &str = "2021-03-23";
const CONTENT_TYPE_JSON: &str = "application/json; charset=utf-8";
const SIGNED_HEADERS: &str = "content-type;host;x-tc-action";
const ADDRESS_FLAG_TEMPORARY: u32 = 0x01;
const ADDRESS_FLAG_DAD_FAILED: u32 = 0x08;
const ADDRESS_FLAG_DEPRECATED: u32 = 0x20;
const ADDRESS_FLAG_TENTATIVE: u32 = 0x40;

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DdnsConfig {
    pub version: u32,
    pub revision: u64,
    pub enabled: bool,
    pub domain: String,
    pub secret_id: String,
    pub secret_key: String,
    pub interface: String,
    pub ttl: u64,
    pub interval_seconds: u64,
    pub force_seconds: u64,
    pub records: Vec<DdnsRecordConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct DdnsRecordConfig {
    pub name: String,
    pub iid: String,
}

#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct DdnsConfigUpdate {
    pub revision: u64,
    pub enabled: bool,
    pub domain: String,
    pub secret_id: String,
    pub secret_key: String,
    pub clear_secret: bool,
    pub interface: String,
    pub ttl: u64,
    pub interval_seconds: u64,
    pub force_seconds: u64,
    pub records: Vec<DdnsRecordConfig>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DdnsPublicConfig {
    pub version: u32,
    pub revision: u64,
    pub enabled: bool,
    pub domain: String,
    pub secret_id: String,
    pub secret_key_configured: bool,
    pub interface: String,
    pub ttl: u64,
    pub interval_seconds: u64,
    pub force_seconds: u64,
    pub records: Vec<DdnsRecordConfig>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct DdnsPreviewRequest {
    pub domain: String,
    pub interface: String,
    pub records: Vec<DdnsRecordConfig>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DdnsPreview {
    pub stable_ipv6: String,
    pub prefix: String,
    pub records: Vec<DdnsPlannedRecord>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DdnsPlannedRecord {
    pub name: String,
    pub fqdn: String,
    pub address: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct DdnsStatus {
    pub version: u32,
    pub config_revision: u64,
    pub pid: u32,
    pub updated_epoch: u64,
    pub state: String,
    pub detail: String,
    pub stable_ipv6: String,
    pub prefix: String,
    pub last_attempt_epoch: u64,
    pub last_success_epoch: u64,
    pub next_attempt_epoch: u64,
    pub changed_count: u64,
    pub consecutive_failures: u64,
    pub last_error: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(default)]
struct DdnsState {
    prefix: String,
    config_fingerprint: String,
    last_verified_epoch: i64,
}

#[derive(Clone, Debug)]
struct RecordTarget {
    name: String,
    iid: u64,
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

#[derive(Clone, Debug)]
pub struct DdnsSyncResult {
    pub stable_ipv6: String,
    pub prefix: String,
    pub changed: usize,
    pub verified: bool,
    pub last_verified_epoch: u64,
}

pub struct DdnsConfigStore {
    path: PathBuf,
    current: DdnsConfig,
}

pub struct DdnsLegacyConfig {
    pub enabled: bool,
    pub domain: String,
    pub secret_id: String,
    pub secret_key: String,
    pub interface: String,
    pub records: String,
    pub ttl: u64,
    pub interval_seconds: u64,
    pub force_seconds: u64,
}

struct DnspodClient {
    http: reqwest::Client,
    endpoint: String,
    secret_id: String,
    secret_key: String,
}

impl Default for DdnsConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            revision: 1,
            enabled: false,
            domain: "gwghome.site".to_owned(),
            secret_id: String::new(),
            secret_key: String::new(),
            interface: "wlan0".to_owned(),
            ttl: 600,
            interval_seconds: 60,
            force_seconds: 21_600,
            records: default_records(),
        }
    }
}

impl Default for DdnsRecordConfig {
    fn default() -> Self {
        Self {
            name: "@".to_owned(),
            iid: String::new(),
        }
    }
}

impl Default for DdnsStatus {
    fn default() -> Self {
        Self {
            version: STATUS_VERSION,
            config_revision: 0,
            pid: 0,
            updated_epoch: 0,
            state: "not_started".to_owned(),
            detail: "DDNS 进程尚未报告状态".to_owned(),
            stable_ipv6: String::new(),
            prefix: String::new(),
            last_attempt_epoch: 0,
            last_success_epoch: 0,
            next_attempt_epoch: 0,
            changed_count: 0,
            consecutive_failures: 0,
            last_error: String::new(),
        }
    }
}

impl DdnsConfig {
    pub fn normalize(mut self) -> Result<Self> {
        self.version = CONFIG_VERSION;
        self.domain = normalize_domain(&self.domain)?;
        self.secret_id = clean_secret(&self.secret_id, "DNSPod SecretId")?;
        self.secret_key = clean_secret(&self.secret_key, "DNSPod SecretKey")?;
        self.interface = normalize_interface(&self.interface)?;
        if !(1..=604_800).contains(&self.ttl) {
            bail!("TTL 必须在 1 到 604800 秒之间");
        }
        if !(10..=86_400).contains(&self.interval_seconds) {
            bail!("检查周期必须在 10 到 86400 秒之间");
        }
        if self.force_seconds < self.interval_seconds || self.force_seconds > 2_592_000 {
            bail!("强制对账周期必须大于检查周期且不超过 30 天");
        }
        self.records = normalize_records(self.records)?;
        Ok(self)
    }

    pub fn readiness_error(&self) -> Option<String> {
        if self.secret_id.is_empty() || self.secret_key.is_empty() {
            return Some("启用 DDNS 前必须配置 DNSPod SecretId 和 SecretKey".to_owned());
        }
        if self.records.is_empty() {
            return Some("启用 DDNS 前必须至少配置一条 AAAA 记录".to_owned());
        }
        None
    }

    pub fn public(&self) -> DdnsPublicConfig {
        DdnsPublicConfig {
            version: self.version,
            revision: self.revision,
            enabled: self.enabled,
            domain: self.domain.clone(),
            secret_id: self.secret_id.clone(),
            secret_key_configured: !self.secret_key.is_empty(),
            interface: self.interface.clone(),
            ttl: self.ttl,
            interval_seconds: self.interval_seconds,
            force_seconds: self.force_seconds,
            records: self.records.clone(),
        }
    }

    pub fn apply_update(&self, update: DdnsConfigUpdate) -> Result<Self> {
        if update.revision != 0 && update.revision != self.revision {
            bail!("DDNS 配置已被其他会话修改，请重新加载");
        }
        let mut next = self.clone();
        next.enabled = update.enabled;
        let secret_id = clean_secret(&update.secret_id, "DNSPod SecretId")?;
        if secret_id != next.secret_id && update.secret_key.trim().is_empty() {
            next.secret_key.clear();
        }
        next.secret_id = secret_id;
        if update.clear_secret {
            next.secret_key.clear();
            next.enabled = false;
        } else if !update.secret_key.trim().is_empty() {
            next.secret_key = update.secret_key;
        }
        next.domain = update.domain;
        next.interface = update.interface;
        next.ttl = update.ttl;
        next.interval_seconds = update.interval_seconds;
        next.force_seconds = update.force_seconds;
        next.records = update.records;
        next.revision = next.revision.saturating_add(1);
        next = next.normalize()?;
        if next.enabled
            && let Some(error) = next.readiness_error()
        {
            bail!(error);
        }
        Ok(next)
    }

    pub fn from_legacy(legacy: DdnsLegacyConfig) -> Result<Self> {
        Self {
            enabled: legacy.enabled,
            domain: legacy.domain,
            secret_id: legacy.secret_id,
            secret_key: legacy.secret_key,
            interface: legacy.interface,
            ttl: legacy.ttl,
            interval_seconds: legacy.interval_seconds,
            force_seconds: legacy.force_seconds,
            records: parse_legacy_records(&legacy.records)?,
            ..Self::default()
        }
        .normalize()
    }
}

impl DdnsConfigStore {
    pub fn load(path: PathBuf) -> Result<Self> {
        let current = match fs::read(&path) {
            Ok(data) => serde_json::from_slice::<DdnsConfig>(&data)
                .with_context(|| format!("解析 DDNS 配置 {}", path.display()))?
                .normalize()?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                DdnsConfig::default().normalize()?
            }
            Err(error) => return Err(error.into()),
        };
        let store = Self { path, current };
        store.save(&store.current)?;
        Ok(store)
    }

    pub fn current(&self) -> DdnsConfig {
        self.current.clone()
    }

    pub fn update(&mut self, update: DdnsConfigUpdate) -> Result<DdnsPublicConfig> {
        let next = self.current.apply_update(update)?;
        self.save(&next)?;
        self.current = next;
        Ok(self.current.public())
    }

    pub fn request_reconcile(&mut self) -> Result<u64> {
        if !self.current.enabled {
            bail!("DDNS 尚未启用");
        }
        if let Some(error) = self.current.readiness_error() {
            bail!(error);
        }
        let mut next = self.current.clone();
        next.revision = next.revision.saturating_add(1);
        self.save(&next)?;
        self.current = next;
        Ok(self.current.revision)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn save(&self, config: &DdnsConfig) -> Result<()> {
        save_config(&self.path, config)
    }
}

impl DnspodClient {
    fn new(config: &DdnsConfig, endpoint: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("camera-hub-ddns/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("创建 DNSPod HTTP 客户端")?;
        Ok(Self {
            http,
            endpoint: endpoint.to_owned(),
            secret_id: config.secret_id.clone(),
            secret_key: config.secret_key.clone(),
        })
    }

    async fn call(&self, action: &str, payload: Value) -> Result<Value> {
        let body = serde_json::to_string(&payload)?;
        let timestamp = epoch_seconds() as i64;
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
            .with_context(|| format!("调用 DNSPod {action}"))?;
        let status = response.status();
        let text = response.text().await.context("读取 DNSPod 响应")?;
        let value = serde_json::from_str::<Value>(&text)
            .with_context(|| format!("解析 DNSPod {action} 响应: {text}"))?;
        if !status.is_success() {
            bail!("DNSPod {action} 返回 HTTP {status}: {value}");
        }
        let response = value.get("Response").context("DNSPod 响应缺少 Response")?;
        if let Some(error) = response.get("Error") {
            let code = error
                .get("Code")
                .and_then(Value::as_str)
                .unwrap_or("Unknown");
            let message = error
                .get("Message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            bail!("DNSPod {action} 失败: {code}: {message}");
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
        .context("解析 DNSPod 记录列表")
    }

    async fn update(
        &self,
        domain: &str,
        current: &RecordListItem,
        target: &DdnsPlannedRecord,
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
                "Value": target.address,
                "TTL": ttl,
            }),
        )
        .await?;
        Ok(())
    }
}

pub fn load_config(path: &Path) -> Result<DdnsConfig> {
    let data = fs::read(path).with_context(|| format!("读取 DDNS 配置 {}", path.display()))?;
    serde_json::from_slice::<DdnsConfig>(&data)
        .with_context(|| format!("解析 DDNS 配置 {}", path.display()))?
        .normalize()
}

pub fn save_config(path: &Path, config: &DdnsConfig) -> Result<()> {
    write_secure_json(path, config)
}

pub fn load_status(path: &Path) -> Result<DdnsStatus> {
    let data = fs::read(path).with_context(|| format!("读取 DDNS 状态 {}", path.display()))?;
    serde_json::from_slice(&data).with_context(|| format!("解析 DDNS 状态 {}", path.display()))
}

pub fn save_status(path: &Path, status: &DdnsStatus) -> Result<()> {
    write_secure_json(path, status)
}

pub fn preview(config: &DdnsConfig) -> Result<DdnsPreview> {
    preview_values(&config.domain, &config.interface, &config.records)
}

pub fn preview_request(request: DdnsPreviewRequest) -> Result<DdnsPreview> {
    preview_values(&request.domain, &request.interface, &request.records)
}

fn preview_values(
    domain: &str,
    interface: &str,
    records: &[DdnsRecordConfig],
) -> Result<DdnsPreview> {
    let domain = normalize_domain(domain)?;
    let interface = normalize_interface(interface)?;
    let records = normalize_records(records.to_vec())?;
    if records.is_empty() {
        bail!("至少需要一条 DDNS 记录");
    }
    let targets = record_targets(&records)?;
    let stable_address = read_stable_global_ipv6(&interface)?;
    let prefix = ipv6_prefix_64(stable_address);
    Ok(DdnsPreview {
        stable_ipv6: stable_address.to_string(),
        prefix: format!("{}/64", Ipv6Addr::from(prefix)),
        records: compose_plan(prefix, &domain, &targets),
    })
}

pub async fn reconcile(
    config: &DdnsConfig,
    state_path: &Path,
    force: bool,
) -> Result<DdnsSyncResult> {
    reconcile_with_endpoint(config, state_path, force, API_ENDPOINT).await
}

pub async fn reconcile_with_endpoint(
    config: &DdnsConfig,
    state_path: &Path,
    force: bool,
    endpoint: &str,
) -> Result<DdnsSyncResult> {
    let config = config.clone().normalize()?;
    if let Some(error) = config.readiness_error() {
        bail!(error);
    }
    let targets = record_targets(&config.records)?;
    let stable_address = read_stable_global_ipv6(&config.interface)?;
    let prefix = ipv6_prefix_64(stable_address);
    let prefix_text = Ipv6Addr::from(prefix).to_string();
    let now = epoch_seconds() as i64;
    let mut state = load_state(state_path);
    let fingerprint = config_fingerprint(&config)?;
    let force_due = now.saturating_sub(state.last_verified_epoch) >= config.force_seconds as i64;
    if !force
        && state.prefix == prefix_text
        && state.config_fingerprint == fingerprint
        && !force_due
    {
        return Ok(DdnsSyncResult {
            stable_ipv6: stable_address.to_string(),
            prefix: format!("{prefix_text}/64"),
            changed: 0,
            verified: false,
            last_verified_epoch: state.last_verified_epoch.max(0) as u64,
        });
    }

    let plan = compose_plan(prefix, &config.domain, &targets);
    let client = DnspodClient::new(&config, endpoint)?;
    let records = client.records(&config.domain).await?;
    let mut current_by_name = HashMap::<&str, &RecordListItem>::new();
    for record in &records {
        if record.record_type == "AAAA"
            && record.line == "默认"
            && current_by_name
                .insert(record.name.as_str(), record)
                .is_some()
        {
            bail!(
                "{} 存在多条默认线路 AAAA 记录，请先删除重复记录",
                record.name
            );
        }
    }
    for target in &plan {
        if !current_by_name.contains_key(target.name.as_str()) {
            bail!("DNSPod AAAA 记录 {} 不存在", target.fqdn);
        }
    }

    let mut changed = 0usize;
    for target in &plan {
        let current = current_by_name[target.name.as_str()];
        let address_matches = current
            .value
            .parse::<Ipv6Addr>()
            .ok()
            .map(|value| value.to_string())
            == Some(target.address.clone());
        if address_matches && current.ttl == config.ttl {
            continue;
        }
        client
            .update(&config.domain, current, target, config.ttl)
            .await?;
        changed += 1;
    }

    state.prefix = prefix_text.clone();
    state.config_fingerprint = fingerprint;
    state.last_verified_epoch = now;
    save_state(state_path, &state)?;
    Ok(DdnsSyncResult {
        stable_ipv6: stable_address.to_string(),
        prefix: format!("{prefix_text}/64"),
        changed,
        verified: true,
        last_verified_epoch: now.max(0) as u64,
    })
}

pub fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn normalize_domain(value: &str) -> Result<String> {
    let domain = value.trim().trim_end_matches('.').to_ascii_lowercase();
    if domain.is_empty()
        || domain.len() > 253
        || domain.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        bail!("DNSPod 主域名格式无效");
    }
    Ok(domain)
}

fn normalize_interface(value: &str) -> Result<String> {
    let interface = value.trim();
    if interface.is_empty()
        || interface.len() > 64
        || !interface
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        bail!("网卡名称格式无效");
    }
    Ok(interface.to_owned())
}

fn clean_secret(value: &str, label: &str) -> Result<String> {
    let value = value.trim();
    if value.len() > 256 || value.chars().any(char::is_control) {
        bail!("{label} 格式无效");
    }
    Ok(value.to_owned())
}

fn normalize_records(records: Vec<DdnsRecordConfig>) -> Result<Vec<DdnsRecordConfig>> {
    let mut normalized = Vec::with_capacity(records.len());
    let mut names = HashSet::new();
    for record in records {
        let name = record.name.trim().to_ascii_lowercase();
        if !valid_record_name(&name) {
            bail!("DNS 记录名称格式无效: {name}");
        }
        if !names.insert(name.clone()) {
            bail!("DNS 记录名称重复: {name}");
        }
        let iid = parse_iid(&record.iid)
            .with_context(|| format!("IPv6 后 64 位格式无效: {}", record.iid.trim()))?;
        normalized.push(DdnsRecordConfig {
            name,
            iid: format_iid(iid),
        });
    }
    Ok(normalized)
}

fn parse_legacy_records(text: &str) -> Result<Vec<DdnsRecordConfig>> {
    let records = text
        .split([',', ';', '\n'])
        .filter_map(|item| {
            let item = item.trim();
            if item.is_empty() {
                return None;
            }
            Some(
                item.split_once('=')
                    .map(|(name, iid)| DdnsRecordConfig {
                        name: name.to_owned(),
                        iid: iid.to_owned(),
                    })
                    .with_context(|| format!("DDNS 记录映射格式无效: {item}")),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    normalize_records(records)
}

fn record_targets(records: &[DdnsRecordConfig]) -> Result<Vec<RecordTarget>> {
    records
        .iter()
        .map(|record| {
            Ok(RecordTarget {
                name: record.name.clone(),
                iid: parse_iid(&record.iid)?,
            })
        })
        .collect()
}

fn parse_iid(text: &str) -> Result<u64> {
    let text = text.trim();
    if text.is_empty() {
        bail!("IPv6 后 64 位不能为空");
    }
    let address = if text.starts_with("::") {
        Ipv6Addr::from_str(text)
    } else {
        Ipv6Addr::from_str(&format!("::{text}"))
    }?;
    let value = u128::from(address);
    if value >> 64 != 0 {
        bail!("只能配置 IPv6 地址的后 64 位");
    }
    Ok(value as u64)
}

fn format_iid(value: u64) -> String {
    if value == 0 {
        return "0".to_owned();
    }
    Ipv6Addr::from(u128::from(value))
        .to_string()
        .trim_start_matches("::")
        .to_owned()
}

fn valid_record_name(name: &str) -> bool {
    name == "@"
        || (!name.is_empty()
            && name.len() <= 253
            && name.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            }))
}

fn read_stable_global_ipv6(interface: &str) -> Result<Ipv6Addr> {
    let contents = fs::read_to_string("/proc/net/if_inet6").context("读取 /proc/net/if_inet6")?;
    stable_global_ipv6(&contents, interface)
}

fn stable_global_ipv6(contents: &str, interface: &str) -> Result<Ipv6Addr> {
    for line in contents.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 6 || fields[5] != interface {
            continue;
        }
        let value = u128::from_str_radix(fields[0], 16)
            .with_context(|| format!("/proc 中的 IPv6 地址无效: {}", fields[0]))?;
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
    bail!("网卡 {interface} 上没有稳定的公网 /64 IPv6 地址")
}

fn ipv6_prefix_64(address: Ipv6Addr) -> u128 {
    u128::from(address) & (u128::MAX << 64)
}

fn compose_plan(prefix: u128, domain: &str, records: &[RecordTarget]) -> Vec<DdnsPlannedRecord> {
    records
        .iter()
        .map(|record| DdnsPlannedRecord {
            name: record.name.clone(),
            fqdn: record_fqdn(&record.name, domain),
            address: Ipv6Addr::from(prefix | u128::from(record.iid)).to_string(),
        })
        .collect()
}

fn record_fqdn(name: &str, domain: &str) -> String {
    if name == "@" {
        domain.to_owned()
    } else {
        format!("{name}.{domain}")
    }
}

fn default_records() -> Vec<DdnsRecordConfig> {
    parse_legacy_records(
        "@=528f:4cff:feef:dd90,mi6=528f:4cff:feef:dd90,\
         v831=a22c:36ff:febd:4feb,lecoo=8647:09ff:fe45:35a0,\
         lecoo-wifi=72c9:12ff:fe1c:2f67,huawei=1a56:80ff:fe82:816a",
    )
    .expect("default DDNS records")
}

fn config_fingerprint(config: &DdnsConfig) -> Result<String> {
    let value = json!({
        "version": config.version,
        "revision": config.revision,
        "domain": config.domain,
        "secret_id": config.secret_id,
        "interface": config.interface,
        "ttl": config.ttl,
        "records": config.records,
    });
    Ok(sha256_hex(&serde_json::to_vec(&value)?))
}

fn authorization(
    secret_id: &str,
    secret_key: &str,
    action: &str,
    timestamp: i64,
    payload: &str,
) -> Result<String> {
    let date = DateTime::<Utc>::from_timestamp(timestamp, 0)
        .context("API 时间戳无效")?
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
    let mut mac = Hmac::<Sha256>::new_from_slice(key).context("初始化 HMAC-SHA256")?;
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
    write_secure_json(path, state)
}

fn write_secure_json(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("创建 DDNS 目录 {}", parent.display()))?;
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("写入临时 DDNS 文件 {}", temporary.display()))?;
    secure_file(&temporary)?;
    fs::rename(&temporary, path).with_context(|| format!("替换 DDNS 文件 {}", path.display()))?;
    secure_file(path)?;
    Ok(())
}

fn secure_file(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
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
        let config = DdnsConfig::default();
        let targets = record_targets(&config.records).unwrap();
        let prefix = ipv6_prefix_64("2409:8a1e:7a52:c9b0:528f:4cff:feef:dd90".parse().unwrap());
        let plan = compose_plan(prefix, &config.domain, &targets);
        assert_eq!(plan[0].address, "2409:8a1e:7a52:c9b0:528f:4cff:feef:dd90");
        assert_eq!(plan[2].address, "2409:8a1e:7a52:c9b0:a22c:36ff:febd:4feb");
        assert_eq!(plan[3].address, "2409:8a1e:7a52:c9b0:8647:9ff:fe45:35a0");
        assert_eq!(plan[4].address, "2409:8a1e:7a52:c9b0:72c9:12ff:fe1c:2f67");
        assert_eq!(plan[5].address, "2409:8a1e:7a52:c9b0:1a56:80ff:fe82:816a");
    }

    #[test]
    fn rejects_duplicate_names_and_full_addresses() {
        assert!(parse_legacy_records("mi6=1,mi6=2").is_err());
        assert!(parse_legacy_records("v831=2409:8a1e:7a52:c9b0:a22c:36ff:febd:4feb").is_err());
    }

    #[test]
    fn keeps_secret_write_only_and_invalidates_it_when_id_changes() {
        let current = DdnsConfig {
            secret_id: "old-id".to_owned(),
            secret_key: "old-key".to_owned(),
            ..DdnsConfig::default()
        };
        let public_json = serde_json::to_string(&current.public()).unwrap();
        assert!(!public_json.contains("old-key"));
        let update = DdnsConfigUpdate {
            revision: current.revision,
            enabled: false,
            domain: current.domain.clone(),
            secret_id: "new-id".to_owned(),
            interface: current.interface.clone(),
            ttl: current.ttl,
            interval_seconds: current.interval_seconds,
            force_seconds: current.force_seconds,
            records: current.records.clone(),
            ..DdnsConfigUpdate::default()
        };
        let updated = current.apply_update(update).unwrap();
        assert!(updated.secret_key.is_empty());
        assert!(!updated.public().secret_key_configured);
    }

    #[test]
    fn config_store_persists_with_restricted_permissions() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "camera-hub-ddns-config-{}-{nonce}",
            std::process::id()
        ));
        let path = root.join("ddns.json");
        let store = DdnsConfigStore::load(path.clone()).unwrap();
        assert_eq!(store.current().revision, 1);
        assert!(path.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(root).unwrap();
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
