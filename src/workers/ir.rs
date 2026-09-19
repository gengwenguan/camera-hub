use crate::ir::{AirconCommand, IrAction, IrTransmission, PeelIrTransmitter, action_catalog};
use anyhow::{Context, Result, bail};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use serde_json::json;
use std::ffi::OsString;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[derive(Debug, Parser)]
#[command(
    name = "camera-hub worker ir",
    version,
    about = "Loopback-only infrared action service for camera-hub"
)]
struct Args {
    #[arg(long, env = "CAMERA_HUB_IR_BIND", default_value = "127.0.0.1:39182")]
    bind: SocketAddr,

    #[arg(long, env = "CAMERA_HUB_IR_DEVICE", default_value = "/dev/peel_ir")]
    device: PathBuf,

    #[arg(long, env = "CAMERA_HUB_IR_MIN_INTERVAL_MS", default_value_t = 1_000)]
    min_interval_ms: u64,
}

struct IrWorker {
    transmitter: Arc<PeelIrTransmitter>,
    device: PathBuf,
    min_interval: Duration,
    last_transmission: Mutex<Option<Instant>>,
}

pub async fn run(args: Vec<OsString>) -> Result<()> {
    let args = Args::parse_from(args);
    if !is_loopback(args.bind.ip()) {
        bail!("红外 worker 只能监听回环地址");
    }
    let transmitter = Arc::new(PeelIrTransmitter::open(args.device.clone())?);
    let state = Arc::new(IrWorker {
        transmitter,
        device: args.device,
        min_interval: Duration::from_millis(args.min_interval_ms.clamp(250, 10_000)),
        last_transmission: Mutex::new(None),
    });
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/actions/{action}", post(send_action))
        .route("/v1/aircon/state", post(send_aircon))
        .with_state(state);
    let listener = TcpListener::bind(args.bind)
        .await
        .with_context(|| format!("绑定红外 worker {}", args.bind))?;
    axum::serve(listener, app).await.context("运行红外 worker")
}

async fn health(State(state): State<Arc<IrWorker>>) -> Json<serde_json::Value> {
    Json(json!({
        "ok": true,
        "device": state.device,
        "actions": action_catalog(),
    }))
}

async fn send_action(
    State(state): State<Arc<IrWorker>>,
    Path(action): Path<String>,
) -> Result<Json<serde_json::Value>, IrError> {
    let action = IrAction::parse(&action).map_err(IrError::bad_request)?;
    transmit(&state, move |transmitter| transmitter.transmit(action)).await
}

async fn send_aircon(
    State(state): State<Arc<IrWorker>>,
    Json(command): Json<AirconCommand>,
) -> Result<Json<serde_json::Value>, IrError> {
    command.validate().map_err(IrError::bad_request)?;
    transmit(&state, move |transmitter| {
        transmitter.transmit_aircon(&command)
    })
    .await
}

async fn transmit(
    state: &IrWorker,
    send: impl FnOnce(&PeelIrTransmitter) -> Result<IrTransmission> + Send + 'static,
) -> Result<Json<serde_json::Value>, IrError> {
    let mut last = state.last_transmission.lock().await;
    if let Some(previous) = *last
        && previous.elapsed() < state.min_interval
    {
        return Err(IrError::too_many_requests("红外发送过于频繁，请稍后重试"));
    }
    let transmitter = state.transmitter.clone();
    // Reserve the interval before sending: failures can still have emitted a
    // partial waveform, and both APIs must share the same physical send gate.
    *last = Some(Instant::now());
    let result = tokio::task::spawn_blocking(move || send(&transmitter))
        .await
        .map_err(|error| IrError::internal(format!("红外发送任务异常退出：{error}")))?
        .map_err(IrError::internal)?;
    *last = Some(Instant::now());
    Ok(Json(json!({"ok":true,"transmission":result})))
}

fn is_loopback(address: IpAddr) -> bool {
    address.is_loopback()
}

struct IrError {
    status: StatusCode,
    message: String,
}

impl IrError {
    fn bad_request(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: error.to_string(),
        }
    }

    fn too_many_requests(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: message.into(),
        }
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for IrError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"ok":false,"error":self.message}))).into_response()
    }
}
