//! One TV watched by the persistent hub, using adbd directly (no adb executable).
use crate::{atomic_file, config::Config};
use anyhow::{Context, Result, bail, ensure};
use libadb::{Connection, ConnectionConfig, keys, shell, sync};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, Notify, RwLock};
use tokio::time::{Instant, sleep, timeout};
use tracing::{info, warn};

pub const MAX_APK_BYTES: usize = 16 * 1024 * 1024;
const APK: &str = "/data/local/tmp/phone-control-shell.apk";
const ENTRY: &str = "site.gwghome.aicontrol.shell.ShellAgent";
const DATA_CHECK: &str = r#"
while read device mountpoint filesystem options rest; do
    if [ "$mountpoint" = /data ]; then
        case ",$options," in
            *,ro,*) echo CAMERA_HUB_DATA_READ_ONLY; exit 1 ;;
            *,rw,*) echo CAMERA_HUB_DATA_RW; exit 0 ;;
        esac
    fi
done < /proc/mounts
echo CAMERA_HUB_DATA_UNKNOWN
exit 1
"#;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct TvConfig {
    pub enabled: bool,
    pub host: String,
    pub adb_port: u16,
    /// Optional externally reachable origin; probes always use the LAN host.
    pub api_origin: String,
    pub poll_seconds: u64,
    pub jpeg_quality: u8,
    pub frame_interval_ms: u64,
}

impl Default for TvConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: String::new(),
            adb_port: 5555,
            api_origin: String::new(),
            poll_seconds: 15,
            jpeg_quality: 25,
            frame_interval_ms: 300,
        }
    }
}

impl TvConfig {
    fn validate(&mut self) -> Result<()> {
        self.host = self.host.trim().trim_matches(['[', ']']).to_owned();
        ensure!(
            self.host.is_empty()
                || self.host.parse::<IpAddr>().is_ok()
                || (self.host.len() <= 253
                    && self
                        .host
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')),
            "电视地址应为 IP 或主机名，不包含端口或路径"
        );
        ensure!(
            !self.enabled || !self.host.is_empty(),
            "启用守护前请填写电视地址"
        );
        ensure!(self.adb_port > 0, "ADB 端口不能为 0");
        ensure!(
            (5..=300).contains(&self.poll_seconds),
            "检测间隔应为 5–300 秒"
        );
        ensure!(
            (20..=90).contains(&self.jpeg_quality),
            "JPEG 质量应为 20–90"
        );
        ensure!(
            [200, 300, 500, 800, 1000, 1500, 2000].contains(&self.frame_interval_ms),
            "不支持的截图间隔"
        );
        self.api_origin = self.api_origin.trim().trim_end_matches('/').to_owned();
        if !self.api_origin.is_empty() {
            let url = reqwest::Url::parse(&self.api_origin).context("AI 访问地址无效")?;
            ensure!(
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.path() == "/"
                    && url.query().is_none()
                    && url.fragment().is_none(),
                "AI 访问地址只填写 http(s)://主机:端口"
            );
        }
        Ok(())
    }

    fn lan_origin(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        format!("http://{host}:9080")
    }
}

#[derive(Clone, Default, Serialize)]
pub struct TvStatus {
    pub phase: String,
    pub message: String,
    pub service_online: bool,
    pub adb_connected: bool,
    pub last_checked_epoch: Option<u64>,
    pub last_recovered_epoch: Option<u64>,
    pub recovery_count: u64,
    pub consecutive_failures: u32,
    pub next_retry_epoch: Option<u64>,
    pub last_error: Option<String>,
    #[serde(skip)]
    retry_at: Option<Instant>,
    #[serde(skip)]
    recovery_failures: u32,
    #[serde(skip)]
    last_frame_key: Option<String>,
    #[serde(skip)]
    last_frame_progress: Option<Instant>,
}

struct TvProbe {
    capture_ready: bool,
    capture_stalled: bool,
    capture_error: Option<String>,
}

#[derive(Serialize)]
pub struct TvOverview {
    pub config: TvConfig,
    pub status: TvStatus,
    pub apk_bytes: u64,
    pub control_url: String,
    pub guide_url: String,
}

pub struct TvControlService {
    config_file: PathBuf,
    data_dir: PathBuf,
    config: RwLock<TvConfig>,
    status: RwLock<TvStatus>,
    client: reqwest::Client,
    /// Serializes checks, bootstrap, config changes, and APK replacement.
    operation: Mutex<()>,
    wake: Notify,
    manual: AtomicBool,
}

impl TvControlService {
    pub fn start(config: &Config) -> Result<Arc<Self>> {
        let config_file = config.tv_control_file.clone();
        let mut saved: TvConfig = match fs::read(&config_file) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("读取电视守护配置")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => TvConfig::default(),
            Err(error) => return Err(error.into()),
        };
        saved.validate()?;
        let service = Arc::new(Self {
            data_dir: config_file.with_extension("state"),
            config_file,
            config: RwLock::new(saved),
            status: RwLock::new(TvStatus {
                phase: "disabled".into(),
                message: "自动守护未启用".into(),
                ..Default::default()
            }),
            client: camera_hub::http_client::builder()?
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(3))
                .build()?,
            operation: Mutex::new(()),
            wake: Notify::new(),
            manual: AtomicBool::new(false),
        });
        let worker = service.clone();
        tokio::spawn(async move {
            loop {
                let manual = worker.manual.swap(false, Ordering::AcqRel);
                worker.reconcile(manual).await;
                let seconds = worker.config.read().await.poll_seconds;
                tokio::select! {
                    _ = sleep(Duration::from_secs(seconds)) => {}
                    _ = worker.wake.notified() => {}
                }
            }
        });
        Ok(service)
    }

    pub async fn overview(&self) -> TvOverview {
        let config = self.config.read().await.clone();
        let origin = if config.host.is_empty() {
            String::new()
        } else if config.api_origin.is_empty() {
            config.lan_origin()
        } else {
            config.api_origin.clone()
        };
        TvOverview {
            config,
            status: self.status.read().await.clone(),
            apk_bytes: fs::metadata(self.data_dir.join("agent.apk"))
                .map(|m| m.len())
                .unwrap_or(0),
            guide_url: if origin.is_empty() {
                String::new()
            } else {
                format!("{origin}/api/v1/guide")
            },
            control_url: origin,
        }
    }

    pub async fn update(&self, mut config: TvConfig) -> Result<TvOverview> {
        config.validate()?;
        let _operation = self.operation.lock().await;
        atomic_file::write_json(&self.config_file, &config)?;
        *self.config.write().await = config;
        let mut status = self.status.write().await;
        let recovered = status.last_recovered_epoch;
        let count = status.recovery_count;
        *status = TvStatus {
            phase: "pending".into(),
            message: "配置已保存，等待检测".into(),
            last_recovered_epoch: recovered,
            recovery_count: count,
            ..Default::default()
        };
        drop(status);
        self.wake.notify_one();
        Ok(self.overview().await)
    }

    pub async fn upload_apk(&self, data: &[u8]) -> Result<TvOverview> {
        ensure!(
            (4..=MAX_APK_BYTES).contains(&data.len()) && data.starts_with(b"PK\x03\x04"),
            "请上传有效 APK，最大 16 MiB"
        );
        let _operation = self.operation.lock().await;
        fs::create_dir_all(&self.data_dir)?;
        let temporary = self.data_dir.join("agent.apk.upload");
        {
            use std::io::Write;
            let mut options = fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            file.write_all(data)?;
            file.sync_all()?;
        }
        fs::rename(temporary, self.data_dir.join("agent.apk"))?;
        Ok(self.overview().await)
    }

    /// A manual request also works while automatic monitoring is disabled.
    pub async fn request_check(&self) -> Result<()> {
        ensure!(
            !self.config.read().await.host.is_empty(),
            "请先保存电视地址"
        );
        self.manual.store(true, Ordering::Release);
        self.wake.notify_one();
        Ok(())
    }

    async fn probe(&self, config: &TvConfig) -> Result<TvProbe> {
        let response = self
            .client
            .get(format!("{}/api/v1/status", config.lan_origin()))
            .send()
            .await?
            .error_for_status()?;
        if response.content_length().is_some_and(|n| n > 64 * 1024) {
            bail!("电视状态响应过大");
        }
        let mut response = response;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(bytes.len() + chunk.len() <= 64 * 1024, "电视状态响应过大");
            bytes.extend_from_slice(&chunk);
        }
        let body: Value = serde_json::from_slice(&bytes)?;
        ensure!(
            body["service"] == "phone-control"
                && body["controlBackend"] == "adb_shell"
                && body["controlConnected"] == true,
            "电视控制尚未就绪"
        );
        let capture = &body["capture"];
        let frame = &capture["latestFrame"];
        let active = capture["projectionActive"] == true && frame.is_object();
        let interval = capture["config"]["frameIntervalMs"].as_u64().unwrap_or(config.frame_interval_ms);
        let stale_after = Duration::from_millis(interval.saturating_mul(5).max(10_000));
        let mut status = self.status.write().await;
        let stalled = if active {
            let key = format!("{}:{}:{}", body["instanceId"], capture["screenEpoch"], frame["frameId"]);
            if status.last_frame_key.as_ref() != Some(&key) {
                status.last_frame_key = Some(key);
                status.last_frame_progress = Some(Instant::now());
            }
            if let Some(age) = capture["frameAgeMs"].as_u64() {
                Duration::from_millis(age) > stale_after
            } else {
                status.last_frame_progress.is_some_and(|at| at.elapsed() > stale_after)
            }
        } else {
            status.last_frame_key = None;
            status.last_frame_progress = None;
            false
        };
        Ok(TvProbe {
            capture_ready: active && !stalled,
            capture_stalled: stalled,
            capture_error: capture["lastError"].as_str().map(str::to_owned),
        })
    }

    async fn healthy(&self, config: &TvConfig) -> Result<()> {
        ensure!(self.probe(config).await?.capture_ready, "电视截图尚未就绪或已停止更新");
        Ok(())
    }

    async fn reconcile(&self, manual: bool) {
        let _operation = self.operation.lock().await;
        let config = self.config.read().await.clone();
        if !manual && !config.enabled {
            let mut status = self.status.write().await;
            status.phase = "disabled".into();
            status.message = "自动守护未启用；可手动检查并恢复".into();
            return;
        }
        let probe = self.probe(&config).await;
        {
            let mut status = self.status.write().await;
            status.last_checked_epoch = Some(epoch());
            status.service_online = probe.is_ok();
            if probe.as_ref().is_ok_and(|p| p.capture_ready) {
                status.phase = "online".into();
                status.message = "电视控制与截图服务正常".into();
                status.consecutive_failures = 0;
                status.recovery_failures = 0;
                status.retry_at = None;
                status.next_retry_epoch = None;
                status.last_error = None;
                return;
            }
            if let Ok(ref capture) = probe {
                if !capture.capture_stalled && !manual {
                    status.phase = "capture_unavailable".into();
                    status.message = "控制服务在线，截图暂不可用；可继续使用方向键和页面控件".into();
                    status.last_error = capture.capture_error.clone();
                    status.consecutive_failures = 0;
                    status.recovery_failures = 0;
                    status.retry_at = None;
                    status.next_retry_epoch = None;
                    return;
                }
            }
            status.consecutive_failures = status.consecutive_failures.saturating_add(1);
            if status.retry_at.is_none() {
                status.last_error = match &probe {
                    Ok(p) if p.capture_stalled => Some("电视截图帧已停止更新".into()),
                    Ok(p) => p.capture_error.clone(),
                    Err(e) => Some(format!("{e:#}")),
                };
            }
            if !manual && status.consecutive_failures < 2 {
                if probe.as_ref().is_ok_and(|p| p.capture_stalled) {
                    status.phase = "capture_stalled".into();
                    status.message = "控制服务在线，但截图停止更新，等待再次确认".into();
                } else {
                    status.phase = "checking".into();
                    status.message = "首次健康检查失败，等待再次确认".into();
                }
                return;
            }
            if !manual && status.retry_at.is_some_and(|next| next > Instant::now()) {
                if status.phase != "filesystem_readonly" {
                    status.phase = "waiting".into();
                    status.message = "服务不可用，等待下次 ADB 恢复".into();
                }
                return;
            }
            status.phase = "recovering".into();
            status.message = "正在连接电视 ADB 并恢复服务".into();
            status.next_retry_epoch = None;
            status.adb_connected = false;
        }
        let result = timeout(Duration::from_secs(60), self.restore(&config))
            .await
            .unwrap_or_else(|_| {
                Err(anyhow::anyhow!(
                    "电视恢复超时；请检查网络或电视上的 ADB 授权"
                ))
            });
        let mut status = self.status.write().await;
        status.last_checked_epoch = Some(epoch());
        match result {
            Ok(()) => {
                status.phase = "online".into();
                status.message = "服务已恢复，控制与截图健康检查通过".into();
                status.service_online = true;
                status.last_error = None;
                status.last_recovered_epoch = Some(epoch());
                status.recovery_count += 1;
                status.consecutive_failures = 0;
                status.recovery_failures = 0;
                status.retry_at = None;
                status.next_retry_epoch = None;
                info!(host = config.host, "TV shell agent restored");
            }
            Err(error) => {
                let detail = format!("{error:#}");
                let read_only = detail.contains("CAMERA_HUB_DATA_READ_ONLY")
                    || detail.contains("Read-only file system");
                status.phase = if read_only { "filesystem_readonly" } else { "waiting" }.into();
                status.message = if read_only {
                    "电视 /data 文件系统只读，已停止部署；请重启电视，恢复可写后会自动重试"
                } else {
                    "暂时无法恢复，请查看具体错误；网络或 ADB 授权异常时请检查电视"
                }.into();
                status.last_error = Some(detail);
                status.recovery_failures = if read_only {
                    0
                } else {
                    status.recovery_failures.saturating_add(1)
                };
                // Keep detecting an offline TV each polling interval. Back off
                // only expensive/authenticated bootstrap failures.
                let delay = if status.adb_connected && !read_only {
                    15_u64.saturating_mul(1 << status.recovery_failures.min(4))
                } else {
                    config.poll_seconds
                };
                status.retry_at = Some(Instant::now() + Duration::from_secs(delay));
                status.next_retry_epoch = Some(epoch() + delay);
                warn!(host = config.host, %error, "TV shell recovery failed");
            }
        }
    }

    async fn restore(&self, config: &TvConfig) -> Result<()> {
        let key_dir = self.data_dir.join("adb");
        let key = tokio::task::spawn_blocking(move || {
            let mut rng = rand::rngs::OsRng;
            let key = match keys::store::load(&key_dir, &mut rng, "camera-hub@tv") {
                Ok(key) => key,
                Err(keys::store::StoreError::Io { source, .. })
                    if source.kind() == std::io::ErrorKind::NotFound =>
                {
                    // The library's host store uses hard_link to publish a key.
                    // Android denies link in app_data_file; our single worker
                    // already serializes key creation, so atomic rename suffices.
                    let key = keys::AdbKey::generate(&mut rng, "camera-hub@tv")?;
                    atomic_file::write_bytes(
                        &key_dir.join("adbkey"),
                        key.to_pkcs8_pem()?.as_bytes(),
                    )?;
                    key
                }
                Err(error) => return Err(error.into()),
            };
            atomic_file::write_bytes(&key_dir.join("adbkey.pub"), key.public_key_line())?;
            Ok::<_, anyhow::Error>(key)
        })
        .await??;
        let stream = timeout(
            Duration::from_secs(3),
            TcpStream::connect((config.host.as_str(), config.adb_port)),
        )
        .await
        .context("电视 ADB 端口连接超时")?
        .context("电视 ADB 离线或端口未开放")?;
        self.status.write().await.adb_connected = true;
        stream.set_nodelay(true)?;
        let mut connection = timeout(
            Duration::from_secs(12),
            Connection::connect_with_raw_banner_and_config(
                AdbTcp::new(stream),
                key,
                b"host::features=\0",
                ConnectionConfig::embedded(),
            ),
        )
        .await
        .context("ADB 握手超时，请在电视上允许 camera-hub 的调试授权")?
        .map_err(|e| anyhow::anyhow!("ADB 握手失败：{e}"))?;

        // Recheck after the handshake: a slow/booting service may have recovered itself.
        if self.healthy(config).await.is_ok() {
            return Ok(());
        }
        check_data_writable(&mut connection).await?;
        let replace_apk = match tokio::fs::read(self.data_dir.join("agent.apk")).await {
            Ok(bytes) => {
                ensure!(
                    (4..=MAX_APK_BYTES).contains(&bytes.len()) && bytes.starts_with(b"PK\x03\x04"),
                    "缓存 APK 无效，请重新上传"
                );
                let digest = hex::encode(Sha256::digest(&bytes));
                if apk_matches(&mut connection, APK, bytes.len(), &digest).await? {
                    info!(host = config.host, "Reusing unchanged TV APK");
                    false
                } else {
                    let mut buffer = vec![0_u8; 66_000];
                    let mut session = sync::open(&mut connection, &mut buffer)
                        .await
                        .map_err(|e| anyhow::anyhow!("ADB sync：{e}"))?;
                    session
                        .send(&format!("{APK}.upload"), 0o100644, epoch() as u32, &bytes)
                        .await
                        .map_err(|e| anyhow::anyhow!("传输电视 APK：{e}"))?;
                    session
                        .quit()
                        .await
                        .map_err(|e| anyhow::anyhow!("结束 APK 传输：{e}"))?;
                    ensure!(
                        apk_matches(&mut connection, &format!("{APK}.upload"), bytes.len(), &digest).await?,
                        "电视临时 APK 校验失败，旧 APK 与进程已保留"
                    );
                    true
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        // Check again after transfer, before stopping the old process or writing
        // PID/log files. Never replace an APK still in use by that process.
        check_data_writable(&mut connection).await?;
        let replacement = if replace_apk {
            format!("mv {APK}.upload {APK} || exit 1")
        } else {
            String::new()
        };
        let script = format!(
            r#"
if [ ! -f {candidate} ]; then echo CAMERA_HUB_APK_MISSING; exit 1; fi
pidfile=/data/local/tmp/phone-control-shell.pid
if [ -f "$pidfile" ]; then
    pid=$(cat "$pidfile")
    case "$pid" in ''|*[!0-9]*) echo CAMERA_HUB_INVALID_PID; exit 1 ;; esac
    if [ -r /proc/$pid/cmdline ]; then
        name=$(tr '\000' ' ' < /proc/$pid/cmdline)
        case "$name" in
            *{ENTRY}*)
                kill "$pid" || exit 1
                attempts=0
                while kill -0 "$pid" 2>/dev/null; do
                    name=$(tr '\000' ' ' < /proc/$pid/cmdline 2>/dev/null)
                    case "$name" in *{ENTRY}*) ;; *) break ;; esac
                    if [ "$attempts" -ge 5 ]; then echo CAMERA_HUB_STOP_TIMEOUT; exit 1; fi
                    sleep 1
                    attempts=$((attempts + 1))
                done ;;
            *) echo CAMERA_HUB_STALE_PID ;;
        esac
    fi
    rm -f "$pidfile" || exit 1
fi
{replacement}
am force-stop site.gwghome.aicontrol >/dev/null 2>&1 || true
trap '' HUP
CLASSPATH={APK} app_process /system/bin {ENTRY} {quality} {interval} > /data/local/tmp/phone-control-shell.log 2>&1 < /dev/null &
echo $! > "$pidfile" || exit 1
echo CAMERA_HUB_STARTED
"#,
            candidate = if replace_apk { format!("{APK}.upload") } else { APK.into() },
            quality = config.jpeg_quality,
            interval = config.frame_interval_ms
        );
        let output = remote_shell(&mut connection, &script).await?;
        ensure!(
            output.contains("CAMERA_HUB_STARTED"),
            "电视启动失败：{output}"
        );
        for _ in 0..12 {
            sleep(Duration::from_secs(1)).await;
            if self.healthy(config).await.is_ok() {
                return Ok(());
            }
        }
        let log = remote_shell(
            &mut connection,
            "tail -n 20 /data/local/tmp/phone-control-shell.log",
        )
        .await
        .unwrap_or_default();
        bail!("已发送启动命令，但控制或截图未就绪：{log}")
    }
}

async fn check_data_writable(connection: &mut Connection<AdbTcp>) -> Result<()> {
    let output = remote_shell(connection, DATA_CHECK).await?;
    ensure!(
        output == "CAMERA_HUB_DATA_RW",
        "电视 /data 不可写，已停止部署：{output}"
    );
    Ok(())
}

async fn apk_matches(
    connection: &mut Connection<AdbTcp>,
    path: &str,
    expected_size: usize,
    digest: &str,
) -> Result<bool> {
    let output = remote_shell(connection, &format!(
        "if [ ! -f {path} ]; then echo CAMERA_HUB_APK_MISSING; \
         elif command -v sha256sum >/dev/null 2>&1; then sha256sum {path}; \
         else echo CAMERA_HUB_HASH_UNAVAILABLE; fi"
    )).await?;
    if output == "CAMERA_HUB_APK_MISSING" {
        return Ok(false);
    }
    if output != "CAMERA_HUB_HASH_UNAVAILABLE" {
        let hash = output.split_whitespace().next().unwrap_or_default();
        ensure!(
            hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit()),
            "读取电视 APK 校验值失败，旧进程已保留：{output}"
        );
        return Ok(hash.eq_ignore_ascii_case(digest));
    }
    // Older Android builds may lack sha256sum. Pull bounded chunks over sync
    // and hash locally; do not degrade verification to file size alone.
    let mut buffer = vec![0_u8; 66_000];
    let mut session = sync::open(connection, &mut buffer)
        .await
        .map_err(|e| anyhow::anyhow!("ADB sync 校验：{e}"))?;
    let stat = session.stat(path).await.map_err(|e| anyhow::anyhow!("读取 APK 大小：{e}"))?;
    let mut matches = false;
    if stat.mode != 0 && stat.size as usize == expected_size {
        session.recv_start(path).await.map_err(|e| anyhow::anyhow!("读取 APK：{e}"))?;
        let mut hash = Sha256::new();
        let mut size = 0;
        while let Some(chunk) = session.recv_next().await.map_err(|e| anyhow::anyhow!("校验 APK：{e}"))? {
            size += chunk.len();
            ensure!(size <= MAX_APK_BYTES, "电视 APK 超过 16 MiB");
            hash.update(chunk);
        }
        matches = size == expected_size && hex::encode(hash.finalize()) == digest;
    }
    session.quit().await.map_err(|e| anyhow::anyhow!("结束 APK 校验：{e}"))?;
    Ok(matches)
}

async fn remote_shell(connection: &mut Connection<AdbTcp>, command: &str) -> Result<String> {
    let mut buffer = vec![0_u8; 64 * 1024];
    let size = shell::v1::exec(connection, command, &mut buffer)
        .await
        .map_err(|e| anyhow::anyhow!("电视 shell：{e}"))?;
    Ok(String::from_utf8_lossy(&buffer[..size]).trim().to_owned())
}

/// Android 5 adbd ends its CNXN banner with `;\0`. libadb 0.7 treats
/// the terminator as a malformed property. Normalize that packet only;
/// keep shell/sync bytes unchanged and let libadb handle the protocol.
struct AdbTcp {
    stream: TcpStream,
    packet: Vec<u8>,
    offset: usize,
}

impl AdbTcp {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            packet: Vec::new(),
            offset: 0,
        }
    }
}

impl embedded_io_async::ErrorType for AdbTcp {
    type Error = std::io::Error;
}

impl libadb::transport::ReadCancelSafety for AdbTcp {
    fn read_cancel_safe(&self) -> bool {
        // A cancelled read_exact may have consumed part of a packet.
        // The outer recovery timeout drops this whole connection.
        false
    }
}

impl embedded_io_async::Read for AdbTcp {
    async fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.offset == self.packet.len() {
            let mut header = [0_u8; 24];
            self.stream.read_exact(&mut header).await?;
            let size = u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;
            if size > 8192 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "ADB packet exceeds 8 KiB",
                ));
            }
            let mut payload = vec![0_u8; size];
            self.stream.read_exact(&mut payload).await?;
            if &header[..4] == b"CNXN" && payload.last() == Some(&0) {
                let checksum = u32::from_le_bytes(header[16..20].try_into().unwrap());
                let version = u32::from_le_bytes(header[4..8].try_into().unwrap());
                let sum: u32 = payload.iter().map(|b| *b as u32).sum();
                if version <= 0x01000000 && checksum != sum {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Invalid ADB banner checksum",
                    ));
                }
                while payload.last() == Some(&0) {
                    payload.pop();
                }
                header[12..16].copy_from_slice(&(payload.len() as u32).to_le_bytes());
                header[16..20].copy_from_slice(&sum.to_le_bytes());
            }
            self.packet.clear();
            self.packet.extend_from_slice(&header);
            self.packet.extend_from_slice(&payload);
            self.offset = 0;
        }
        let size = output.len().min(self.packet.len() - self.offset);
        output[..size].copy_from_slice(&self.packet[self.offset..self.offset + size]);
        self.offset += size;
        Ok(size)
    }
}

impl embedded_io_async::Write for AdbTcp {
    async fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.stream.write(data).await
    }
    async fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush().await
    }
}

fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
