use crate::auth::constant_time_eq;
use crate::config::Config;
use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::Url;
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Semaphore;

const CONFIG_VERSION: u32 = 1;
const MAX_ALLOWED_ORIGINS: usize = 32;
const MAX_URL_BYTES: usize = 4096;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_CONCURRENT_REQUESTS: usize = 4;
const REQUEST_TIMEOUT_SECONDS: u64 = 10;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpProxyUpdate {
    pub enabled: bool,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpProxyQuery {
    pub token: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct HttpProxyOverview {
    pub enabled: bool,
    pub token: String,
    pub allowed_origins: Vec<String>,
    pub endpoint: &'static str,
    pub max_url_bytes: usize,
    pub max_response_bytes: usize,
    pub max_concurrent_requests: usize,
    pub timeout_seconds: u64,
}

pub struct HttpProxyResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub enum HttpProxyError {
    Invalid(String),
    Disabled,
    Unauthorized,
    Forbidden(String),
    Busy,
    Upstream(String),
    TooLarge,
}

impl std::fmt::Display for HttpProxyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) | Self::Forbidden(message) | Self::Upstream(message) => {
                formatter.write_str(message)
            }
            Self::Disabled => formatter.write_str("HTTP 代理接口尚未启用"),
            Self::Unauthorized => formatter.write_str("invalid HTTP proxy token"),
            Self::Busy => formatter.write_str("HTTP 代理并发请求已满"),
            Self::TooLarge => formatter.write_str("目标响应超过 1 MiB 限制"),
        }
    }
}

impl std::error::Error for HttpProxyError {}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
struct HttpProxyDocument {
    version: u32,
    revision: u64,
    enabled: bool,
    token: String,
    allowed_origins: Vec<String>,
}

impl Default for HttpProxyDocument {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            revision: 1,
            enabled: false,
            token: String::new(),
            allowed_origins: Vec::new(),
        }
    }
}

impl HttpProxyDocument {
    fn normalize(mut self) -> Result<Self> {
        self.version = CONFIG_VERSION;
        if self.token.is_empty() {
            self.token = generate_token()?;
        }
        if self.token.len() > 128 || self.token.chars().any(char::is_control) {
            anyhow::bail!("HTTP 代理 Token 格式无效");
        }
        if self.allowed_origins.len() > MAX_ALLOWED_ORIGINS {
            anyhow::bail!("HTTP 代理最多允许 {MAX_ALLOWED_ORIGINS} 个 Origin");
        }
        let mut origins = BTreeSet::new();
        for origin in &self.allowed_origins {
            origins.insert(normalize_origin(origin)?);
        }
        self.allowed_origins = origins.into_iter().collect();
        if self.enabled && self.allowed_origins.is_empty() {
            anyhow::bail!("启用 HTTP 代理前至少配置一个允许的 Origin");
        }
        Ok(self)
    }
}

struct HttpProxyStore {
    path: PathBuf,
    current: HttpProxyDocument,
}

impl HttpProxyStore {
    fn load(path: PathBuf) -> Result<Self> {
        let current = match fs::read(&path) {
            Ok(data) => serde_json::from_slice::<HttpProxyDocument>(&data)
                .with_context(|| format!("parse HTTP proxy config {}", path.display()))?
                .normalize()?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                HttpProxyDocument::default().normalize()?
            }
            Err(error) => return Err(error.into()),
        };
        let store = Self { path, current };
        store.save()?;
        Ok(store)
    }

    fn save(&self) -> Result<()> {
        crate::atomic_file::write_json(&self.path, &self.current)
    }

    fn commit(&mut self, next: HttpProxyDocument) -> Result<()> {
        crate::atomic_file::write_json(&self.path, &next)?;
        self.current = next;
        Ok(())
    }
}

pub struct HttpProxyService {
    store: Mutex<HttpProxyStore>,
    client: reqwest::Client,
    concurrency: Arc<Semaphore>,
}

impl HttpProxyService {
    pub fn start(config: &Config) -> Result<Arc<Self>> {
        let client = camera_hub::http_client::builder()?
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .context("build HTTP proxy client")?;
        Ok(Arc::new(Self {
            store: Mutex::new(HttpProxyStore::load(config.http_proxy_config_file.clone())?),
            client,
            concurrency: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
        }))
    }

    pub fn overview(&self) -> HttpProxyOverview {
        let store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        HttpProxyOverview {
            enabled: store.current.enabled,
            token: if store.current.enabled {
                store.current.token.clone()
            } else {
                String::new()
            },
            allowed_origins: store.current.allowed_origins.clone(),
            endpoint: "/api/v1/integrations/http-proxy/fetch",
            max_url_bytes: MAX_URL_BYTES,
            max_response_bytes: MAX_RESPONSE_BYTES,
            max_concurrent_requests: MAX_CONCURRENT_REQUESTS,
            timeout_seconds: REQUEST_TIMEOUT_SECONDS,
        }
    }

    pub fn update(&self, update: HttpProxyUpdate) -> Result<HttpProxyOverview> {
        let mut store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        let mut next = store.current.clone();
        next.enabled = update.enabled;
        next.allowed_origins = update.allowed_origins;
        if next.enabled && !store.current.enabled {
            next.token = generate_token()?;
        }
        next.revision = next.revision.saturating_add(1);
        next = next.normalize()?;
        store.commit(next)?;
        drop(store);
        Ok(self.overview())
    }

    pub fn rotate_token(&self) -> Result<String> {
        let mut store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        let mut next = store.current.clone();
        next.token = generate_token()?;
        next.revision = next.revision.saturating_add(1);
        let token = next.token.clone();
        store.commit(next)?;
        Ok(token)
    }

    pub fn verify_token(&self, token: &str) -> std::result::Result<(), HttpProxyError> {
        let store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        if !store.current.enabled {
            return Err(HttpProxyError::Disabled);
        }
        if token.is_empty() || !constant_time_eq(token.as_bytes(), store.current.token.as_bytes()) {
            return Err(HttpProxyError::Unauthorized);
        }
        Ok(())
    }

    pub async fn fetch(
        &self,
        raw_url: &str,
    ) -> std::result::Result<HttpProxyResponse, HttpProxyError> {
        let allowed_origins = {
            let store = self.store.lock().unwrap_or_else(|error| error.into_inner());
            if !store.current.enabled {
                return Err(HttpProxyError::Disabled);
            }
            store.current.allowed_origins.clone()
        };
        let url = validate_target(raw_url, &allowed_origins)?;
        let _permit = self
            .concurrency
            .clone()
            .try_acquire_owned()
            .map_err(|_| HttpProxyError::Busy)?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| HttpProxyError::Upstream(format!("目标请求失败：{error}")))?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(HttpProxyError::TooLarge);
        }
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk
                .map_err(|error| HttpProxyError::Upstream(format!("读取目标响应失败：{error}")))?;
            if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(HttpProxyError::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(HttpProxyResponse {
            status,
            content_type,
            body,
        })
    }
}

fn normalize_origin(value: &str) -> Result<String> {
    let url = Url::parse(value.trim()).context("Origin 必须是完整 URL")?;
    if !matches!(url.scheme(), "http" | "https") {
        anyhow::bail!("Origin 仅支持 http 或 https");
    }
    if url.host_str().is_none() {
        anyhow::bail!("Origin 缺少主机名");
    }
    if !url.username().is_empty() || url.password().is_some() {
        anyhow::bail!("Origin 不能包含用户名或密码");
    }
    if !matches!(url.path(), "" | "/") || url.query().is_some() || url.fragment().is_some() {
        anyhow::bail!("Origin 只能包含协议、主机和端口");
    }
    Ok(url.origin().ascii_serialization())
}

fn validate_target(
    value: &str,
    allowed_origins: &[String],
) -> std::result::Result<Url, HttpProxyError> {
    if value.is_empty() || value.len() > MAX_URL_BYTES || value.chars().any(char::is_control) {
        return Err(HttpProxyError::Invalid("目标 URL 格式无效".to_owned()));
    }
    let url = Url::parse(value)
        .map_err(|_| HttpProxyError::Invalid("目标 URL 必须是完整 URL".to_owned()))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(HttpProxyError::Invalid(
            "目标 URL 仅支持无用户信息和片段的 HTTP(S) URL".to_owned(),
        ));
    }
    let origin = url.origin().ascii_serialization();
    if !allowed_origins.iter().any(|allowed| allowed == &origin) {
        return Err(HttpProxyError::Forbidden(format!(
            "目标 Origin 未在白名单中：{origin}"
        )));
    }
    Ok(url)
}

fn generate_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).context("generate HTTP proxy token")?;
    Ok(format!("chp_{}", hex::encode(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn temporary_config(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "camera-hub-http-proxy-{label}-{}-{nonce}.json",
            std::process::id()
        ))
    }

    fn test_config(path: PathBuf) -> Config {
        let mut config = Config::parse_from(["camera-hub"]);
        config.http_proxy_config_file = path;
        config
    }

    #[test]
    fn normalizes_only_exact_http_origins() {
        assert_eq!(
            normalize_origin("https://example.com:8443/").unwrap(),
            "https://example.com:8443"
        );
        assert!(normalize_origin("ftp://example.com").is_err());
        assert!(normalize_origin("https://user@example.com").is_err());
        assert!(normalize_origin("https://example.com/path").is_err());
        assert!(normalize_origin("https://example.com/?query=1").is_err());
    }

    #[test]
    fn persists_configuration_and_rotates_token_when_enabled() {
        let path = temporary_config("persist");
        let service = HttpProxyService::start(&test_config(path.clone())).unwrap();
        assert!(!service.overview().enabled);
        let enabled = service
            .update(HttpProxyUpdate {
                enabled: true,
                allowed_origins: vec![
                    "https://example.com/".to_owned(),
                    "https://example.com".to_owned(),
                ],
            })
            .unwrap();
        assert!(enabled.enabled);
        assert_eq!(enabled.allowed_origins, ["https://example.com"]);
        assert!(enabled.token.starts_with("chp_"));
        let reloaded = HttpProxyService::start(&test_config(path.clone())).unwrap();
        assert_eq!(reloaded.overview().token, enabled.token);
        let rotated = reloaded.rotate_token().unwrap();
        assert_ne!(rotated, enabled.token);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_targets_outside_allowlist() {
        let allowed = vec!["https://example.com".to_owned()];
        assert!(validate_target("https://example.com/path?q=1", &allowed).is_ok());
        assert!(matches!(
            validate_target("https://other.example/path", &allowed),
            Err(HttpProxyError::Forbidden(_))
        ));
        assert!(validate_target("https://user@example.com/path", &allowed).is_err());
        assert!(validate_target("https://example.com/path#fragment", &allowed).is_err());
    }

    #[tokio::test]
    async fn forwards_allowed_get() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 2048];
            let length = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..length]);
            assert!(request.starts_with("GET /status?value=ok HTTP/1.1"));
            stream
                .write_all(
                    b"HTTP/1.1 201 Created\r\nContent-Type: text/plain\r\nContent-Length: 5\r\nConnection: close\r\n\r\nready",
                )
                .await
                .unwrap();
        });
        let path = temporary_config("forward");
        let service = HttpProxyService::start(&test_config(path.clone())).unwrap();
        service
            .update(HttpProxyUpdate {
                enabled: true,
                allowed_origins: vec![format!("http://{address}")],
            })
            .unwrap();
        let response = service
            .fetch(&format!("http://{address}/status?value=ok"))
            .await
            .unwrap();
        assert_eq!(response.status, 201);
        assert_eq!(response.content_type.as_deref(), Some("text/plain"));
        assert_eq!(response.body, b"ready");
        server.await.unwrap();
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn returns_redirect_without_following_it() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 1024];
            stream.read(&mut request).await.unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
        });
        let path = temporary_config("redirect");
        let service = HttpProxyService::start(&test_config(path.clone())).unwrap();
        service
            .update(HttpProxyUpdate {
                enabled: true,
                allowed_origins: vec![format!("http://{address}")],
            })
            .unwrap();
        let response = service
            .fetch(&format!("http://{address}/redirect"))
            .await
            .unwrap();
        assert_eq!(response.status, 302);
        assert!(response.body.is_empty());
        server.await.unwrap();
        let _ = fs::remove_file(path);
    }
}
