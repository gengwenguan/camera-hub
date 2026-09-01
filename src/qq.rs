use crate::config::Config;
use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Instant, interval_at, sleep, timeout};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

const QQ_CONFIG_VERSION: u32 = 1;
const QQ_API_BASE: &str = "https://api.bot.qq.com";
const GROUP_AND_C2C_INTENT: u64 = 1 << 25;
const NOTIFY_QUEUE_CAPACITY: usize = 64;
const MESSAGE_CHUNK_BYTES: usize = 1800;
const MESSAGE_MAX_CHARS: usize = 12_000;

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
struct QqConfig {
    version: u32,
    revision: u64,
    enabled: bool,
    app_id: String,
    app_secret: String,
    default_group: String,
    push_token_sha256: String,
    groups: Vec<QqGroup>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct QqGroup {
    pub openid: String,
    pub name: String,
    pub added_epoch: u64,
}

#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct QqConfigUpdate {
    pub enabled: bool,
    pub app_id: String,
    pub app_secret: String,
    pub clear_secret: bool,
    pub default_group: String,
    pub group_aliases: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct QqPublicConfig {
    pub revision: u64,
    pub enabled: bool,
    pub app_id: String,
    pub secret_configured: bool,
    pub default_group: String,
    pub push_token_configured: bool,
    pub groups: Vec<QqGroup>,
}

#[derive(Clone, Debug, Serialize)]
pub struct QqStatus {
    pub state: String,
    pub online: bool,
    pub detail: String,
    pub bot_name: String,
    pub token_expires_epoch: u64,
    pub connected_epoch: u64,
    pub last_event_epoch: u64,
    pub last_send_epoch: u64,
    pub sent_count: u64,
    pub failed_count: u64,
    pub reconnect_count: u64,
    pub last_error: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct QqOverview {
    pub config: QqPublicConfig,
    pub status: QqStatus,
    pub config_path: PathBuf,
    pub push_endpoint: &'static str,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct QqNotifyRequest {
    pub target: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct QqNotifyReceipt {
    pub target: String,
    pub messages: usize,
}

#[derive(Debug)]
pub enum QqNotifyError {
    Invalid(String),
    Unauthorized,
    Unavailable(String),
    Delivery(String),
}

impl std::fmt::Display for QqNotifyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) | Self::Unavailable(message) | Self::Delivery(message) => {
                formatter.write_str(message)
            }
            Self::Unauthorized => formatter.write_str("invalid QQ push token"),
        }
    }
}

impl std::error::Error for QqNotifyError {}

impl Default for QqConfig {
    fn default() -> Self {
        Self {
            version: QQ_CONFIG_VERSION,
            revision: 1,
            enabled: false,
            app_id: String::new(),
            app_secret: String::new(),
            default_group: String::new(),
            push_token_sha256: String::new(),
            groups: Vec::new(),
        }
    }
}

impl Default for QqNotifyRequest {
    fn default() -> Self {
        Self {
            target: "default".to_owned(),
            content: String::new(),
        }
    }
}

impl Default for QqStatus {
    fn default() -> Self {
        Self {
            state: "disabled".to_owned(),
            online: false,
            detail: "QQ 机器人未启用".to_owned(),
            bot_name: String::new(),
            token_expires_epoch: 0,
            connected_epoch: 0,
            last_event_epoch: 0,
            last_send_epoch: 0,
            sent_count: 0,
            failed_count: 0,
            reconnect_count: 0,
            last_error: String::new(),
        }
    }
}

impl QqConfig {
    fn normalize(mut self) -> Result<Self> {
        self.version = QQ_CONFIG_VERSION;
        self.app_id = self.app_id.trim().to_owned();
        self.app_secret = self.app_secret.trim().to_owned();
        self.default_group = self.default_group.trim().to_owned();
        self.push_token_sha256 = self.push_token_sha256.trim().to_ascii_lowercase();

        if !self.app_id.is_empty()
            && (self.app_id.len() > 32 || !self.app_id.bytes().all(|byte| byte.is_ascii_digit()))
        {
            bail!("AppID 必须是 1-32 位数字");
        }
        if !self.app_secret.is_empty()
            && (self.app_secret.len() > 256 || self.app_secret.chars().any(char::is_control))
        {
            bail!("AppSecret 格式无效");
        }
        if !self.push_token_sha256.is_empty()
            && (self.push_token_sha256.len() != 64
                || !self
                    .push_token_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit()))
        {
            bail!("推送 Token 摘要格式无效");
        }

        let mut openids = HashSet::new();
        self.groups.retain_mut(|group| {
            group.openid = group.openid.trim().to_owned();
            group.name = clean_group_name(&group.name, &group.openid);
            valid_group_openid(&group.openid) && openids.insert(group.openid.clone())
        });
        if !self.default_group.is_empty()
            && !self
                .groups
                .iter()
                .any(|group| group.openid == self.default_group)
        {
            self.default_group.clear();
        }
        if self.enabled && (self.app_id.is_empty() || self.app_secret.is_empty()) {
            bail!("启用 QQ 机器人前必须配置 AppID 和 AppSecret");
        }
        Ok(self)
    }

    fn public(&self) -> QqPublicConfig {
        QqPublicConfig {
            revision: self.revision,
            enabled: self.enabled,
            app_id: self.app_id.clone(),
            secret_configured: !self.app_secret.is_empty(),
            default_group: self.default_group.clone(),
            push_token_configured: !self.push_token_sha256.is_empty(),
            groups: self.groups.clone(),
        }
    }

    fn ready(&self) -> bool {
        self.enabled && !self.app_id.is_empty() && !self.app_secret.is_empty()
    }

    fn resolve_group(&self, target: &str) -> Result<&QqGroup> {
        let target = target.trim();
        let openid = if target.is_empty() || target == "default" {
            if self.default_group.is_empty() {
                bail!("尚未配置默认 QQ 群");
            }
            self.default_group.as_str()
        } else {
            self.groups
                .iter()
                .find(|group| group.name == target)
                .map(|group| group.openid.as_str())
                .unwrap_or(target)
        };
        self.groups
            .iter()
            .find(|group| group.openid == openid)
            .ok_or_else(|| anyhow::anyhow!("目标 QQ 群未登记"))
    }
}

struct QqConfigStore {
    path: PathBuf,
    current: QqConfig,
}

impl QqConfigStore {
    fn load(path: PathBuf) -> Result<Self> {
        let current = match fs::read(&path) {
            Ok(data) => serde_json::from_slice::<QqConfig>(&data)
                .with_context(|| format!("parse QQ config {}", path.display()))?
                .normalize()?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                QqConfig::default().normalize()?
            }
            Err(error) => return Err(error.into()),
        };
        let store = Self { path, current };
        store.save(&store.current)?;
        Ok(store)
    }

    fn update(&mut self, update: QqConfigUpdate) -> Result<QqConfig> {
        let mut next = self.current.clone();
        next.enabled = update.enabled;
        let app_id = update.app_id.trim().to_owned();
        if app_id != next.app_id && update.app_secret.trim().is_empty() {
            next.app_secret.clear();
        }
        next.app_id = app_id;
        if update.clear_secret {
            next.app_secret.clear();
            next.enabled = false;
        } else if !update.app_secret.trim().is_empty() {
            next.app_secret = update.app_secret;
        }
        next.default_group = update.default_group;
        for group in &mut next.groups {
            if let Some(name) = update.group_aliases.get(&group.openid) {
                group.name = clean_group_name(name, &group.openid);
            }
        }
        next.revision = next.revision.saturating_add(1);
        next = next.normalize()?;
        self.save(&next)?;
        self.current = next.clone();
        Ok(next)
    }

    fn set_push_token(&mut self, digest: String) -> Result<QqConfig> {
        let mut next = self.current.clone();
        next.push_token_sha256 = digest;
        next.revision = next.revision.saturating_add(1);
        next = next.normalize()?;
        self.save(&next)?;
        self.current = next.clone();
        Ok(next)
    }

    fn discover_group(&mut self, openid: &str) -> Result<Option<QqConfig>> {
        if !valid_group_openid(openid) || self.current.groups.iter().any(|g| g.openid == openid) {
            return Ok(None);
        }
        let mut next = self.current.clone();
        next.groups.push(QqGroup {
            openid: openid.to_owned(),
            name: default_group_name(openid),
            added_epoch: epoch_seconds(),
        });
        if next.default_group.is_empty() {
            next.default_group = openid.to_owned();
        }
        next.revision = next.revision.saturating_add(1);
        self.save(&next)?;
        self.current = next.clone();
        Ok(Some(next))
    }

    fn remove_group(&mut self, openid: &str) -> Result<Option<QqConfig>> {
        let mut next = self.current.clone();
        let before = next.groups.len();
        next.groups.retain(|group| group.openid != openid);
        if next.groups.len() == before {
            return Ok(None);
        }
        if next.default_group == openid {
            next.default_group = next
                .groups
                .first()
                .map(|group| group.openid.clone())
                .unwrap_or_default();
        }
        next.revision = next.revision.saturating_add(1);
        self.save(&next)?;
        self.current = next.clone();
        Ok(Some(next))
    }

    fn save(&self, config: &QqConfig) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(config)?)?;
        secure_file(&temporary)?;
        fs::rename(&temporary, &self.path)?;
        secure_file(&self.path)?;
        Ok(())
    }
}

struct NotifyJob {
    request: QqNotifyRequest,
    response: oneshot::Sender<std::result::Result<QqNotifyReceipt, String>>,
}

pub struct QqService {
    store: Mutex<QqConfigStore>,
    status: RwLock<QqStatus>,
    config_tx: watch::Sender<QqConfig>,
    notify_tx: mpsc::Sender<NotifyJob>,
    http: reqwest::Client,
}

impl QqService {
    pub fn start(config: &Config) -> Result<Arc<Self>> {
        let store = QqConfigStore::load(config.qq_config_file.clone())?;
        let initial = store.current.clone();
        let (config_tx, config_rx) = watch::channel(initial);
        let (notify_tx, notify_rx) = mpsc::channel(NOTIFY_QUEUE_CAPACITY);
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(15))
            .build()
            .context("build QQ HTTP client")?;
        let service = Arc::new(Self {
            store: Mutex::new(store),
            status: RwLock::new(QqStatus::default()),
            config_tx,
            notify_tx,
            http,
        });
        tokio::spawn(run_worker(service.clone(), config_rx, notify_rx));
        Ok(service)
    }

    pub fn overview(&self) -> QqOverview {
        let store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        QqOverview {
            config: store.current.public(),
            status: self.status(),
            config_path: store.path.clone(),
            push_endpoint: "/api/v1/integrations/qq/notify",
        }
    }

    pub fn update(&self, update: QqConfigUpdate) -> Result<QqPublicConfig> {
        let next = self
            .store
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .update(update)?;
        self.config_tx.send_replace(next.clone());
        Ok(next.public())
    }

    pub fn rotate_push_token(&self) -> Result<String> {
        let mut random = [0u8; 32];
        getrandom::fill(&mut random).context("generate QQ push token")?;
        let token = format!("chq_{}", hex::encode(random));
        let digest = token_digest(&token);
        self.store
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .set_push_token(digest)?;
        Ok(token)
    }

    pub fn verify_push_token(&self, token: &str) -> bool {
        let expected = self
            .store
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .current
            .push_token_sha256
            .clone();
        !expected.is_empty()
            && constant_time_eq(expected.as_bytes(), token_digest(token).as_bytes())
    }

    pub fn status(&self) -> QqStatus {
        self.status
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub async fn notify(
        &self,
        mut request: QqNotifyRequest,
    ) -> std::result::Result<QqNotifyReceipt, QqNotifyError> {
        request.target = request.target.trim().to_owned();
        request.content = request.content.trim().to_owned();
        let count = request.content.chars().count();
        if count == 0 {
            return Err(QqNotifyError::Invalid("消息内容不能为空".to_owned()));
        }
        if count > MESSAGE_MAX_CHARS {
            return Err(QqNotifyError::Invalid(format!(
                "消息内容不能超过 {MESSAGE_MAX_CHARS} 个字符"
            )));
        }
        if !self.status().online {
            return Err(QqNotifyError::Unavailable("QQ 机器人尚未在线".to_owned()));
        }
        let (response, receiver) = oneshot::channel();
        self.notify_tx
            .try_send(NotifyJob { request, response })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => {
                    QqNotifyError::Unavailable("QQ 消息队列已满".to_owned())
                }
                mpsc::error::TrySendError::Closed(_) => {
                    QqNotifyError::Unavailable("QQ 消息服务已停止".to_owned())
                }
            })?;
        match timeout(Duration::from_secs(120), receiver).await {
            Ok(Ok(Ok(receipt))) => Ok(receipt),
            Ok(Ok(Err(message))) => Err(QqNotifyError::Delivery(message)),
            Ok(Err(_)) => Err(QqNotifyError::Unavailable("QQ 消息服务已断开".to_owned())),
            Err(_) => Err(QqNotifyError::Unavailable(
                "等待 QQ 消息发送超时".to_owned(),
            )),
        }
    }

    fn update_status(&self, update: impl FnOnce(&mut QqStatus)) {
        update(
            &mut self
                .status
                .write()
                .unwrap_or_else(|error| error.into_inner()),
        );
    }

    fn current_config(&self) -> QqConfig {
        self.store
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .current
            .clone()
    }

    fn discover_group(&self, openid: &str) {
        let result = self
            .store
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .discover_group(openid);
        match result {
            Ok(Some(_)) => info!(group_openid = %openid, "QQ group discovered"),
            Ok(None) => {}
            Err(error) => warn!(%error, "save discovered QQ group failed"),
        }
    }

    fn remove_group(&self, openid: &str) {
        let result = self
            .store
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove_group(openid);
        match result {
            Ok(Some(_)) => info!(group_openid = %openid, "QQ group removed"),
            Ok(None) => {}
            Err(error) => warn!(%error, "remove QQ group failed"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum SessionEnd {
    ConfigChanged,
    RefreshToken,
}

async fn run_worker(
    service: Arc<QqService>,
    mut config_rx: watch::Receiver<QqConfig>,
    mut notify_rx: mpsc::Receiver<NotifyJob>,
) {
    let mut retry_seconds = 1u64;
    loop {
        let config = config_rx.borrow_and_update().clone();
        if !config.ready() {
            service.update_status(|status| {
                status.online = false;
                status.state = if config.enabled {
                    "incomplete".to_owned()
                } else {
                    "disabled".to_owned()
                };
                status.detail = if config.enabled {
                    "等待配置 AppID 和 AppSecret".to_owned()
                } else {
                    "QQ 机器人未启用".to_owned()
                };
                status.token_expires_epoch = 0;
                status.last_error.clear();
            });
            tokio::select! {
                changed = config_rx.changed() => {
                    if changed.is_err() {
                        return;
                    }
                }
                job = notify_rx.recv() => {
                    let Some(job) = job else { return };
                    let _ = job.response.send(Err("QQ 机器人未启用".to_owned()));
                }
            }
            continue;
        }

        let connected_before = service.status().connected_epoch;
        service.update_status(|status| {
            status.online = false;
            status.state = "connecting".to_owned();
            status.detail = "正在连接 QQ Gateway".to_owned();
            status.last_error.clear();
        });
        let delay_seconds;
        match run_session(&service, &config, &mut config_rx, &mut notify_rx).await {
            Ok(SessionEnd::ConfigChanged | SessionEnd::RefreshToken) => {
                retry_seconds = 1;
                continue;
            }
            Err(error) => {
                let message = format!("{error:#}");
                delay_seconds = retry_seconds;
                warn!(error = %message, "QQ Gateway disconnected");
                service.update_status(|status| {
                    status.online = false;
                    status.state = "retrying".to_owned();
                    status.detail = format!("{delay_seconds} 秒后重连");
                    status.last_error.clone_from(&message);
                    status.reconnect_count = status.reconnect_count.saturating_add(1);
                });
                if service.status().connected_epoch > connected_before {
                    retry_seconds = 1;
                } else {
                    retry_seconds = (retry_seconds * 2).min(60);
                }
            }
        }

        let retry = sleep(Duration::from_secs(delay_seconds));
        tokio::pin!(retry);
        loop {
            tokio::select! {
                _ = &mut retry => break,
                changed = config_rx.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    break;
                }
                job = notify_rx.recv() => {
                    let Some(job) = job else { return };
                    let _ = job.response.send(Err("QQ 机器人正在重连".to_owned()));
                }
            }
        }
    }
}

async fn run_session(
    service: &Arc<QqService>,
    config: &QqConfig,
    config_rx: &mut watch::Receiver<QqConfig>,
    notify_rx: &mut mpsc::Receiver<NotifyJob>,
) -> Result<SessionEnd> {
    let token = fetch_access_token(&service.http, config).await?;
    service.update_status(|status| {
        status.token_expires_epoch = epoch_seconds().saturating_add(token.expires_in);
        status.detail = "正在获取 QQ Gateway".to_owned();
    });
    let gateway = fetch_gateway(&service.http, &token.value).await?;
    let (mut socket, _) = timeout(Duration::from_secs(15), connect_async(&gateway))
        .await
        .context("connect QQ Gateway timeout")?
        .context("connect QQ Gateway")?;

    let hello = timeout(Duration::from_secs(15), socket.next())
        .await
        .context("wait QQ Gateway Hello timeout")?
        .context("QQ Gateway closed before Hello")??;
    let hello = message_value(hello).context("QQ Gateway Hello is not JSON")?;
    if hello.get("op").and_then(Value::as_u64) != Some(10) {
        bail!("unexpected QQ Gateway Hello");
    }
    let heartbeat_ms = hello
        .pointer("/d/heartbeat_interval")
        .and_then(Value::as_u64)
        .filter(|value| *value >= 1_000)
        .context("QQ Gateway heartbeat interval is missing")?;
    let authorization = format!("QQBot {}", token.value);
    socket
        .send(Message::Text(
            json!({
                "op": 2,
                "d": {
                    "token": authorization,
                    "intents": GROUP_AND_C2C_INTENT,
                    "shard": [0, 1],
                    "properties": {
                        "$os": "linux",
                        "$browser": "camera-hub",
                        "$device": "camera-hub"
                    }
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .context("send QQ Gateway Identify")?;

    let ready = timeout(Duration::from_secs(15), async {
        loop {
            let message = socket
                .next()
                .await
                .context("QQ Gateway closed before Ready")??;
            match message {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(text.as_str())?;
                    if value.get("op").and_then(Value::as_u64) == Some(9) {
                        bail!("QQ Gateway rejected Identify");
                    }
                    if value.get("op").and_then(Value::as_u64) == Some(0)
                        && value.get("t").and_then(Value::as_str) == Some("READY")
                    {
                        return Ok::<Value, anyhow::Error>(value);
                    }
                }
                Message::Ping(data) => socket.send(Message::Pong(data)).await?,
                Message::Close(frame) => {
                    bail!(
                        "QQ Gateway closed before Ready: {}",
                        close_reason(frame.as_ref())
                    );
                }
                _ => {}
            }
        }
    })
    .await
    .context("wait QQ Gateway Ready timeout")??;

    let bot_name = ready
        .pointer("/d/user/username")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let mut last_sequence = ready.get("s").and_then(Value::as_u64);
    service.update_status(|status| {
        status.online = true;
        status.state = "online".to_owned();
        status.detail = "QQ Gateway 已连接".to_owned();
        status.bot_name = bot_name;
        status.connected_epoch = epoch_seconds();
        status.last_error.clear();
    });
    info!(app_id = %config.app_id, "QQ Gateway ready");

    let heartbeat_duration = Duration::from_millis(heartbeat_ms);
    let mut heartbeat = interval_at(Instant::now() + heartbeat_duration, heartbeat_duration);
    let mut heartbeat_acknowledged = true;
    let refresh_after = token.expires_in.saturating_sub(60).max(60);
    let refresh = sleep(Duration::from_secs(refresh_after));
    tokio::pin!(refresh);

    loop {
        tokio::select! {
            changed = config_rx.changed() => {
                if changed.is_err() {
                    bail!("QQ configuration channel closed");
                }
                let _ = socket.close(None).await;
                return Ok(SessionEnd::ConfigChanged);
            }
            _ = &mut refresh => {
                let _ = socket.close(None).await;
                return Ok(SessionEnd::RefreshToken);
            }
            _ = heartbeat.tick() => {
                if !heartbeat_acknowledged {
                    bail!("QQ Gateway heartbeat ACK timeout");
                }
                socket.send(Message::Text(json!({
                    "op": 1,
                    "d": last_sequence,
                }).to_string().into())).await.context("send QQ heartbeat")?;
                heartbeat_acknowledged = false;
            }
            job = notify_rx.recv() => {
                let Some(job) = job else {
                    bail!("QQ notification queue closed");
                };
                let delivery_config = service.current_config();
                let result = deliver_notification(
                    &service.http,
                    &delivery_config,
                    &token.value,
                    &job.request,
                ).await;
                match result {
                    Ok(receipt) => {
                        service.update_status(|status| {
                            status.sent_count = status.sent_count
                                .saturating_add(receipt.messages as u64);
                            status.last_send_epoch = epoch_seconds();
                            status.last_error.clear();
                        });
                        let _ = job.response.send(Ok(receipt));
                    }
                    Err(error) => {
                        let message = format!("{error:#}");
                        service.update_status(|status| {
                            status.failed_count = status.failed_count.saturating_add(1);
                            status.last_error.clone_from(&message);
                        });
                        let _ = job.response.send(Err(message));
                    }
                }
            }
            message = socket.next() => {
                let message = message.context("QQ Gateway closed")??;
                match message {
                    Message::Text(text) => {
                        let value: Value = serde_json::from_str(text.as_str())
                            .context("parse QQ Gateway payload")?;
                        if let Some(sequence) = value.get("s").and_then(Value::as_u64) {
                            last_sequence = Some(sequence);
                        }
                        match value.get("op").and_then(Value::as_u64) {
                            Some(0) => handle_dispatch(service, &value),
                            Some(7) => bail!("QQ Gateway requested reconnect"),
                            Some(9) => bail!("QQ Gateway session became invalid"),
                            Some(11) => heartbeat_acknowledged = true,
                            _ => {}
                        }
                    }
                    Message::Ping(data) => socket.send(Message::Pong(data)).await?,
                    Message::Close(frame) => {
                        bail!("QQ Gateway closed: {}", close_reason(frame.as_ref()));
                    }
                    _ => {}
                }
            }
        }
    }
}

fn handle_dispatch(service: &QqService, payload: &Value) {
    let event = payload.get("t").and_then(Value::as_str).unwrap_or_default();
    let group_openid = payload
        .pointer("/d/group_openid")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !group_openid.is_empty() {
        if event == "GROUP_DEL_ROBOT" {
            service.remove_group(group_openid);
        } else if matches!(
            event,
            "GROUP_ADD_ROBOT" | "GROUP_AT_MESSAGE_CREATE" | "GROUP_MESSAGE_CREATE"
        ) {
            service.discover_group(group_openid);
        }
    }
    service.update_status(|status| {
        status.last_event_epoch = epoch_seconds();
    });
}

struct AccessToken {
    value: String,
    expires_in: u64,
}

async fn fetch_access_token(client: &reqwest::Client, config: &QqConfig) -> Result<AccessToken> {
    let body = serde_json::to_vec(&json!({
        "appId": config.app_id,
        "clientSecret": config.app_secret,
    }))?;
    let value = request_json(
        client
            .post(format!("{QQ_API_BASE}/app/getAppAccessToken"))
            .header(CONTENT_TYPE, "application/json")
            .body(body),
    )
    .await
    .context("get QQ access token")?;
    let token = value
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .context("QQ access token is missing")?;
    let expires_in = value
        .get("expires_in")
        .and_then(value_u64)
        .unwrap_or(7200)
        .clamp(120, 7200);
    Ok(AccessToken {
        value: token.to_owned(),
        expires_in,
    })
}

async fn fetch_gateway(client: &reqwest::Client, token: &str) -> Result<String> {
    let value = request_json(
        client
            .get(format!("{QQ_API_BASE}/gateway"))
            .header(AUTHORIZATION, format!("QQBot {token}")),
    )
    .await
    .context("get QQ Gateway")?;
    value
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| url.starts_with("wss://"))
        .map(str::to_owned)
        .context("QQ Gateway URL is missing")
}

async fn deliver_notification(
    client: &reqwest::Client,
    config: &QqConfig,
    token: &str,
    request: &QqNotifyRequest,
) -> Result<QqNotifyReceipt> {
    let group = config.resolve_group(&request.target)?;
    let chunks = split_message(&request.content);
    for chunk in &chunks {
        let body = serde_json::to_vec(&json!({
            "msg_type": 0,
            "content": chunk,
        }))?;
        request_json(
            client
                .post(format!("{QQ_API_BASE}/v2/groups/{}/messages", group.openid))
                .header(AUTHORIZATION, format!("QQBot {token}"))
                .header(CONTENT_TYPE, "application/json; charset=utf-8")
                .body(body),
        )
        .await
        .with_context(|| format!("send QQ group message to {}", group.name))?;
    }
    Ok(QqNotifyReceipt {
        target: group.name.clone(),
        messages: chunks.len(),
    })
}

async fn request_json(request: reqwest::RequestBuilder) -> Result<Value> {
    let response = request.send().await?;
    let status = response.status();
    let trace_id = response
        .headers()
        .get("x-tps-trace-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let bytes = response.bytes().await?;
    let value = serde_json::from_slice::<Value>(&bytes)
        .unwrap_or_else(|_| json!({"message": String::from_utf8_lossy(&bytes).trim()}));
    let error_code = value
        .get("err_code")
        .or_else(|| value.get("code"))
        .and_then(value_i64)
        .unwrap_or(0);
    if !status.is_success() || error_code != 0 {
        let message = value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("QQ API request failed");
        bail!(
            "QQ API HTTP {} err_code={} message={} trace_id={}",
            status.as_u16(),
            error_code,
            message,
            trace_id
        );
    }
    Ok(value)
}

fn message_value(message: Message) -> Option<Value> {
    match message {
        Message::Text(text) => serde_json::from_str(text.as_str()).ok(),
        _ => None,
    }
}

fn close_reason(frame: Option<&tokio_tungstenite::tungstenite::protocol::CloseFrame>) -> String {
    frame
        .map(|frame| format!("{} {}", frame.code, frame.reason))
        .unwrap_or_else(|| "without close frame".to_owned())
}

fn split_message(content: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for character in content.chars() {
        if !current.is_empty() && current.len() + character.len_utf8() > MESSAGE_CHUNK_BYTES {
            chunks.push(std::mem::take(&mut current));
        }
        current.push(character);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn default_group_name(openid: &str) -> String {
    let suffix = openid
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("群 {suffix}")
}

fn clean_group_name(name: &str, openid: &str) -> String {
    let name = name
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    if name.is_empty() {
        return default_group_name(openid);
    }
    name.chars().take(32).collect()
}

fn valid_group_openid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn token_digest(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn value_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str()?.parse::<u64>().ok())
}

fn value_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str()?.parse::<i64>().ok())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let length = left.len().max(right.len());
    for index in 0..length {
        let lhs = left.get(index).copied().unwrap_or_default();
        let rhs = right.get(index).copied().unwrap_or_default();
        difference |= usize::from(lhs ^ rhs);
    }
    difference == 0
}

fn secure_file(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_config(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "camera-hub-qq-{label}-{}-{nonce}.json",
            std::process::id()
        ))
    }

    #[test]
    fn keeps_secret_write_only_and_persists_aliases() {
        let path = temporary_config("config");
        let mut store = QqConfigStore::load(path.clone()).unwrap();
        store
            .discover_group("GROUP123456")
            .unwrap()
            .expect("new group");
        let updated = store
            .update(QqConfigUpdate {
                enabled: true,
                app_id: "123456789".to_owned(),
                app_secret: "secret-value".to_owned(),
                default_group: "GROUP123456".to_owned(),
                group_aliases: BTreeMap::from([("GROUP123456".to_owned(), "开发通知".to_owned())]),
                ..QqConfigUpdate::default()
            })
            .unwrap();
        assert_eq!(updated.app_secret, "secret-value");
        assert!(updated.public().secret_configured);
        assert_eq!(updated.groups[0].name, "开发通知");

        let retained = store
            .update(QqConfigUpdate {
                enabled: true,
                app_id: "123456789".to_owned(),
                app_secret: String::new(),
                default_group: "GROUP123456".to_owned(),
                group_aliases: BTreeMap::new(),
                ..QqConfigUpdate::default()
            })
            .unwrap();
        assert_eq!(retained.app_secret, "secret-value");

        let changed_app = store
            .update(QqConfigUpdate {
                enabled: false,
                app_id: "987654321".to_owned(),
                app_secret: String::new(),
                default_group: "GROUP123456".to_owned(),
                group_aliases: BTreeMap::new(),
                ..QqConfigUpdate::default()
            })
            .unwrap();
        assert!(changed_app.app_secret.is_empty());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn resolves_only_discovered_groups() {
        let mut config = QqConfig::default();
        config.groups.push(QqGroup {
            openid: "GROUP123456".to_owned(),
            name: "默认群".to_owned(),
            added_epoch: 1,
        });
        config.default_group = "GROUP123456".to_owned();
        assert_eq!(config.resolve_group("default").unwrap().name, "默认群");
        assert_eq!(
            config.resolve_group("默认群").unwrap().openid,
            "GROUP123456"
        );
        assert!(config.resolve_group("UNKNOWN").is_err());
    }

    #[test]
    fn chunks_unicode_messages_without_breaking_characters() {
        let content = "测".repeat(MESSAGE_CHUNK_BYTES);
        let chunks = split_message(&content);
        assert!(chunks.len() > 1);
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.len() <= MESSAGE_CHUNK_BYTES)
        );
        assert_eq!(chunks.concat(), content);
    }

    #[test]
    fn push_token_digest_uses_constant_time_comparison() {
        let digest = token_digest("chq_test");
        assert!(constant_time_eq(
            digest.as_bytes(),
            token_digest("chq_test").as_bytes()
        ));
        assert!(!constant_time_eq(
            digest.as_bytes(),
            token_digest("chq_other").as_bytes()
        ));
    }
}
