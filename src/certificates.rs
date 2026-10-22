use crate::atomic_file;
use crate::certificate_edge::{self, EdgeCertificateConfig};
use crate::config::Config;
use anyhow::{Context, Result, bail};
use axum::http::{Request, Response};
use axum_server::tls_rustls::RustlsConfig;
use bytes::Bytes;
use camera_hub::ddns::{DdnsConfigStore, DnspodClient, epoch_seconds};
use http_body_util::{BodyExt, Full};
use instant_acme::{
    Account, AuthorizationStatus, BodyWrapper, BytesResponse, ChallengeType, HttpClient,
    Identifier, NewAccount, NewOrder, OrderStatus, RetryPolicy,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{Notify, RwLock};
use tracing::{info, warn};
use x509_parser::extensions::GeneralName;

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CertificateConfig {
    pub enabled: bool,
    pub email: String,
    pub local_domain: String,
    pub edge: Option<EdgeCertificateConfig>,
    pub directory: String,
    pub dnspod_endpoint: String,
    pub interval_seconds: u64,
    pub retry_seconds: u64,
    pub dns_wait_seconds: u64,
}

impl Default for CertificateConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            email: String::new(),
            local_domain: String::new(),
            edge: None,
            directory: "https://acme-v02.api.letsencrypt.org/directory".to_owned(),
            dnspod_endpoint: "https://dnspod.tencentcloudapi.com".to_owned(),
            interval_seconds: 43_200,
            retry_seconds: 300,
            dns_wait_seconds: 60,
        }
    }
}

impl CertificateConfig {
    fn validate(&self) -> Result<()> {
        for domain in std::iter::once(self.local_domain.as_str())
            .chain(self.edge.iter().map(|edge| edge.domain.as_str()))
            .filter(|domain| !domain.is_empty())
        {
            if domain.len() > 253
                || domain.split('.').any(|label| {
                    label.is_empty()
                        || label.starts_with('-')
                        || label.ends_with('-')
                        || !label
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                })
            {
                bail!("无效的证书域名");
            }
        }
        if self.enabled && self.local_domain.is_empty() && self.edge.is_none() {
            bail!("至少配置一个证书域名");
        }
        if let Some(edge) = &self.edge {
            certificate_edge::validate(edge)?;
            if edge.domain.is_empty() || edge.domain == self.local_domain {
                bail!("V831 证书需要独立域名");
            }
        }
        for endpoint in [&self.directory, &self.dnspod_endpoint] {
            let url = reqwest::Url::parse(endpoint)?;
            if !matches!(url.scheme(), "http" | "https") {
                bail!("证书服务端点必须是 HTTP(S)");
            }
        }
        if !(10..=86_400).contains(&self.interval_seconds)
            || !(10..=3600).contains(&self.retry_seconds)
            || self.dns_wait_seconds > 300
        {
            bail!("证书检查、重试或 DNS 等待间隔超出范围");
        }
        Ok(())
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct CertificateStatus {
    domain: String,
    not_after_epoch: u64,
    renew_at_epoch: u64,
    last_success_epoch: u64,
    last_error: Option<String>,
    renewed: bool,
    reloaded: bool,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
struct Status {
    checking: bool,
    last_check_epoch: u64,
    next_attempt_epoch: u64,
    last_error: Option<String>,
    certificates: Vec<CertificateStatus>,
}

#[derive(Deserialize, Serialize)]
struct Bundle {
    chain: String,
    key: String,
}

#[derive(Deserialize, Serialize)]
struct ChallengeRecord {
    zone: String,
    id: u64,
}

pub struct CertificateService {
    hub: Config,
    directory: PathBuf,
    config: Mutex<CertificateConfig>,
    status: Mutex<Status>,
    tls: RwLock<Option<RustlsConfig>>,
    wake: Notify,
    force: AtomicBool,
}

impl CertificateService {
    pub async fn load(hub: &Config) -> Result<Arc<Self>> {
        let directory = hub.certificates_config_file.with_extension("state");
        let loaded =
            read_json::<CertificateConfig>(&hub.certificates_config_file).and_then(|config| {
                let config = config.unwrap_or_default();
                config.validate()?;
                Ok(config)
            });
        let config = loaded.unwrap_or_else(|error| {
            warn!(%error, "证书配置无效，暂时关闭自动续期");
            CertificateConfig::default()
        });
        let mut status: Status = read_json(&directory.join("status.json"))
            .unwrap_or_else(|error| {
                warn!(%error, "读取证书状态失败");
                None
            })
            .unwrap_or_default();
        status.checking = false;
        let service = Arc::new(Self {
            hub: hub.clone(),
            directory,
            config: Mutex::new(config.clone()),
            status: Mutex::new(status),
            tls: RwLock::new(None),
            wake: Notify::new(),
            force: AtomicBool::new(false),
        });
        // The bundle is authoritative if a previous process stopped between the two PEM renames.
        if !config.local_domain.is_empty() {
            match service.cached(&config.local_domain) {
                Ok(Some(bundle)) => {
                    if let Err(error) = service.deploy_local(&config.local_domain, &bundle).await {
                        warn!(%error, "恢复本机证书缓存失败");
                    }
                }
                Ok(None) => {}
                Err(error) => warn!(%error, "读取本机证书缓存失败"),
            }
        }
        if service.tls.read().await.is_none() && hub.tls_cert.is_file() && hub.tls_key.is_file() {
            *service.tls.write().await =
                Some(RustlsConfig::from_pem_file(&hub.tls_cert, &hub.tls_key).await?);
        }
        Ok(service)
    }

    pub async fn tls_config(&self) -> Option<RustlsConfig> {
        self.tls.read().await.clone()
    }

    pub fn overview(&self) -> Value {
        json!({
            "config": self.config.lock().unwrap().clone(),
            "status": self.status.lock().unwrap().clone(),
        })
    }

    pub fn update(&self, config: CertificateConfig) -> Result<()> {
        config.validate()?;
        let mut current = self.config.lock().unwrap();
        atomic_file::write_json(&self.hub.certificates_config_file, &config)?;
        *current = config;
        self.wake.notify_one();
        Ok(())
    }

    pub fn request_check(&self, force: bool) {
        if force {
            self.force.store(true, Ordering::Relaxed);
        }
        self.wake.notify_one();
    }

    pub fn spawn(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            loop {
                let config = service.config.lock().unwrap().clone();
                let force = service.force.swap(false, Ordering::Relaxed);
                let failed = if config.enabled {
                    service.check(&config, force).await
                } else {
                    let mut status = service.status.lock().unwrap();
                    status.next_attempt_epoch = 0;
                    false
                };
                let delay = if failed {
                    config.retry_seconds
                } else {
                    config.interval_seconds
                };
                tokio::select! {
                    _ = service.wake.notified() => {},
                    _ = tokio::time::sleep(Duration::from_secs(delay)) => {},
                }
            }
        })
    }

    async fn check(&self, config: &CertificateConfig, force: bool) -> bool {
        {
            let mut status = self.status.lock().unwrap();
            status.checking = true;
            status.last_check_epoch = epoch_seconds();
            status.last_error = None;
        }
        let mut failed = false;
        let mut statuses = Vec::new();
        for (domain, edge) in std::iter::once((config.local_domain.as_str(), None))
            .chain(
                config
                    .edge
                    .iter()
                    .map(|edge| (edge.domain.as_str(), Some(edge))),
            )
            .filter(|(domain, _)| !domain.is_empty())
        {
            let previous = self
                .status
                .lock()
                .unwrap()
                .certificates
                .iter()
                .find(|status| status.domain == domain)
                .cloned()
                .unwrap_or_default();
            let mut status = CertificateStatus {
                domain: domain.to_owned(),
                renewed: false,
                reloaded: false,
                ..previous
            };
            let result = tokio::time::timeout(
                Duration::from_secs(600),
                self.check_target(config, domain, edge, force, &mut status),
            )
            .await;
            match result {
                Ok(Ok(())) => {
                    status.last_error = None;
                    status.last_success_epoch = epoch_seconds();
                }
                error => {
                    failed = true;
                    let message = match error {
                        Ok(Err(error)) => format!("{error:#}"),
                        Err(_) => "证书续期操作超时".to_owned(),
                        Ok(Ok(())) => unreachable!(),
                    };
                    warn!(%domain, error = %message, "证书检查失败，将自动重试");
                    status.last_error = Some(message);
                }
            }
            statuses.push(status);
        }
        let mut status = self.status.lock().unwrap();
        status.checking = false;
        status.certificates = statuses;
        status.next_attempt_epoch = epoch_seconds()
            + if failed {
                config.retry_seconds
            } else {
                config.interval_seconds
            };
        if let Err(error) = atomic_file::write_json(&self.directory.join("status.json"), &*status) {
            warn!(%error, "保存证书状态失败");
        }
        failed
    }

    fn cached(&self, domain: &str) -> Result<Option<Bundle>> {
        read_json(&self.directory.join(format!("{domain}.json")))
    }

    async fn check_target(
        &self,
        config: &CertificateConfig,
        domain: &str,
        edge: Option<&EdgeCertificateConfig>,
        force: bool,
        status: &mut CertificateStatus,
    ) -> Result<()> {
        let mut bundle = self.cached(domain)?;
        if bundle.is_none()
            && edge.is_none()
            && self.hub.tls_cert.is_file()
            && self.hub.tls_key.is_file()
        {
            bundle = Some(Bundle {
                chain: fs::read_to_string(&self.hub.tls_cert)?,
                key: fs::read_to_string(&self.hub.tls_key)?,
            });
        }
        if let Some(current) = &bundle {
            (status.not_after_epoch, status.renew_at_epoch) =
                certificate_dates(domain, &current.chain)?;
        }
        let dns = DdnsConfigStore::load(self.hub.ddns_config_file.clone())?;
        let dns = dns.current();
        let client = DnspodClient::new(&dns, &config.dnspod_endpoint)?;
        let journal = self.directory.join(format!("{domain}.challenge.json"));
        // Do this even when the certificate isn't due: recover records left by cancellation/crash.
        cleanup(&client, &journal).await?;
        if force || bundle.is_none() || epoch_seconds() >= status.renew_at_epoch {
            if dns.secret_id.is_empty() || dns.secret_key.is_empty() {
                bail!("请先配置 DNSPod SecretId 和 SecretKey");
            }
            let issued = self
                .issue(
                    config,
                    domain,
                    edge.is_some(),
                    &dns.domain,
                    &client,
                    &journal,
                )
                .await?;
            (status.not_after_epoch, status.renew_at_epoch) =
                certificate_dates(domain, &issued.chain)?;
            RustlsConfig::from_pem(
                issued.chain.as_bytes().to_vec(),
                issued.key.as_bytes().to_vec(),
            )
            .await
            .context("验证新证书与私钥")?;
            atomic_file::write_json(&self.directory.join(format!("{domain}.json")), &issued)?;
            status.renewed = true;
            bundle = Some(issued);
        }
        let bundle = bundle.context("没有可部署的证书")?;
        status.reloaded = if let Some(edge) = edge {
            certificate_edge::synchronize(edge, &bundle.chain, &bundle.key).await?
        } else {
            self.deploy_local(domain, &bundle).await?
        };
        info!(%domain, expires = status.not_after_epoch, renewed = status.renewed,
            reloaded = status.reloaded, "证书检查完成");
        Ok(())
    }

    async fn deploy_local(&self, domain: &str, bundle: &Bundle) -> Result<bool> {
        certificate_dates(domain, &bundle.chain)?;
        let candidate = RustlsConfig::from_pem(
            bundle.chain.as_bytes().to_vec(),
            bundle.key.as_bytes().to_vec(),
        )
        .await
        .context("验证本机 TLS 证书与私钥")?;
        let changed = fs::read(&self.hub.tls_cert).ok().as_deref() != Some(bundle.chain.as_bytes())
            || fs::read(&self.hub.tls_key).ok().as_deref() != Some(bundle.key.as_bytes());
        if changed {
            write_pem(&self.hub.tls_key, &bundle.key)?;
            write_pem(&self.hub.tls_cert, &bundle.chain)?;
        }
        let mut tls = self.tls.write().await;
        if let Some(tls) = tls.as_ref() {
            if changed {
                tls.reload_from_config(candidate.get_inner());
            }
        } else {
            *tls = Some(candidate);
        }
        Ok(changed)
    }

    async fn account(&self, config: &CertificateConfig) -> Result<Account> {
        use sha2::{Digest, Sha256};
        let path = self.directory.join(format!(
            "account-{}.json",
            hex::encode(Sha256::digest(config.directory.as_bytes()))
        ));
        let http = AcmeHttp(
            camera_hub::http_client::builder()?
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
        );
        let builder = Account::builder_with_http(Box::new(http));
        if let Some(credentials) = read_json(&path)? {
            return Ok(builder.from_credentials(credentials).await?);
        }
        let contact = (!config.email.is_empty()).then(|| format!("mailto:{}", config.email));
        let contacts = contact.iter().map(String::as_str).collect::<Vec<_>>();
        let (account, credentials) = builder
            .create(
                &NewAccount {
                    contact: &contacts,
                    terms_of_service_agreed: true,
                    only_return_existing: false,
                },
                config.directory.clone(),
                None,
            )
            .await?;
        atomic_file::write_json(&path, &credentials)?;
        Ok(account)
    }

    async fn issue(
        &self,
        config: &CertificateConfig,
        domain: &str,
        shortlived: bool,
        zone: &str,
        dns: &DnspodClient,
        journal: &Path,
    ) -> Result<Bundle> {
        let name = if domain == zone {
            "_acme-challenge".to_owned()
        } else {
            format!(
                "_acme-challenge.{}",
                domain
                    .strip_suffix(&format!(".{zone}"))
                    .context("证书域名不属于配置的 DNSPod 主域名")?
            )
        };
        let result = async {
            let account = self.account(config).await?;
            let identifiers = [Identifier::Dns(domain.to_owned())];
            let mut request = NewOrder::new(&identifiers);
            if shortlived { request = request.profile("shortlived"); }
            let mut order = account.new_order(&request).await?;
            let mut authorizations = order.authorizations();
            while let Some(authorization) = authorizations.next().await {
                let mut authorization = authorization?;
                if authorization.status == AuthorizationStatus::Valid { continue; }
                if authorization.status != AuthorizationStatus::Pending {
                    bail!("ACME 授权状态异常: {:?}", authorization.status);
                }
                let mut challenge = authorization.challenge(ChallengeType::Dns01)
                    .context("ACME 未提供 DNS-01 验证")?;
                let response = dns.call("CreateRecord", json!({
                    "Domain": zone, "SubDomain": name, "RecordType": "TXT",
                    "RecordLine": "默认", "Value": challenge.key_authorization().dns_value(), "TTL": 600,
                })).await?;
                let id = response.get("RecordId").and_then(Value::as_u64)
                    .context("DNSPod 创建 TXT 未返回 RecordId")?;
                if let Err(error) = atomic_file::write_json(journal, &ChallengeRecord { zone: zone.to_owned(), id }) {
                    let _ = dns.call("DeleteRecord", json!({"Domain": zone, "RecordId": id})).await;
                    return Err(error);
                }
                tokio::time::sleep(Duration::from_secs(config.dns_wait_seconds)).await;
                challenge.set_ready().await?;
            }
            let retry = RetryPolicy::new().timeout(Duration::from_secs(180));
            if order.poll_ready(&retry).await? != OrderStatus::Ready {
                bail!("ACME 订单未进入 ready 状态");
            }
            let key = order.finalize().await?;
            let chain = order.poll_certificate(&retry).await?;
            certificate_dates(domain, &chain)?;
            RustlsConfig::from_pem(chain.as_bytes().to_vec(), key.as_bytes().to_vec())
                .await.context("验证 ACME 签发证书与私钥")?;
            Ok(Bundle { chain, key })
        }.await;
        let cleaned = cleanup(dns, journal).await;
        match (result, cleaned) {
            (Ok(bundle), Err(error)) => {
                // Retain issuance despite a cleanup failure; next check retries only cleanup.
                atomic_file::write_json(&self.directory.join(format!("{domain}.json")), &bundle)?;
                Err(error)
            }
            (Ok(bundle), Ok(())) => Ok(bundle),
            (Err(error), _) => Err(error),
        }
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>> {
    match fs::read(path) {
        Ok(data) => Ok(Some(
            serde_json::from_slice(&data).with_context(|| format!("读取 {}", path.display()))?,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

async fn cleanup(dns: &DnspodClient, journal: &Path) -> Result<()> {
    let Some(record): Option<ChallengeRecord> = read_json(journal)? else {
        return Ok(());
    };
    let result = dns
        .call(
            "DeleteRecord",
            json!({"Domain": record.zone, "RecordId": record.id}),
        )
        .await;
    if let Err(error) = result {
        if !error.to_string().contains("InvalidParameter.RecordId") {
            return Err(error.context("清理 ACME 临时 TXT"));
        }
    }
    fs::remove_file(journal)?;
    Ok(())
}

fn certificate_dates(domain: &str, chain: &str) -> Result<(u64, u64)> {
    let (_, pem) = x509_parser::pem::parse_x509_pem(chain.as_bytes())
        .map_err(|_| anyhow::anyhow!("解析证书 PEM 失败"))?;
    let certificate = pem.parse_x509().context("解析 X509 证书")?;
    let san = certificate
        .subject_alternative_name()?
        .context("证书缺少 SAN")?;
    if !san.value.general_names.iter().any(
        |name| matches!(name, GeneralName::DNSName(value) if value.eq_ignore_ascii_case(domain)),
    ) {
        bail!("证书 SAN 不匹配域名 {domain}");
    }
    let start = certificate.validity().not_before.timestamp().max(0) as u64;
    let end = certificate.validity().not_after.timestamp().max(0) as u64;
    // Expired certificates still need to be parsed so the scheduler can replace them.
    if end <= start || start > epoch_seconds() + 300 {
        bail!("证书有效期异常");
    }
    let threshold = ((end - start) / 3).min(30 * 86_400);
    Ok((end, end.saturating_sub(threshold)))
}

fn write_pem(path: &Path, value: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("pem.new");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(value.as_bytes())?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    Ok(())
}

struct AcmeHttp(reqwest::Client);

impl HttpClient for AcmeHttp {
    fn request(
        &self,
        request: Request<BodyWrapper<Bytes>>,
    ) -> Pin<Box<dyn Future<Output = Result<BytesResponse, instant_acme::Error>> + Send>> {
        let client = self.0.clone();
        Box::pin(async move {
            let result = async {
                let (parts, body) = request.into_parts();
                let body = body.collect().await?.to_bytes();
                let response = client
                    .request(parts.method, parts.uri.to_string())
                    .headers(parts.headers)
                    .body(body)
                    .send()
                    .await?;
                let mut builder = Response::builder().status(response.status());
                *builder.headers_mut().unwrap() = response.headers().clone();
                Ok::<_, anyhow::Error>(builder.body(Full::new(response.bytes().await?))?.into())
            }
            .await;
            result.map_err(|error| instant_acme::Error::Other(error.into()))
        })
    }
}
