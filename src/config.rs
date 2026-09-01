use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Clone, Debug, Parser)]
#[command(
    version,
    about = "Cross-platform media, AI, recording, and low-latency streaming hub"
)]
pub struct Config {
    #[arg(skip)]
    pub moq_auth_token: String,

    #[arg(long, env = "CAMERA_HUB_WEB_USERNAME", default_value = "admin")]
    pub web_username: String,

    #[arg(long, env = "CAMERA_HUB_WEB_PASSWORD", default_value = "12345")]
    pub web_password: String,

    #[arg(long, env = "CAMERA_HUB_BIND", default_value = "[::]:80")]
    pub bind: SocketAddr,

    #[arg(long, env = "CAMERA_HUB_TLS_BIND", default_value = "[::]:443")]
    pub tls_bind: SocketAddr,

    #[arg(long, env = "CAMERA_HUB_MOQ_ENABLED", default_value_t = false)]
    pub moq_enabled: bool,

    #[arg(long, env = "CAMERA_HUB_MOQ_BIND", default_value = "[::]:443")]
    pub moq_bind: SocketAddr,

    #[arg(
        long,
        env = "CAMERA_HUB_TLS_CERT",
        default_value = "camera-hub-state/cert.pem"
    )]
    pub tls_cert: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_TLS_KEY",
        default_value = "camera-hub-state/key.pem"
    )]
    pub tls_key: PathBuf,

    #[arg(long, env = "CAMERA_HUB_DATA_DIR", default_value = "camera-hub-data")]
    pub data_dir: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_SETTINGS_FILE",
        default_value = "camera-hub-state/settings.json"
    )]
    pub settings_file: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_QQ_CONFIG_FILE",
        default_value = "camera-hub-state/qq.json"
    )]
    pub qq_config_file: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_VOICE_CONFIG_FILE",
        default_value = "camera-hub-state/voice.json"
    )]
    pub voice_config_file: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_VOICE_STATUS_FILE",
        default_value = "camera-hub-state/voice-status.json"
    )]
    pub voice_status_file: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_VOICE_EVENTS_FILE",
        default_value = "camera-hub-data/voice/events.jsonl"
    )]
    pub voice_events_file: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_VOICE_COMMAND_FILE",
        default_value = "camera-hub-state/voice-command.json"
    )]
    pub voice_command_file: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_ACME_WEBROOT",
        default_value = "camera-hub-state/acme"
    )]
    pub acme_webroot: PathBuf,

    #[arg(long, env = "CAMERA_HUB_SEGMENT_SECONDS", default_value_t = 600)]
    pub segment_seconds: u64,

    #[arg(
        long,
        env = "CAMERA_HUB_MAX_BYTES",
        default_value_t = 8 * 1024 * 1024 * 1024
    )]
    pub max_bytes: u64,

    #[arg(long, env = "CAMERA_HUB_RETAIN_DAYS", default_value_t = 7)]
    pub retain_days: u64,

    #[arg(long, env = "CAMERA_HUB_RECORD_ENABLED", default_value_t = true)]
    pub record_enabled: bool,

    #[arg(long, env = "CAMERA_HUB_AI_ENABLED", default_value_t = false)]
    pub ai_enabled: bool,

    #[arg(
        long,
        env = "CAMERA_HUB_AI_RUNTIME",
        default_value = "camera-hub-ai/runtime/lib/libonnxruntime.so"
    )]
    pub ai_runtime: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_AI_MODEL",
        default_value = "camera-hub-ai/models/yolox_nano.onnx"
    )]
    pub ai_model: PathBuf,

    #[arg(long, env = "CAMERA_HUB_AI_INTERVAL_MS", default_value_t = 1000)]
    pub ai_interval_ms: u64,

    #[arg(long, env = "CAMERA_HUB_AI_THRESHOLD", default_value_t = 0.3)]
    pub ai_threshold: f32,

    #[arg(
        long,
        env = "CAMERA_HUB_AI_MIN_PERSON_AREA_RATIO",
        default_value_t = 0.02
    )]
    pub ai_min_person_area_ratio: f32,

    #[arg(long, env = "CAMERA_HUB_AI_MIN_SNAPSHOT_SECONDS", default_value_t = 10)]
    pub ai_min_snapshot_seconds: u64,

    #[arg(long, env = "CAMERA_HUB_AI_SNAPSHOT_MAX_COUNT", default_value_t = 500)]
    pub ai_snapshot_max_count: u64,

    #[arg(long, env = "CAMERA_HUB_AI_SNAPSHOT_QUALITY", default_value_t = 95)]
    pub ai_snapshot_quality: u8,
}

impl Config {
    pub fn normalize(mut self) -> Self {
        self.segment_seconds = self.segment_seconds.clamp(10, 3600);
        self.max_bytes = self
            .max_bytes
            .clamp(64 * 1024 * 1024, 512 * 1024 * 1024 * 1024);
        self.retain_days = self.retain_days.clamp(1, 365);
        self.ai_interval_ms = self.ai_interval_ms.clamp(500, 60_000);
        self.ai_threshold = self.ai_threshold.clamp(0.05, 0.95);
        self.ai_min_person_area_ratio = if self.ai_min_person_area_ratio.is_finite() {
            self.ai_min_person_area_ratio.clamp(0.0, 1.0)
        } else {
            0.02
        };
        self.ai_min_snapshot_seconds = self.ai_min_snapshot_seconds.clamp(1, 3600);
        self.ai_snapshot_max_count = self.ai_snapshot_max_count.clamp(1, 100_000);
        self.ai_snapshot_quality = self.ai_snapshot_quality.clamp(1, 100);
        self
    }
}
