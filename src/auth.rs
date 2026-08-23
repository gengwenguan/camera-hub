use axum::Json;
use axum::body::Body;
use axum::extract::{Extension, Request};
use axum::http::header::{COOKIE, HOST, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const COOKIE_NAME: &str = "camera_hub_session";

#[derive(Clone, Copy)]
pub struct TransportSecurity {
    pub secure: bool,
    pub tls_available: bool,
}

pub struct WebAuth {
    username: String,
    password: String,
    token: String,
    moq_token: String,
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
        }
    }

    pub fn moq_token(&self) -> &str {
        &self.moq_token
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
    if public_path(request.method().as_str(), request.uri().path()) {
        return next.run(request).await;
    }
    let transport = request
        .extensions()
        .get::<TransportSecurity>()
        .copied()
        .unwrap_or(TransportSecurity {
            secure: false,
            tls_available: false,
        });
    if !transport.secure && transport.tls_available {
        return https_redirect(
            request.headers(),
            request.uri().path_and_query().map(|v| v.as_str()),
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

fn public_path(method: &str, path: &str) -> bool {
    method == "OPTIONS"
        || path == "/login"
        || path == "/api/v1/auth/login"
        || path == "/health"
        || path == "/certificate.sha256"
        || path.starts_with("/.well-known/acme-challenge/")
        || (method == "GET" && path.starts_with("/api/v1/devices/") && path.ends_with("/link"))
}

pub async fn login_page(
    Extension(auth): Extension<Arc<WebAuth>>,
    Extension(transport): Extension<TransportSecurity>,
    headers: HeaderMap,
) -> Response {
    if !transport.secure && transport.tls_available {
        return https_redirect(&headers, Some("/login"));
    }
    if auth.authorized(&headers) {
        return redirect("/", StatusCode::SEE_OTHER);
    }
    Html(login_html(false)).into_response()
}

pub async fn login(
    Extension(auth): Extension<Arc<WebAuth>>,
    Extension(transport): Extension<TransportSecurity>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Response {
    if !transport.secure && transport.tls_available {
        return https_redirect(&headers, Some("/login"));
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

fn https_redirect(headers: &HeaderMap, path: Option<&str>) -> Response {
    let host = headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .map(without_port)
        .unwrap_or("mi6.gwghome.site");
    redirect(
        &format!("https://{host}{}", path.unwrap_or("/")),
        StatusCode::TEMPORARY_REDIRECT,
    )
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
        };
        assert!(auth.verify("admin", "12345"));
        assert!(!auth.verify("admin", "bad"));
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("camera_hub_session=token"));
        assert!(auth.authorized(&headers));
    }

    #[test]
    fn strips_http_port_but_preserves_ipv6_brackets() {
        assert_eq!(without_port("mi6.gwghome.site:80"), "mi6.gwghome.site");
        assert_eq!(without_port("[2409::1]:80"), "[2409::1]");
    }
}
