use crate::auth::constant_time_eq;
use crate::config::Config;
use anyhow::{Context, Result};
use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::header::{CONTENT_TYPE, HeaderMap};
use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Semaphore;

const CONFIG_VERSION: u32 = 2;
const MAX_URL_BYTES: usize = 4096;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_REQUEST_HEADERS: usize = 64;
const MAX_REQUEST_HEADER_BYTES: usize = 16 * 1024;
const MAX_CONCURRENT_REQUESTS: usize = 4;
const REQUEST_TIMEOUT_SECONDS: u64 = 10;
const SUPPORTED_METHODS: [&str; 6] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"];

#[derive(Clone, Debug, Deserialize)]
pub struct HttpProxyUpdate {
    pub enabled: bool,
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
    pub endpoint: &'static str,
    pub legacy_get_endpoint: &'static str,
    pub unrestricted_targets: bool,
    pub supported_methods: [&'static str; 6],
    pub max_url_bytes: usize,
    pub max_request_bytes: usize,
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
    MethodNotAllowed,
    Disabled,
    Unauthorized,
    Busy,
    Upstream(String),
    RequestTooLarge,
    TooLarge,
}

impl std::fmt::Display for HttpProxyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) | Self::Upstream(message) => formatter.write_str(message),
            Self::MethodNotAllowed => {
                formatter.write_str("HTTP 代理仅支持 GET、POST、PUT、PATCH、DELETE 和 HEAD")
            }
            Self::Disabled => formatter.write_str("HTTP 代理接口尚未启用"),
            Self::Unauthorized => formatter.write_str("invalid HTTP proxy token"),
            Self::Busy => formatter.write_str("HTTP 代理并发请求已满"),
            Self::RequestTooLarge => formatter.write_str("代理请求体超过 1 MiB 限制"),
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
}

impl Default for HttpProxyDocument {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            revision: 1,
            enabled: false,
            token: String::new(),
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
            endpoint: "/api/v1/integrations/http-proxy/request",
            legacy_get_endpoint: "/api/v1/integrations/http-proxy/fetch",
            unrestricted_targets: true,
            supported_methods: SUPPORTED_METHODS,
            max_url_bytes: MAX_URL_BYTES,
            max_request_bytes: MAX_REQUEST_BYTES,
            max_response_bytes: MAX_RESPONSE_BYTES,
            max_concurrent_requests: MAX_CONCURRENT_REQUESTS,
            timeout_seconds: REQUEST_TIMEOUT_SECONDS,
        }
    }

    pub fn update(&self, update: HttpProxyUpdate) -> Result<HttpProxyOverview> {
        let mut store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        let mut next = store.current.clone();
        next.enabled = update.enabled;
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

    pub async fn forward(
        &self,
        method: Method,
        raw_url: &str,
        headers: &HeaderMap,
        body: Bytes,
    ) -> std::result::Result<HttpProxyResponse, HttpProxyError> {
        {
            let store = self.store.lock().unwrap_or_else(|error| error.into_inner());
            if !store.current.enabled {
                return Err(HttpProxyError::Disabled);
            }
        }
        validate_method(&method)?;
        if body.len() > MAX_REQUEST_BYTES {
            return Err(HttpProxyError::RequestTooLarge);
        }
        if matches!(method, Method::GET | Method::HEAD) && !body.is_empty() {
            return Err(HttpProxyError::Invalid(
                "GET 和 HEAD 请求不能携带请求体".to_owned(),
            ));
        }
        let url = validate_target(raw_url)?;
        let headers = filter_request_headers(headers)?;
        let _permit = self
            .concurrency
            .clone()
            .try_acquire_owned()
            .map_err(|_| HttpProxyError::Busy)?;
        let mut request = self.client.request(method, url).headers(headers);
        if !body.is_empty() {
            request = request.body(body);
        }
        let response = request
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

fn validate_method(method: &Method) -> std::result::Result<(), HttpProxyError> {
    if matches!(
        *method,
        Method::GET | Method::POST | Method::PUT | Method::PATCH | Method::DELETE | Method::HEAD
    ) {
        Ok(())
    } else {
        Err(HttpProxyError::MethodNotAllowed)
    }
}

fn validate_target(value: &str) -> std::result::Result<Url, HttpProxyError> {
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
    if matches!(
        url.path(),
        "/api/v1/integrations/http-proxy/fetch" | "/api/v1/integrations/http-proxy/request"
    ) {
        return Err(HttpProxyError::Invalid(
            "目标 URL 不能指向 HTTP 代理接口自身".to_owned(),
        ));
    }
    Ok(url)
}

fn filter_request_headers(headers: &HeaderMap) -> std::result::Result<HeaderMap, HttpProxyError> {
    if headers.len() > MAX_REQUEST_HEADERS {
        return Err(HttpProxyError::Invalid(format!(
            "请求头数量不能超过 {MAX_REQUEST_HEADERS}"
        )));
    }
    let mut total_bytes = 0usize;
    let mut filtered = HeaderMap::new();
    for (name, value) in headers {
        let name_text = name.as_str();
        total_bytes = total_bytes
            .saturating_add(name_text.len())
            .saturating_add(value.as_bytes().len());
        if total_bytes > MAX_REQUEST_HEADER_BYTES {
            return Err(HttpProxyError::Invalid(
                "请求头总大小不能超过 16 KiB".to_owned(),
            ));
        }
        if forbidden_request_header(name_text) {
            continue;
        }
        filtered.append(name.clone(), value.clone());
    }
    Ok(filtered)
}

fn forbidden_request_header(name: &str) -> bool {
    matches!(
        name,
        "host"
            | "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "content-length"
            | "cookie"
            | "origin"
            | "referer"
            | "accept-encoding"
            | "forwarded"
            | "x-real-ip"
    ) || name.starts_with("x-forwarded-")
        || name.starts_with("sec-")
        || name.starts_with("cf-")
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

    fn enable(service: &HttpProxyService) -> HttpProxyOverview {
        service.update(HttpProxyUpdate { enabled: true }).unwrap()
    }

    #[test]
    fn accepts_any_valid_http_target_and_common_methods() {
        assert!(validate_target("https://example.com/path?q=1").is_ok());
        assert!(validate_target("http://127.0.0.1:8080/private").is_ok());
        assert!(validate_target("ftp://example.com/file").is_err());
        assert!(validate_target("https://user@example.com/path").is_err());
        assert!(validate_target("https://example.com/path#fragment").is_err());
        assert!(
            validate_target("https://example.com/api/v1/integrations/http-proxy/request").is_err()
        );
        for method in [
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::HEAD,
        ] {
            validate_method(&method).unwrap();
        }
        assert!(matches!(
            validate_method(&Method::CONNECT),
            Err(HttpProxyError::MethodNotAllowed)
        ));
    }

    #[test]
    fn migrates_old_allowlist_config_and_rotates_token() {
        let path = temporary_config("persist");
        fs::write(
            &path,
            br#"{"version":1,"revision":3,"enabled":true,"token":"chp_legacy","allowed_origins":["https://example.com"]}"#,
        )
        .unwrap();
        let service = HttpProxyService::start(&test_config(path.clone())).unwrap();
        let overview = service.overview();
        assert!(overview.enabled);
        assert!(overview.unrestricted_targets);
        assert_eq!(overview.token, "chp_legacy");
        assert_eq!(overview.supported_methods, SUPPORTED_METHODS);
        let persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(persisted["version"], CONFIG_VERSION);
        assert!(persisted.get("allowed_origins").is_none());
        let rotated = service.rotate_token().unwrap();
        assert_ne!(rotated, overview.token);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn strips_transport_and_browser_identity_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer target".parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert("x-business-id", "42".parse().unwrap());
        headers.insert("cookie", "camera_hub_session=secret".parse().unwrap());
        headers.insert("x-forwarded-for", "127.0.0.1".parse().unwrap());
        headers.insert("sec-fetch-site", "same-origin".parse().unwrap());
        let filtered = filter_request_headers(&headers).unwrap();
        assert_eq!(filtered["authorization"], "Bearer target");
        assert_eq!(filtered["content-type"], "application/json");
        assert_eq!(filtered["x-business-id"], "42");
        assert!(!filtered.contains_key("cookie"));
        assert!(!filtered.contains_key("x-forwarded-for"));
        assert!(!filtered.contains_key("sec-fetch-site"));
    }

    #[tokio::test]
    async fn forwards_get_to_any_http_origin() {
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
        enable(&service);
        let response = service
            .forward(
                Method::GET,
                &format!("http://{address}/status?value=ok"),
                &HeaderMap::new(),
                Bytes::new(),
            )
            .await
            .unwrap();
        assert_eq!(response.status, 201);
        assert_eq!(response.content_type.as_deref(), Some("text/plain"));
        assert_eq!(response.body, b"ready");
        server.await.unwrap();
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn forwards_post_body_and_safe_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 4096];
            let length = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..length]);
            let lower = request.to_ascii_lowercase();
            assert!(request.starts_with("POST /commands HTTP/1.1"));
            assert!(lower.contains("authorization: bearer target-token\r\n"));
            assert!(lower.contains("content-type: application/json\r\n"));
            assert!(lower.contains("x-business-id: 42\r\n"));
            assert!(!lower.contains("camera_hub_session"));
            assert!(!lower.contains("x-forwarded-for"));
            assert!(request.ends_with(r#"{"power":true}"#));
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
                )
                .await
                .unwrap();
        });
        let path = temporary_config("post");
        let service = HttpProxyService::start(&test_config(path.clone())).unwrap();
        enable(&service);
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer target-token".parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert("x-business-id", "42".parse().unwrap());
        headers.insert("cookie", "camera_hub_session=secret".parse().unwrap());
        headers.insert("x-forwarded-for", "127.0.0.1".parse().unwrap());
        let response = service
            .forward(
                Method::POST,
                &format!("http://{address}/commands"),
                &headers,
                Bytes::from_static(br#"{"power":true}"#),
            )
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, br#"{"ok":true}"#);
        server.await.unwrap();
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn rejects_unsupported_bodies_before_network_access() {
        let path = temporary_config("body-limits");
        let service = HttpProxyService::start(&test_config(path.clone())).unwrap();
        enable(&service);
        assert!(matches!(
            service
                .forward(
                    Method::GET,
                    "https://example.com/",
                    &HeaderMap::new(),
                    Bytes::from_static(b"unexpected"),
                )
                .await,
            Err(HttpProxyError::Invalid(_))
        ));
        assert!(matches!(
            service
                .forward(
                    Method::POST,
                    "https://example.com/",
                    &HeaderMap::new(),
                    Bytes::from(vec![0; MAX_REQUEST_BYTES + 1]),
                )
                .await,
            Err(HttpProxyError::RequestTooLarge)
        ));
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
        enable(&service);
        let response = service
            .forward(
                Method::GET,
                &format!("http://{address}/redirect"),
                &HeaderMap::new(),
                Bytes::new(),
            )
            .await
            .unwrap();
        assert_eq!(response.status, 302);
        assert!(response.body.is_empty());
        server.await.unwrap();
        let _ = fs::remove_file(path);
    }
}
