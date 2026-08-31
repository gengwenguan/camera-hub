mod ai;
mod auth;
mod benchmark;
mod config;
mod flv_live;
mod frames;
mod inference_lock;
mod live;
mod media;
mod moq_live;
mod mux;
mod record_file;
mod settings;
mod state;
mod system;
mod voice;
mod voice_config;
mod web;
mod webrtc_live;

use crate::ai::AiService;
use crate::config::Config;
use crate::frames::FrameHub;
use crate::media::MediaStore;
use crate::settings::{HubSettings, HubSettingsPatch, HubSettingsStore};
use crate::state::{AppState, DeviceHeartbeat};
use crate::voice::VoiceService;
use crate::voice_config::{VoiceConfig, VoiceTestRequest};
use anyhow::{Context, Result};
use axum::body::{Body, Bytes};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router, middleware};
use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::fs;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("camera_hub=info")),
        )
        .init();

    let mut config = Config::parse().normalize();
    let web_auth = Arc::new(auth::WebAuth::new(
        config.web_username.clone(),
        config.web_password.clone(),
    ));
    config.moq_auth_token = web_auth.moq_token().to_owned();
    let settings = Arc::new(HubSettingsStore::load(
        config.settings_file.clone(),
        HubSettings::from_config(&config),
    )?);
    let media = Arc::new(MediaStore::new(config.data_dir.clone(), settings.clone())?);
    let frames = Arc::new(FrameHub::default());
    let ai = AiService::start(&config, settings.clone(), frames.clone())?;
    let voice = Arc::new(VoiceService::load(&config)?);
    let state = Arc::new(AppState::new(
        config.clone(),
        settings,
        media.clone(),
        ai.clone(),
        voice,
        frames,
    ));
    spawn_cleaner(media, ai);

    let app = Router::new()
        .route("/login", get(auth::login_page))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/", get(web::index))
        .route("/app.js", get(web::app))
        .route("/generated/flv-player.js", get(web::flv_player))
        .route("/generated/moq-player.js", get(web::moq_player))
        .route("/generated/evaluation.js", get(web::evaluation))
        .route("/style.css", get(web::style))
        .route("/favicon.svg", get(web::favicon))
        .route("/favicon.ico", get(web::favicon))
        .route("/.well-known/acme-challenge/{token}", get(acme_challenge))
        .route("/certificate.sha256", get(moq_certificate))
        .route(
            "/records/{device_id}/{date}/{*name}",
            get(record_file::record),
        )
        .route("/health", get(health))
        .route("/api/v1/info", get(info))
        .route(
            "/api/v1/settings",
            get(hub_settings).put(update_hub_settings),
        )
        .route("/api/v1/ai/status", get(ai_status))
        .route("/api/v1/voice", get(voice_overview).put(update_voice))
        .route("/api/v1/voice/test", post(test_voice))
        .route("/api/v1/system/status", get(system_status))
        .route("/api/v1/moq/status", get(moq_status))
        .route(
            "/api/v1/benchmark/{session_id}",
            get(benchmark_status).delete(stop_benchmark),
        )
        .route("/api/v1/devices", get(devices))
        .route("/api/v1/devices/{device_id}/link", get(device_link))
        .route("/api/v1/devices/{device_id}/live", get(live_stream))
        .route("/api/v1/devices/{device_id}/live.flv", get(flv_stream))
        .route(
            "/api/v1/devices/{device_id}/webrtc/offer",
            post(relay_webrtc_offer).delete(close_relay_webrtc),
        )
        .route(
            "/api/v1/devices/{device_id}/photos",
            get(ai_photos).delete(delete_ai_photos),
        )
        .route(
            "/api/v1/devices/{device_id}/photos/delete",
            post(delete_selected_ai_photos),
        )
        .route(
            "/api/v1/devices/{device_id}/photos/{name}",
            delete(delete_ai_photo),
        )
        .route("/photos/{device_id}/{name}", get(view_ai_photo))
        .route("/api/v1/media/status", get(media_status))
        .route("/api/v1/devices/{device_id}/records/days", get(record_days))
        .route(
            "/api/v1/devices/{device_id}/records/{date}",
            get(recordings),
        )
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PUT,
                    Method::DELETE,
                    Method::OPTIONS,
                ])
                .allow_headers([CONTENT_TYPE]),
        )
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(auth::require_auth))
        .layer(Extension(web_auth))
        .with_state(state);

    let tls_available = config.tls_cert.is_file() && config.tls_key.is_file();
    let mut tls_task = None;
    if tls_available {
        let tls = RustlsConfig::from_pem_file(&config.tls_cert, &config.tls_key)
            .await
            .context("load camera-hub TLS certificate")?;
        let bind = config.tls_bind;
        let tls_app = app.clone().layer(Extension(auth::TransportSecurity {
            secure: true,
            tls_available: true,
        }));
        info!(%bind, "camera-hub HTTPS started");
        tls_task = Some(tokio::spawn(async move {
            if let Err(error) = axum_server::bind_rustls(bind, tls)
                .serve(tls_app.into_make_service_with_connect_info::<SocketAddr>())
                .await
            {
                error!(%error, %bind, "camera-hub HTTPS stopped");
            }
        }));
    } else {
        info!(
            cert = %config.tls_cert.display(),
            key = %config.tls_key.display(),
            "camera-hub HTTPS disabled because certificate is missing"
        );
    }
    let listener = TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("bind camera-hub on {}", config.bind))?;
    info!(
        bind = %config.bind,
        data_dir = %config.data_dir.display(),
        "camera-hub started"
    );
    axum::serve(
        listener,
        app.layer(Extension(auth::TransportSecurity {
            secure: false,
            tls_available,
        }))
        .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("serve camera-hub")?;
    if let Some(task) = tls_task {
        task.abort();
    }
    Ok(())
}

async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({
        "ok": true,
        "service": "camera-hub",
        "uptime_seconds": state.uptime_seconds()
    }))
}

async fn info(State(state): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, ApiError> {
    let settings = state.settings.current();
    Ok(Json(json!({
        "service": "camera-hub",
        "data_dir": state.config.data_dir,
        "segment_seconds": settings.segment_seconds,
        "max_bytes": settings.max_bytes,
        "retain_days": settings.retain_days,
        "record_enabled": settings.record_enabled,
    })))
}

async fn devices(State(state): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({
        "devices": state.devices().await,
        "frame_links": state.frames.statuses()
    })))
}

async fn device_link(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    websocket: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    validate_device_id(&device_id)?;
    Ok(websocket
        .max_message_size(2 * 1024 * 1024)
        .on_upgrade(move |socket| serve_device_link(socket, state, device_id, remote))
        .into_response())
}

async fn serve_device_link(
    mut socket: WebSocket,
    state: Arc<AppState>,
    device_id: String,
    remote: SocketAddr,
) {
    let generation = state.begin_link(&device_id).await;
    state.frames.reset_clock_sync(&device_id);
    let mut photo_cursor = String::new();
    let mut photo_interval = tokio::time::interval(Duration::from_secs(1));
    let mut clock_interval = tokio::time::interval(Duration::from_secs(2));
    clock_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    photo_interval.tick().await;
    loop {
        if !state.link_is_current(&device_id, generation).await {
            info!(device_id, generation, "superseded device link closed");
            break;
        }
        tokio::select! {
            message = socket.recv() => {
                let Some(Ok(message)) = message else { break };
                let result = match message {
                    Message::Text(text) => {
                        if !state.link_is_current(&device_id, generation).await {
                            break;
                        }
                        handle_device_control(
                            &state,
                            &device_id,
                            remote,
                            &mut photo_cursor,
                            text.as_str(),
                            crate::frames::epoch_us(),
                        ).await
                    }
                    Message::Binary(data) => {
                        if !state.link_is_current(&device_id, generation).await {
                            break;
                        }
                        handle_device_packet(&state, &device_id, &data).await
                    }
                    Message::Ping(data) => socket.send(Message::Pong(data)).await.map_err(Into::into),
                    Message::Close(_) => break,
                    _ => Ok(()),
                };
                if result.is_err() {
                    break;
                }
            }
            _ = photo_interval.tick() => {
                let root = state.config.data_dir.join(&device_id).join("snapshot");
                let after = photo_cursor.clone();
                let name = tokio::task::spawn_blocking(move || next_snapshot_name(&root, &after))
                    .await
                    .ok()
                    .flatten();
                if let Some(name) = name {
                    if let Some(path) = snapshot_path(&state.config.data_dir, &device_id, &name) {
                        if let Ok(jpeg) = tokio::fs::read(path).await {
                            if let Ok(packet) = encode_photo_packet(&name, &jpeg) {
                                if socket.send(Message::Binary(packet.into())).await.is_err() {
                                    break;
                                }
                                photo_cursor = name;
                            }
                        }
                    }
                }
            }
            _ = clock_interval.tick() => {
                let server_send_epoch_us = crate::frames::epoch_us();
                let request = json!({
                    "type": "clock_sync_request",
                    "server_send_epoch_us": server_send_epoch_us,
                });
                if socket.send(Message::Text(request.to_string().into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn handle_device_control(
    state: &AppState,
    device_id: &str,
    remote: SocketAddr,
    photo_cursor: &mut String,
    text: &str,
    server_receive_epoch_us: i64,
) -> Result<()> {
    let value = serde_json::from_str::<serde_json::Value>(text)?;
    match value.get("type").and_then(|value| value.as_str()) {
        Some("hello") => {
            let heartbeat = DeviceHeartbeat {
                firmware: value
                    .get("firmware")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_owned(),
                ipv6: value
                    .get("ipv6")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_owned(),
            };
            if let Some(last_photo) = value
                .get("last_photo")
                .and_then(|value| value.as_str())
                .filter(|name| name.is_empty() || valid_snapshot_name(name))
            {
                photo_cursor.clear();
                photo_cursor.push_str(last_photo);
            }
            state.heartbeat(device_id, heartbeat, remote).await;
        }
        Some("clock_sync_response") => {
            let number = |name: &str| value.get(name).and_then(|value| value.as_i64());
            if let (Some(server_send), Some(source_receive), Some(source_send)) = (
                number("server_send_epoch_us"),
                number("source_receive_epoch_us"),
                number("source_send_epoch_us"),
            ) {
                state.frames.update_clock_sync(
                    device_id,
                    server_send,
                    source_receive,
                    source_send,
                    server_receive_epoch_us,
                );
            }
        }
        _ => {}
    }
    Ok(())
}

async fn handle_device_packet(state: &AppState, device_id: &str, data: &[u8]) -> Result<()> {
    let (kind, flags, sequence, pts_us, capture_epoch_us, payload) = decode_link_packet(data)?;
    if !matches!(kind, 1 | 2) {
        anyhow::bail!("unsupported frame kind {kind}");
    }
    state.frames.push(
        device_id,
        kind,
        sequence,
        pts_us,
        capture_epoch_us,
        flags & 1 != 0,
        Arc::from(payload),
    );
    state.muxers.ensure(device_id).await;
    state.moq.ensure(device_id).await;
    Ok(())
}

fn decode_link_packet(data: &[u8]) -> Result<(u8, u16, u32, i64, Option<i64>, &[u8])> {
    if data.len() < 24 || &data[..4] != b"CHP1" || !matches!(data[5], 1 | 2) {
        anyhow::bail!("invalid frame link packet");
    }
    let version = data[5];
    let header_len = if version == 2 { 32 } else { 24 };
    if data.len() < header_len {
        anyhow::bail!("short frame link packet");
    }
    let flags = u16::from_be_bytes([data[6], data[7]]);
    let sequence = u32::from_be_bytes(data[8..12].try_into()?);
    let pts_us = i64::from_be_bytes(data[12..20].try_into()?);
    let length = u32::from_be_bytes(data[20..24].try_into()?) as usize;
    if data.len() != header_len + length {
        anyhow::bail!("invalid frame link payload length");
    }
    let capture_epoch_us = (version == 2)
        .then(|| i64::from_be_bytes(data[24..32].try_into().expect("v2 header length checked")));
    Ok((
        data[4],
        flags,
        sequence,
        pts_us,
        capture_epoch_us,
        &data[header_len..],
    ))
}

fn encode_photo_packet(name: &str, jpeg: &[u8]) -> Result<Vec<u8>> {
    let name_len = u16::try_from(name.len())?;
    let payload_len = 2usize + name.len() + jpeg.len();
    let mut output = Vec::with_capacity(24 + payload_len);
    output.extend_from_slice(b"CHP1");
    output.push(0x81);
    output.push(1);
    output.extend_from_slice(&0u16.to_be_bytes());
    output.extend_from_slice(&0u32.to_be_bytes());
    output.extend_from_slice(&0i64.to_be_bytes());
    output.extend_from_slice(&(payload_len as u32).to_be_bytes());
    output.extend_from_slice(&name_len.to_be_bytes());
    output.extend_from_slice(name.as_bytes());
    output.extend_from_slice(jpeg);
    Ok(output)
}

#[derive(Default, Deserialize)]
struct BenchmarkStreamQuery {
    benchmark: Option<String>,
}

async fn live_stream(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    Query(query): Query<BenchmarkStreamQuery>,
    websocket: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    validate_device_id(&device_id)?;
    let subscription = state
        .live
        .subscribe(&device_id)
        .ok_or_else(|| ApiError::status(StatusCode::NOT_FOUND, "live stream is not initialized"))?;
    Ok(websocket
        .max_message_size(2 * 1024 * 1024)
        .on_upgrade(move |socket| {
            serve_live(
                socket,
                subscription,
                state.benchmark.clone(),
                query.benchmark,
            )
        })
        .into_response())
}

async fn flv_stream(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    Query(query): Query<BenchmarkStreamQuery>,
) -> Result<Response, ApiError> {
    validate_device_id(&device_id)?;
    let subscription = state.frames.subscribe(&device_id).ok_or_else(|| {
        ApiError::status(StatusCode::NOT_FOUND, "device frame stream is not ready")
    })?;
    let output = state
        .flv
        .open(&device_id, subscription, query.benchmark)
        .await
        .map_err(|error| {
            ApiError::status(
                StatusCode::SERVICE_UNAVAILABLE,
                format!("start HTTP-FLV stream: {error:#}"),
            )
        })?;
    let mut response = Response::new(Body::from_stream(ReaderStream::new(output.stream)));
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("video/x-flv"));
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );
    response.headers_mut().insert(
        HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );
    Ok(response)
}

#[derive(Deserialize)]
struct WebRtcOffer {
    sdp: String,
    #[serde(default)]
    benchmark: Option<String>,
}

async fn relay_webrtc_offer(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    Json(offer): Json<WebRtcOffer>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_device_id(&device_id)?;
    let answer = state
        .webrtc
        .answer(&device_id, offer.sdp, offer.benchmark)
        .await?;
    Ok(Json(json!({"sdp":answer})))
}

async fn close_relay_webrtc(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    validate_device_id(&device_id)?;
    state.webrtc.close(&device_id).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn serve_live(
    mut socket: WebSocket,
    mut subscription: crate::live::LiveSubscription,
    benchmark: Arc<crate::benchmark::BenchmarkRegistry>,
    benchmark_id: Option<String>,
) {
    if socket
        .send(Message::Binary(Bytes::copy_from_slice(&subscription.init)))
        .await
        .is_err()
    {
        return;
    }
    loop {
        match subscription.receiver.recv().await {
            Ok(fragment) => {
                if let (Some(session_id), Some(anchor)) =
                    (benchmark_id.as_deref(), fragment.anchor.clone())
                {
                    benchmark.set_anchor_value(session_id, "mse", anchor);
                }
                if socket
                    .send(Message::Binary(Bytes::copy_from_slice(&fragment.data)))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}

#[derive(Deserialize)]
struct BenchmarkStatusQuery {
    device_id: String,
    after: Option<u32>,
}

async fn benchmark_status(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(query): Query<BenchmarkStatusQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !crate::benchmark::valid_session_id(&session_id) {
        return Err(ApiError::status(
            StatusCode::BAD_REQUEST,
            "invalid benchmark session id",
        ));
    }
    validate_device_id(&query.device_id)?;
    Ok(Json(json!({
        "session_id": session_id,
        "device_id": query.device_id,
        "server_epoch_us": crate::frames::epoch_us(),
        "source_clock": state.frames.clock_sync(&query.device_id),
        "anchors": state.benchmark.anchors(&session_id),
        "frames": state.frames.video_clock(&query.device_id, query.after),
    })))
}

async fn stop_benchmark(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> StatusCode {
    state.benchmark.remove(&session_id);
    StatusCode::NO_CONTENT
}

#[derive(Serialize)]
struct AiPhotoInfo {
    name: String,
    size: u64,
    modified_epoch: u64,
}

#[derive(Default, Serialize)]
struct DeletedAiPhotos {
    files: u64,
    bytes: u64,
}

#[derive(Deserialize)]
struct DeleteAiPhotosRequest {
    names: Vec<String>,
}

#[derive(Default, Serialize)]
struct DeletedSelectedAiPhotos {
    deleted: u64,
    bytes: u64,
    missing: u64,
    errors: Vec<String>,
}

async fn ai_photos(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_device_id(&device_id)?;
    let root = state.config.data_dir.join(&device_id).join("snapshot");
    let photos = tokio::task::spawn_blocking(move || list_snapshots(&root))
        .await
        .context("join snapshot list task")?;
    Ok(Json(json!({"device_id":device_id,"photos":photos})))
}

async fn delete_ai_photo(
    State(state): State<Arc<AppState>>,
    Path((device_id, name)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    validate_device_id(&device_id)?;
    let path = snapshot_path(&state.config.data_dir, &device_id, &name)
        .ok_or_else(|| ApiError::status(StatusCode::BAD_REQUEST, "invalid snapshot name"))?;
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {
            if let Some(parent) = path.parent() {
                let _ = tokio::fs::remove_dir(parent).await;
            }
            Ok(StatusCode::NO_CONTENT)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(ApiError::status(
            StatusCode::NOT_FOUND,
            "snapshot not found",
        )),
        Err(error) => Err(error.into()),
    }
}

async fn delete_ai_photos(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_device_id(&device_id)?;
    let root = state.config.data_dir.join(&device_id).join("snapshot");
    let deleted = tokio::task::spawn_blocking(move || clear_snapshots(&root))
        .await
        .context("join snapshot deletion task")??;
    Ok(Json(json!({
        "ok": true,
        "device_id": device_id,
        "deleted": deleted,
    })))
}

async fn delete_selected_ai_photos(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    Json(request): Json<DeleteAiPhotosRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_device_id(&device_id)?;
    if request.names.is_empty() || request.names.len() > 1000 {
        return Err(ApiError::status(
            StatusCode::BAD_REQUEST,
            "photo selection must contain 1 to 1000 names",
        ));
    }
    let mut seen = HashSet::with_capacity(request.names.len());
    let mut photos = Vec::with_capacity(request.names.len());
    for name in request.names {
        if !seen.insert(name.clone()) {
            continue;
        }
        let path = snapshot_path(&state.config.data_dir, &device_id, &name)
            .ok_or_else(|| ApiError::status(StatusCode::BAD_REQUEST, "invalid snapshot name"))?;
        photos.push((name, path));
    }
    let deleted = tokio::task::spawn_blocking(move || delete_selected_snapshots(photos))
        .await
        .context("join selected snapshot deletion task")?;
    Ok(Json(json!({
        "ok": deleted.errors.is_empty(),
        "device_id": device_id,
        "deleted": deleted,
    })))
}

async fn view_ai_photo(
    State(state): State<Arc<AppState>>,
    Path((device_id, name)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    snapshot_response(&state, &device_id, &name).await
}

async fn snapshot_response(
    state: &AppState,
    device_id: &str,
    name: &str,
) -> Result<Response, ApiError> {
    validate_device_id(&device_id)?;
    let path = snapshot_path(&state.config.data_dir, &device_id, &name)
        .ok_or_else(|| ApiError::status(StatusCode::BAD_REQUEST, "invalid snapshot name"))?;
    let data = tokio::fs::read(path)
        .await
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                ApiError::status(StatusCode::NOT_FOUND, "snapshot not found")
            }
            _ => ApiError::from(error),
        })?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "image/jpeg")
        .header(CONTENT_LENGTH, data.len())
        .body(Body::from(data))?)
}

async fn media_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(json!({"devices":state.media.statuses()})))
}

async fn ai_status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({"ai":state.ai.status()}))
}

async fn voice_overview(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let (config_path, status_path, events_path) = state.voice.paths();
    Json(json!({
        "config": state.voice.current(),
        "status": state.voice.status(),
        "events": state.voice.events(50),
        "paths": {
            "config": config_path,
            "status": status_path,
            "events": events_path,
        }
    }))
}

async fn update_voice(
    State(state): State<Arc<AppState>>,
    Json(config): Json<VoiceConfig>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let config = state.voice.update(config)?;
    Ok(Json(json!({"ok":true,"config":config})))
}

async fn test_voice(
    State(state): State<Arc<AppState>>,
    Json(request): Json<VoiceTestRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    state.voice.queue_test(request)?;
    Ok((StatusCode::ACCEPTED, Json(json!({"ok":true}))))
}

async fn system_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let monitor = state.system.clone();
    let process_uptime = state.uptime_seconds();
    let status = tokio::task::spawn_blocking(move || monitor.sample(process_uptime))
        .await
        .context("join system status task")?;
    Ok(Json(serde_json::to_value(status)?))
}

async fn moq_status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({"moq":state.moq.status().await}))
}

async fn moq_certificate(State(state): State<Arc<AppState>>) -> Result<Response, ApiError> {
    let status = state.moq.status().await;
    let fingerprint = status
        .fingerprints
        .first()
        .ok_or_else(|| ApiError::status(StatusCode::NOT_FOUND, "MoQ certificate is unavailable"))?;
    let mut body = fingerprint.clone();
    body.push(char::from(10));
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(CACHE_CONTROL, "no-store")
        .body(Body::from(body))?)
}

async fn acme_challenge(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<Response, ApiError> {
    if token.is_empty()
        || token.len() > 256
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(ApiError::status(
            StatusCode::BAD_REQUEST,
            "invalid ACME challenge token",
        ));
    }
    let path = state
        .config
        .acme_webroot
        .join(".well-known")
        .join("acme-challenge")
        .join(token);
    let data = tokio::fs::read(path)
        .await
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                ApiError::status(StatusCode::NOT_FOUND, "ACME challenge not found")
            }
            _ => ApiError::from(error),
        })?;
    if data.len() > 16 * 1024 {
        return Err(ApiError::status(
            StatusCode::PAYLOAD_TOO_LARGE,
            "ACME challenge is too large",
        ));
    }
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(CACHE_CONTROL, "no-store")
        .header(CONTENT_LENGTH, data.len())
        .body(Body::from(data))?)
}

async fn hub_settings(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({
        "settings": state.settings.current(),
        "deployment": {
            "bind": state.config.bind,
            "tls_bind": state.config.tls_bind,
            "moq_enabled": state.config.moq_enabled,
            "moq_bind": state.config.moq_bind,
            "acme_webroot": state.config.acme_webroot,
            "data_dir": state.config.data_dir,
            "settings_file": state.settings.path(),
            "ai_runtime": state.config.ai_runtime,
            "ai_model": state.config.ai_model,
            "voice_config_file": state.config.voice_config_file,
            "voice_status_file": state.config.voice_status_file,
            "voice_events_file": state.config.voice_events_file,
        }
    }))
}

async fn update_hub_settings(
    State(state): State<Arc<AppState>>,
    Json(patch): Json<HubSettingsPatch>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let settings = state.settings.update(patch)?;
    let ai = state.ai.clone();
    let snapshot_cleanup = tokio::task::spawn_blocking(move || ai.clean_snapshots())
        .await
        .context("join AI snapshot cleanup task")??;
    Ok(Json(json!({
        "ok": true,
        "settings": settings,
        "snapshot_cleanup": snapshot_cleanup,
    })))
}

async fn record_days(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_device_id(&device_id)?;
    let media = state.media.clone();
    let id = device_id.clone();
    let days = tokio::task::spawn_blocking(move || media.record_days(&id))
        .await
        .context("join record days task")??;
    Ok(Json(json!({"device_id":device_id,"days":days})))
}

async fn recordings(
    State(state): State<Arc<AppState>>,
    Path((device_id, date)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    validate_device_id(&device_id)?;
    let media = state.media.clone();
    let id = device_id.clone();
    let record_date = date.clone();
    let records = tokio::task::spawn_blocking(move || media.recordings(&id, &record_date))
        .await
        .context("join recordings task")??;
    Ok(Json(
        json!({"device_id":device_id,"date":date,"records":records}),
    ))
}

fn validate_device_id(device_id: &str) -> Result<(), ApiError> {
    if !device_id.is_empty()
        && device_id.len() <= 64
        && device_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        Ok(())
    } else {
        Err(ApiError::status(
            StatusCode::BAD_REQUEST,
            "invalid device id",
        ))
    }
}

fn next_snapshot_name(root: &std::path::Path, after: &str) -> Option<String> {
    let mut names = fs::read_dir(root)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .flat_map(|entry| fs::read_dir(entry.path()).into_iter().flatten().flatten())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (entry.path().is_file() && valid_snapshot_name(&name)).then_some(name)
        })
        .collect::<Vec<_>>();
    names.sort();
    if after.is_empty() {
        names.pop()
    } else {
        names.into_iter().find(|name| name.as_str() > after)
    }
}

fn list_snapshots(root: &std::path::Path) -> Vec<AiPhotoInfo> {
    let mut photos = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .flat_map(|entry| fs::read_dir(entry.path()).into_iter().flatten().flatten())
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if !path.is_file() || !valid_snapshot_name(&name) {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            Some(AiPhotoInfo {
                name,
                size: metadata.len(),
                modified_epoch: metadata
                    .modified()
                    .unwrap_or(std::time::UNIX_EPOCH)
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            })
        })
        .collect::<Vec<_>>();
    photos.sort_by(|left, right| right.name.cmp(&left.name));
    photos
}

fn clear_snapshots(root: &std::path::Path) -> Result<DeletedAiPhotos> {
    let mut deleted = DeletedAiPhotos::default();
    let days = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(deleted),
        Err(error) => return Err(error.into()),
    };
    for day in days.flatten().filter(|entry| entry.path().is_dir()) {
        for entry in fs::read_dir(day.path())?.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if !path.is_file() || !valid_snapshot_name(&name) {
                continue;
            }
            let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
            match fs::remove_file(&path) {
                Ok(()) => {
                    deleted.files = deleted.files.saturating_add(1);
                    deleted.bytes = deleted.bytes.saturating_add(size);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        let _ = fs::remove_dir(day.path());
    }
    Ok(deleted)
}

fn delete_selected_snapshots(photos: Vec<(String, std::path::PathBuf)>) -> DeletedSelectedAiPhotos {
    let mut result = DeletedSelectedAiPhotos::default();
    for (name, path) in photos {
        let size = path.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        match fs::remove_file(&path) {
            Ok(()) => {
                result.deleted = result.deleted.saturating_add(1);
                result.bytes = result.bytes.saturating_add(size);
                if let Some(parent) = path.parent() {
                    let _ = fs::remove_dir(parent);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                result.missing = result.missing.saturating_add(1);
            }
            Err(_) => result.errors.push(name),
        }
    }
    result
}

fn snapshot_path(
    root: &std::path::Path,
    device_id: &str,
    name: &str,
) -> Option<std::path::PathBuf> {
    if !valid_snapshot_name(name) {
        return None;
    }
    Some(
        root.join(device_id)
            .join("snapshot")
            .join(name.get(..8)?)
            .join(name),
    )
}

fn valid_snapshot_name(name: &str) -> bool {
    name.ends_with(".jpg")
        && name.len() <= 64
        && name
            .get(..8)
            .is_some_and(|date| date.len() == 8 && date.bytes().all(|byte| byte.is_ascii_digit()))
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn spawn_cleaner(media: Arc<MediaStore>, ai: Arc<AiService>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            let media = media.clone();
            match tokio::task::spawn_blocking(move || media.clean()).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => error!(%error, "media cleanup failed"),
                Err(error) => error!(%error, "media cleanup task failed"),
            }
            let ai = ai.clone();
            match tokio::task::spawn_blocking(move || ai.clean_snapshots()).await {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => error!(%error, "AI snapshot cleanup failed"),
                Err(error) => error!(%error, "AI snapshot cleanup task failed"),
            }
        }
    });
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    info!("shutdown signal received");
}

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn status(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl<E> From<E> for ApiError
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        let error = error.into();
        Self {
            status: StatusCode::BAD_REQUEST,
            message: format!("{error:#}"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"ok":false,"error":self.message}))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::{clear_snapshots, decode_link_packet, delete_selected_snapshots};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn packet(version: u8, capture_epoch_us: Option<i64>) -> Vec<u8> {
        let payload = [1u8, 2, 3];
        let mut packet = Vec::new();
        packet.extend_from_slice(b"CHP1");
        packet.push(1);
        packet.push(version);
        packet.extend_from_slice(&1u16.to_be_bytes());
        packet.extend_from_slice(&7u32.to_be_bytes());
        packet.extend_from_slice(&123_456i64.to_be_bytes());
        packet.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        if let Some(value) = capture_epoch_us {
            packet.extend_from_slice(&value.to_be_bytes());
        }
        packet.extend_from_slice(&payload);
        packet
    }

    #[test]
    fn decodes_chp1_v1_without_source_clock() {
        let packet = packet(1, None);
        let decoded = decode_link_packet(&packet).unwrap();
        assert_eq!(decoded.0, 1);
        assert_eq!(decoded.2, 7);
        assert_eq!(decoded.3, 123_456);
        assert_eq!(decoded.4, None);
        assert_eq!(decoded.5, [1, 2, 3]);
    }

    #[test]
    fn decodes_chp1_v2_capture_clock() {
        let packet = packet(2, Some(1_765_000_000_123_456));
        let decoded = decode_link_packet(&packet).unwrap();
        assert_eq!(decoded.4, Some(1_765_000_000_123_456));
        assert_eq!(decoded.5, [1, 2, 3]);
    }

    #[test]
    fn clears_only_managed_ai_snapshots() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "camera-hub-clear-snapshots-{}-{nonce}",
            std::process::id()
        ));
        let day = root.join("20260814");
        fs::create_dir_all(&day).unwrap();
        fs::write(day.join("20260814_100000_000.jpg"), b"photo").unwrap();
        fs::write(day.join("notes.txt"), b"keep").unwrap();

        let deleted = clear_snapshots(&root).unwrap();

        assert_eq!(deleted.files, 1);
        assert_eq!(deleted.bytes, 5);
        assert!(day.join("notes.txt").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deletes_only_selected_ai_snapshots() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "camera-hub-selected-snapshots-{}-{nonce}",
            std::process::id()
        ));
        let day = root.join("20260814");
        fs::create_dir_all(&day).unwrap();
        let selected = day.join("20260814_100000_000.jpg");
        let retained = day.join("20260814_100001_000.jpg");
        fs::write(&selected, b"first").unwrap();
        fs::write(&retained, b"second").unwrap();

        let deleted = delete_selected_snapshots(vec![
            ("20260814_100000_000.jpg".to_owned(), selected.clone()),
            (
                "20260814_100002_000.jpg".to_owned(),
                day.join("20260814_100002_000.jpg"),
            ),
        ]);

        assert_eq!(deleted.deleted, 1);
        assert_eq!(deleted.bytes, 5);
        assert_eq!(deleted.missing, 1);
        assert!(deleted.errors.is_empty());
        assert!(!selected.exists());
        assert!(retained.is_file());
        fs::remove_dir_all(root).unwrap();
    }
}
