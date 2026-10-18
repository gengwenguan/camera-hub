use axum::Json;
use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::extract::{Extension, Request};
use axum::http::header::{COOKIE, HOST, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const COOKIE_NAME: &str = "camera_hub_session";

#[derive(Clone, Copy)]
pub struct TransportSecurity {
    pub secure: bool,
    pub tls_available: bool,
    pub tls_port: u16,
}

pub struct WebAuth {
    username: String,
    password: String,
    token: String,
    moq_token: String,
    device_tokens: BTreeMap<String, String>,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

impl WebAuth {
    pub fn new(username: String, password: String) -> Self {
        Self {
            username,
            password,
            token: session_token(),
            moq_token: session_token(),
            device_tokens: BTreeMap::new(),
        }
    }

    pub fn moq_token(&self) -> &str {
        &self.moq_token
    }

    pub fn with_device_tokens(mut self, json: &str) -> anyhow::Result<Self> {
        self.device_tokens = serde_json::from_str(json)?;
        if self
            .device_tokens
            .values()
            .any(|token| token.len() < 32 || token.chars().any(char::is_control))
        {
            anyhow::bail!("device link tokens must contain at least 32 characters");
        }
        Ok(self)
    }

    pub fn can_read_device_photos(
        &self,
        headers: &HeaderMap,
        device_id: &str,
        token: &str,
    ) -> bool {
        self.authorized(headers)
            || self.device_tokens.get(device_id).is_some_and(|expected| {
                !token.is_empty() && constant_time_eq(expected.as_bytes(), token.as_bytes())
            })
    }

    fn verify(&self, username: &str, password: &str) -> bool {
        constant_time_eq(username.as_bytes(), self.username.as_bytes())
            & constant_time_eq(password.as_bytes(), self.password.as_bytes())
    }

    fn authorized(&self, headers: &HeaderMap) -> bool {
        headers
            .get(COOKIE)
            .and_then(|value| value.to_str().ok())
            .and_then(|header| cookie_value(header, COOKIE_NAME))
            .is_some_and(|value| constant_time_eq(value.as_bytes(), self.token.as_bytes()))
    }

    fn set_cookie(&self, secure: bool) -> String {
        format!(
            "{COOKIE_NAME}={}; Path=/; Max-Age=604800; HttpOnly; SameSite=Strict{}",
            self.token,
            if secure { "; Secure" } else { "" }
        )
    }

    fn clear_cookie(secure: bool) -> String {
        format!(
            "{COOKIE_NAME}=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict{}",
            if secure { "; Secure" } else { "" }
        )
    }
}

pub async fn require_auth(request: Request, next: Next) -> Response {
    let transport = request
        .extensions()
        .get::<TransportSecurity>()
        .copied()
        .unwrap_or(TransportSecurity {
            secure: false,
            tls_available: false,
            tls_port: 443,
        });
    let local = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .is_some_and(|peer| is_loopback(peer.0.ip()));
    if voice_studio_path(request.uri().path()) && !transport.secure {
        if should_redirect_to_https(transport, local) {
            return https_redirect(
                request.headers(),
                request.uri().path_and_query().map(|value| value.as_str()),
                transport.tls_port,
            );
        }
        if !local {
            return (
                StatusCode::UPGRADE_REQUIRED,
                Json(serde_json::json!({
                    "ok": false,
                    "error": "voice studio requires HTTPS"
                })),
            )
                .into_response();
        }
    }
    if public_path(request.method().as_str(), request.uri().path()) {
        return next.run(request).await;
    }
    if should_redirect_to_https(transport, local) {
        return https_redirect(
            request.headers(),
            request.uri().path_and_query().map(|v| v.as_str()),
            transport.tls_port,
        );
    }
    let authorized = request
        .extensions()
        .get::<Arc<WebAuth>>()
        .is_some_and(|auth| auth.authorized(request.headers()));
    if authorized {
        return next.run(request).await;
    }
    if request.uri().path().starts_with("/api/")
        || request.uri().path().starts_with("/records/")
        || request.uri().path().starts_with("/photos/")
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "ok": false,
                "error": "authentication required"
            })),
        )
            .into_response();
    }
    redirect("/login", StatusCode::SEE_OTHER)
}

fn voice_studio_path(path: &str) -> bool {
    path == "/voice-studio"
        || path == "/voice-studio.js"
        || path == "/voice-studio.css"
        || path.starts_with("/api/v1/public/voice-studio/")
}

fn public_path(method: &str, path: &str) -> bool {
    method == "OPTIONS"
        || path == "/login"
        || path == "/api/v1/auth/login"
        || path == "/voice-studio"
        || path == "/voice-studio.js"
        || path == "/voice-studio.css"
        || path.starts_with("/api/v1/public/voice-studio/")
        || (method == "POST" && path == "/api/v1/integrations/qq/notify")
        || (matches!(method, "GET" | "POST") && path == "/api/v1/integrations/aircon/command")
        || (method == "POST" && path.starts_with("/api/v1/internal/voice/aircon/"))
        || path == "/health"
        || path == "/certificate.sha256"
        || path.starts_with("/.well-known/acme-challenge/")
        || (method == "GET" && path.starts_with("/api/v1/devices/") && path.ends_with("/link"))
}

pub async fn login_page(
    Extension(auth): Extension<Arc<WebAuth>>,
    Extension(transport): Extension<TransportSecurity>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if should_redirect_to_https(transport, is_loopback(peer.ip())) {
        return https_redirect(&headers, Some("/login"), transport.tls_port);
    }
    if auth.authorized(&headers) {
        return redirect("/", StatusCode::SEE_OTHER);
    }
    Html(login_html(false)).into_response()
}

pub async fn login(
    Extension(auth): Extension<Arc<WebAuth>>,
    Extension(transport): Extension<TransportSecurity>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Response {
    if should_redirect_to_https(transport, is_loopback(peer.ip())) {
        return https_redirect(&headers, Some("/login"), transport.tls_port);
    }
    if !auth.verify(&request.username, &request.password) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "ok": false,
                "error": "用户名或密码错误"
            })),
        )
            .into_response();
    }
    let mut response = (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response();
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_str(&auth.set_cookie(transport.secure)).expect("valid session cookie"),
    );
    response
}

pub async fn logout(Extension(transport): Extension<TransportSecurity>) -> Response {
    let mut response = redirect("/login", StatusCode::SEE_OTHER);
    if let Ok(value) = HeaderValue::from_str(&WebAuth::clear_cookie(transport.secure)) {
        response.headers_mut().insert(SET_COOKIE, value);
    }
    response
}

fn https_redirect(headers: &HeaderMap, path: Option<&str>, tls_port: u16) -> Response {
    let host = headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .map(without_port)
        .unwrap_or("mi6.gwghome.site");
    let authority = if tls_port == 443 {
        host.to_owned()
    } else {
        format!("{host}:{tls_port}")
    };
    redirect(
        &format!("https://{authority}{}", path.unwrap_or("/")),
        StatusCode::TEMPORARY_REDIRECT,
    )
}

fn should_redirect_to_https(transport: TransportSecurity, local: bool) -> bool {
    !transport.secure && transport.tls_available && !local
}

fn is_loopback(ip: IpAddr) -> bool {
    ip.is_loopback()
        || matches!(ip, IpAddr::V6(value) if value.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback()))
}

fn redirect(location: &str, status: StatusCode) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = status;
    if let Ok(value) = HeaderValue::from_str(location) {
        response.headers_mut().insert(LOCATION, value);
    }
    response
}

fn without_port(host: &str) -> &str {
    if let Some(rest) = host.strip_prefix('[') {
        return rest.find(']').map(|end| &host[..end + 2]).unwrap_or(host);
    }
    host.rsplit_once(':')
        .filter(|(_, port)| port.bytes().all(|byte| byte.is_ascii_digit()))
        .map(|(name, _)| name)
        .unwrap_or(host)
}

fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key == name).then_some(value)
    })
}

fn session_token() -> String {
    let mut random = [0u8; 32];
    if File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .is_err()
    {
        let fallback = format!(
            "{}:{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        );
        random.copy_from_slice(&Sha256::digest(fallback.as_bytes()));
    }
    random.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let length = left.len().max(right.len());
    for index in 0..length {
        difference |= usize::from(
            left.get(index).copied().unwrap_or_default()
                ^ right.get(index).copied().unwrap_or_default(),
        );
    }
    difference == 0
}

fn login_html(invalid: bool) -> String {
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Camera Hub 登录</title>
<style>
*{{box-sizing:border-box}}body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#111827;color:#111827;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}}main{{width:min(360px,calc(100vw - 32px));background:#fff;border-radius:8px;padding:28px;box-shadow:0 20px 60px #0008}}h1{{margin:0 0 24px;font-size:24px}}label{{display:block;margin:14px 0 6px;font-size:14px;font-weight:600}}input{{width:100%;height:42px;border:1px solid #cbd5e1;border-radius:6px;padding:0 12px;font-size:16px}}input:focus{{outline:2px solid #2563eb;border-color:transparent}}button{{width:100%;height:42px;margin-top:22px;border:0;border-radius:6px;background:#0f766e;color:#fff;font-size:15px;font-weight:700;cursor:pointer}}p{{margin:14px 0 0;color:#b91c1c;font-size:14px}}</style>
</head>
<body><main>
<h1>Camera Hub 登录</h1>
<form id="loginForm">
<label for="username">用户名</label>
<input id="username" name="username" autocomplete="username" required autofocus>
<label for="password">密码</label>
<input id="password" name="password" type="password" autocomplete="current-password" required>
<button type="submit">登录</button>
<p id="error">{}</p>
</form>
<script>
document.getElementById("loginForm").addEventListener("submit",async(event)=>{{
event.preventDefault();const form=new FormData(event.currentTarget);
const response=await fetch("/api/v1/auth/login",{{method:"POST",headers:{{"Content-Type":"application/json"}},body:JSON.stringify({{username:form.get("username"),password:form.get("password")}})}});
if(response.ok){{location.replace("/")}}else{{document.getElementById("error").textContent="用户名或密码错误"}}
}});
</script>
</main></body></html>"#,
        if invalid {
            "用户名或密码错误"
        } else {
            ""
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_credentials_and_cookie() {
        let auth = WebAuth {
            username: "admin".to_owned(),
            password: "12345".to_owned(),
            token: "token".to_owned(),
            moq_token: "moq-token".to_owned(),
            device_tokens: BTreeMap::new(),
        };
        assert!(auth.verify("admin", "12345"));
        assert!(!auth.verify("admin", "bad"));
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("camera_hub_session=token"));
        assert!(auth.authorized(&headers));
    }

    #[test]
    fn photo_return_requires_a_session_or_device_bound_token() {
        let auth = WebAuth::new("admin".to_owned(), "password".to_owned())
            .with_device_tokens(r#"{"front":"0123456789abcdef0123456789abcdef"}"#)
            .unwrap();
        let headers = HeaderMap::new();
        assert!(!auth.can_read_device_photos(&headers, "front", ""));
        assert!(!auth.can_read_device_photos(&headers, "front", "wrong"));
        assert!(!auth.can_read_device_photos(&headers, "back", "0123456789abcdef0123456789abcdef"));
        assert!(auth.can_read_device_photos(&headers, "front", "0123456789abcdef0123456789abcdef"));
        let mut session_headers = HeaderMap::new();
        session_headers.insert(
            COOKIE,
            HeaderValue::from_str(&format!("{COOKIE_NAME}={}", auth.token)).unwrap(),
        );
        assert!(auth.can_read_device_photos(&session_headers, "front", ""));
    }

    #[test]
    fn strips_http_port_but_preserves_ipv6_brackets() {
        assert_eq!(without_port("mi6.gwghome.site:80"), "mi6.gwghome.site");
        assert_eq!(without_port("[2409::1]:80"), "[2409::1]");
    }

    #[test]
    fn redirects_to_configured_tls_port() {
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("mi6.gwghome.site:8080"));
        let response = https_redirect(&headers, Some("/login"), 8443);
        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            response.headers().get(LOCATION).unwrap(),
            "https://mi6.gwghome.site:8443/login"
        );

        headers.insert(HOST, HeaderValue::from_static("[2409::1]:8080"));
        let response = https_redirect(&headers, Some("/"), 8443);
        assert_eq!(
            response.headers().get(LOCATION).unwrap(),
            "https://[2409::1]:8443/"
        );
    }

    #[test]
    fn permits_loopback_http_without_disabling_remote_https() {
        let transport = TransportSecurity {
            secure: false,
            tls_available: true,
            tls_port: 8443,
        };
        assert!(!should_redirect_to_https(transport, true));
        assert!(should_redirect_to_https(transport, false));
        assert!(is_loopback("127.0.0.1".parse().unwrap()));
        assert!(is_loopback("::1".parse().unwrap()));
        assert!(is_loopback("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_loopback("192.168.1.18".parse().unwrap()));
    }

    #[test]
    fn exposes_only_explicit_public_api_routes() {
        assert!(public_path("POST", "/api/v1/integrations/qq/notify"));
        assert!(!public_path("GET", "/api/v1/integrations/qq/notify"));
        assert!(!public_path("POST", "/api/v1/integrations/qq/notify/extra"));
        assert!(public_path("GET", "/api/v1/integrations/aircon/command"));
        assert!(public_path("POST", "/api/v1/integrations/aircon/command"));
        assert!(!public_path(
            "GET",
            "/api/v1/integrations/aircon/command/extra"
        ));
        assert!(public_path("POST", "/api/v1/public/voice-studio/session"));
        assert!(public_path("GET", "/voice-studio"));
        assert!(!public_path("POST", "/api/v1/voice/reference"));
        assert!(voice_studio_path("/voice-studio"));
        assert!(voice_studio_path("/api/v1/public/voice-studio/synthesize"));
        assert!(!voice_studio_path("/api/v1/voice/reference"));
    }
}
