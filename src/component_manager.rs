use crate::config::Config;
use crate::voice_config::VoiceWorkerStatus;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

const COMPONENT_CONFIG_VERSION: u32 = 1;
const STATUS_STALE_SECONDS: u64 = 15;
const RESTART_DELAY_SECONDS: u64 = 2;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ComponentId {
    Tts,
    Voice,
    Ddns,
    Ir,
}

impl ComponentId {
    const ALL: [Self; 4] = [Self::Tts, Self::Voice, Self::Ddns, Self::Ir];

    fn parse(value: &str) -> Result<Self> {
        match value {
            "tts" => Ok(Self::Tts),
            "voice" => Ok(Self::Voice),
            "ddns" => Ok(Self::Ddns),
            "ir" => Ok(Self::Ir),
            _ => bail!("不支持的组件：{value}"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Tts => "tts",
            Self::Voice => "voice",
            Self::Ddns => "ddns",
            Self::Ir => "ir",
        }
    }

    fn log_name(self) -> &'static str {
        match self {
            Self::Tts => "camera-hub-tts.log",
            Self::Voice => "camera-hub-voice.log",
            Self::Ddns => "camera-hub-ddns.log",
            Self::Ir => "camera-hub-ir.log",
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
struct ComponentSettings {
    version: u32,
    tts_autostart: bool,
    voice_autostart: bool,
    ddns_autostart: bool,
    ir_autostart: bool,
}

impl Default for ComponentSettings {
    fn default() -> Self {
        Self {
            version: COMPONENT_CONFIG_VERSION,
            tts_autostart: true,
            voice_autostart: true,
            ddns_autostart: true,
            ir_autostart: true,
        }
    }
}

impl ComponentSettings {
    fn autostart(&self, id: ComponentId) -> bool {
        match id {
            ComponentId::Tts => self.tts_autostart,
            ComponentId::Voice => self.voice_autostart,
            ComponentId::Ddns => self.ddns_autostart,
            ComponentId::Ir => self.ir_autostart,
        }
    }

    fn set_autostart(&mut self, id: ComponentId, enabled: bool) {
        match id {
            ComponentId::Tts => self.tts_autostart = enabled,
            ComponentId::Voice => self.voice_autostart = enabled,
            ComponentId::Ddns => self.ddns_autostart = enabled,
            ComponentId::Ir => self.ir_autostart = enabled,
        }
    }
}

struct ManagedComponent {
    desired_running: bool,
    child: Option<Child>,
    started_epoch: u64,
    restart_count: u64,
    last_exit: String,
    last_error: String,
    next_start_epoch: u64,
}

impl ManagedComponent {
    fn new(desired_running: bool) -> Self {
        Self {
            desired_running,
            child: None,
            started_epoch: 0,
            restart_count: 0,
            last_exit: String::new(),
            last_error: String::new(),
            next_start_epoch: 0,
        }
    }
}

struct ManagerState {
    settings: ComponentSettings,
    components: BTreeMap<ComponentId, ManagedComponent>,
}

pub struct ComponentManager {
    enabled: bool,
    executable: PathBuf,
    settings_path: PathBuf,
    log_dir: PathBuf,
    voice_lib_dir: PathBuf,
    config: Config,
    state: Mutex<ManagerState>,
}

#[derive(Debug, Serialize)]
pub struct ComponentsOverview {
    pub control_available: bool,
    pub tts: ComponentStatus,
    pub voice: ComponentStatus,
    pub ddns: ComponentStatus,
    pub ir: ComponentStatus,
}

#[derive(Debug, Serialize)]
pub struct ComponentStatus {
    pub id: &'static str,
    pub installed: bool,
    pub autostart: bool,
    pub desired_running: bool,
    pub running: bool,
    pub healthy: bool,
    pub pid: Option<u32>,
    pub started_epoch: u64,
    pub restart_count: u64,
    pub state: String,
    pub detail: String,
    pub last_exit: String,
    pub last_error: String,
}

#[cfg(feature = "voice-workers")]
#[derive(Default)]
pub struct AssetResume {
    tts: bool,
    voice: bool,
}

impl ComponentManager {
    pub fn start(config: &Config) -> Result<Arc<Self>> {
        let settings = load_settings(&config.components_file)?;
        let enabled = config.component_manager_enabled;
        let components = ComponentId::ALL
            .into_iter()
            .map(|id| {
                let desired = settings.autostart(id)
                    || (id == ComponentId::Tts && settings.autostart(ComponentId::Voice));
                (id, ManagedComponent::new(enabled && desired))
            })
            .collect();
        let manager = Arc::new(Self {
            enabled,
            executable: std::env::current_exe().context("定位 camera-hub 可执行文件")?,
            settings_path: config.components_file.clone(),
            log_dir: config.log_dir.clone(),
            voice_lib_dir: config.voice_lib_dir.clone(),
            config: config.clone(),
            state: Mutex::new(ManagerState {
                settings,
                components,
            }),
        });
        if enabled {
            let task = manager.clone();
            tokio::spawn(async move {
                task.bootstrap().await;
                task.monitor().await;
            });
        }
        Ok(manager)
    }

    pub async fn overview(&self, voice_status: &VoiceWorkerStatus) -> ComponentsOverview {
        self.reap_exited().await;
        let check_tts = self.state.lock().await.components[&ComponentId::Tts]
            .child
            .is_some();
        let tts_health = if check_tts {
            self.tts_health().await
        } else {
            None
        };
        let ir_health = self.ir_health().await;
        let state = self.state.lock().await;
        ComponentsOverview {
            control_available: self.enabled,
            tts: self.component_status(
                &state,
                ComponentId::Tts,
                tts_health.as_ref(),
                ir_health,
                voice_status,
            ),
            voice: self.component_status(&state, ComponentId::Voice, None, ir_health, voice_status),
            ddns: self.component_status(&state, ComponentId::Ddns, None, ir_health, voice_status),
            ir: self.component_status(&state, ComponentId::Ir, None, ir_health, voice_status),
        }
    }

    pub async fn control(&self, service: &str, action: &str) -> Result<()> {
        if !self.enabled {
            bail!("当前部署未启用内置组件管理器");
        }
        let id = ComponentId::parse(service)?;
        if !matches!(action, "start" | "stop" | "restart") {
            bail!("不支持的组件操作：{action}");
        }
        if !self.installed(id) {
            bail!("组件 {} 尚未安装完整运行资源", id.as_str());
        }
        match action {
            "start" => self.start_component(id).await,
            "stop" => self.stop_component(id, false).await,
            "restart" => {
                self.stop_component(id, true).await?;
                self.start_component(id).await
            }
            _ => unreachable!(),
        }
    }

    pub async fn set_autostart(&self, service: &str, enabled: bool) -> Result<()> {
        if !self.enabled {
            bail!("当前部署未启用内置组件管理器");
        }
        let id = ComponentId::parse(service)?;
        let mut state = self.state.lock().await;
        state.settings.set_autostart(id, enabled);
        save_settings(&self.settings_path, &state.settings)
    }

    #[cfg(feature = "voice-workers")]
    pub async fn quiesce_for_asset(&self, asset: &str) -> Result<AssetResume> {
        if !self.enabled {
            return Ok(AssetResume::default());
        }
        let resume = {
            let state = self.state.lock().await;
            AssetResume {
                tts: asset == "voice-tts" && should_resume(&state.components[&ComponentId::Tts]),
                voice: should_resume(&state.components[&ComponentId::Voice]),
            }
        };
        match asset {
            "voice-kws" | "voice-asr" => {
                self.stop_component(ComponentId::Voice, false).await?;
            }
            "voice-tts" => {
                self.stop_component(ComponentId::Voice, false).await?;
                self.stop_component(ComponentId::Tts, false).await?;
            }
            _ => bail!("不支持的资源包：{asset}"),
        }
        Ok(resume)
    }

    #[cfg(feature = "voice-workers")]
    pub async fn resume_after_asset(&self, asset: &str, resume: AssetResume) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if asset == "voice-tts" && resume.tts {
            self.start_component(ComponentId::Tts).await?;
        }
        if resume.voice {
            self.start_component(ComponentId::Voice).await?;
        }
        Ok(())
    }

    pub async fn shutdown(&self) {
        let children = {
            let mut state = self.state.lock().await;
            state
                .components
                .values_mut()
                .filter_map(|component| {
                    component.desired_running = false;
                    component.child.take()
                })
                .collect::<Vec<_>>()
        };
        for mut child in children {
            terminate_child(&mut child).await;
        }
    }

    async fn bootstrap(&self) {
        let _ = self.start_if_desired(ComponentId::Tts).await;
        let _ = self.start_if_desired(ComponentId::Ddns).await;
        if self.desired(ComponentId::Voice).await
            && self.installed(ComponentId::Voice)
            && let Err(error) = self.start_component(ComponentId::Voice).await
        {
            self.record_error(ComponentId::Voice, error.to_string())
                .await;
        }
    }

    async fn monitor(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            self.reap_exited().await;
            for id in ComponentId::ALL {
                let should_start = {
                    let state = self.state.lock().await;
                    let component = &state.components[&id];
                    component.desired_running
                        && component.child.is_none()
                        && component.next_start_epoch <= epoch_seconds()
                        && self.installed(id)
                };
                if !should_start {
                    continue;
                }
                if id == ComponentId::Voice && self.tts_health().await.is_none() {
                    continue;
                }
                let _ = self.spawn_component(id).await;
            }
        }
    }

    async fn start_if_desired(&self, id: ComponentId) -> Result<()> {
        if self.desired(id).await && self.installed(id) {
            self.spawn_component(id).await?;
        }
        Ok(())
    }

    async fn start_component(&self, id: ComponentId) -> Result<()> {
        {
            let mut state = self.state.lock().await;
            state
                .components
                .get_mut(&id)
                .expect("managed component")
                .desired_running = true;
        }
        if id == ComponentId::Voice {
            if !self.installed(ComponentId::Tts) {
                bail!("TTS 运行资源未安装，无法启动语音识别");
            }
            {
                let mut state = self.state.lock().await;
                state
                    .components
                    .get_mut(&ComponentId::Tts)
                    .expect("TTS component")
                    .desired_running = true;
            }
            self.spawn_component(ComponentId::Tts).await?;
            self.wait_for_tts().await?;
        }
        self.spawn_component(id).await
    }

    async fn spawn_component(&self, id: ComponentId) -> Result<()> {
        if !self.installed(id) {
            bail!("组件 {} 尚未安装完整运行资源", id.as_str());
        }
        let mut state = self.state.lock().await;
        let component = state.components.get_mut(&id).expect("managed component");
        if component.child.is_some() || !component.desired_running {
            return Ok(());
        }
        let log_path = self.log_dir.join(id.log_name());
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("打开组件日志 {}", log_path.display()))?;
        let stderr = stdout.try_clone()?;
        let mut command = Command::new(&self.executable);
        command
            .arg("worker")
            .arg(id.as_str())
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .kill_on_drop(true);
        let library_path = std::env::var_os("LD_LIBRARY_PATH")
            .map(|current| {
                let mut value = OsString::from(self.voice_lib_dir.as_os_str());
                value.push(":");
                value.push(current);
                value
            })
            .unwrap_or_else(|| self.voice_lib_dir.as_os_str().to_owned());
        command.env("LD_LIBRARY_PATH", library_path);
        #[cfg(target_os = "linux")]
        unsafe {
            command.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        match command.spawn() {
            Ok(child) => {
                component.child = Some(child);
                component.started_epoch = epoch_seconds();
                component.next_start_epoch = 0;
                component.last_error.clear();
                Ok(())
            }
            Err(error) => {
                component.last_error = error.to_string();
                component.next_start_epoch = epoch_seconds().saturating_add(RESTART_DELAY_SECONDS);
                Err(error.into())
            }
        }
    }

    async fn stop_component(&self, id: ComponentId, restart: bool) -> Result<()> {
        let child = {
            let mut state = self.state.lock().await;
            let component = state.components.get_mut(&id).expect("managed component");
            component.desired_running = restart;
            component.child.take()
        };
        if let Some(mut child) = child {
            terminate_child(&mut child).await;
        }
        Ok(())
    }

    async fn reap_exited(&self) {
        let mut state = self.state.lock().await;
        for component in state.components.values_mut() {
            let Some(child) = component.child.as_mut() else {
                continue;
            };
            match child.try_wait() {
                Ok(Some(status)) => {
                    component.last_exit = status.to_string();
                    component.child = None;
                    if component.desired_running {
                        component.restart_count = component.restart_count.saturating_add(1);
                        component.next_start_epoch =
                            epoch_seconds().saturating_add(RESTART_DELAY_SECONDS);
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    component.last_error = error.to_string();
                    component.child = None;
                    component.next_start_epoch =
                        epoch_seconds().saturating_add(RESTART_DELAY_SECONDS);
                }
            }
        }
    }

    async fn desired(&self, id: ComponentId) -> bool {
        self.state.lock().await.components[&id].desired_running
    }

    async fn record_error(&self, id: ComponentId, error: String) {
        self.state
            .lock()
            .await
            .components
            .get_mut(&id)
            .expect("managed component")
            .last_error = error;
    }

    async fn wait_for_tts(&self) -> Result<()> {
        for _ in 0..30 {
            if self.tts_health().await.is_some() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        bail!("TTS 服务未在 30 秒内就绪")
    }

    async fn tts_health(&self) -> Option<Value> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(3))
            .build()
            .ok()?;
        let response = client
            .get(format!(
                "{}/health",
                self.config.tts_url.trim_end_matches('/')
            ))
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json().await.ok()
    }

    async fn ir_health(&self) -> bool {
        let running = self.state.lock().await.components[&ComponentId::Ir]
            .child
            .is_some();
        if !running {
            return false;
        }
        let Some(client) = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(1))
            .timeout(Duration::from_secs(2))
            .build()
            .ok()
        else {
            return false;
        };
        client
            .get(format!(
                "{}/health",
                self.config.ir_url.trim_end_matches('/')
            ))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }

    fn installed(&self, id: ComponentId) -> bool {
        match id {
            ComponentId::Ddns => true,
            ComponentId::Ir => self.config.ir_device.exists(),
            ComponentId::Tts => {
                cfg!(feature = "voice-workers")
                    && self.config.tts_model_dir.join("tokens.txt").is_file()
                    && self
                        .config
                        .tts_model_dir
                        .join("encoder.int8.onnx")
                        .is_file()
                    && self
                        .config
                        .tts_model_dir
                        .join("decoder.int8.onnx")
                        .is_file()
                    && self.config.tts_model_dir.join("lexicon.txt").is_file()
                    && self.config.tts_model_dir.join("espeak-ng-data").is_dir()
                    && self.config.tts_vocoder.is_file()
            }
            ComponentId::Voice => {
                cfg!(feature = "voice-workers")
                    && self.config.voice_model_dir.join("tokens.txt").is_file()
                    && self
                        .config
                        .voice_model_dir
                        .join("encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx")
                        .is_file()
                    && self
                        .config
                        .voice_model_dir
                        .join("decoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx")
                        .is_file()
                    && self
                        .config
                        .voice_model_dir
                        .join("joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx")
                        .is_file()
            }
        }
    }

    fn component_status(
        &self,
        state: &ManagerState,
        id: ComponentId,
        tts_health: Option<&Value>,
        ir_health: bool,
        voice_status: &VoiceWorkerStatus,
    ) -> ComponentStatus {
        let component = &state.components[&id];
        let installed = self.installed(id);
        let running = component.child.is_some();
        let pid = component.child.as_ref().and_then(Child::id);
        let (healthy, service_state, detail) = match id {
            ComponentId::Tts if running => match tts_health {
                Some(health) => {
                    let loaded = health
                        .get("loaded")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let busy = health.get("busy").and_then(Value::as_bool).unwrap_or(false);
                    (
                        true,
                        if busy {
                            "busy".to_owned()
                        } else if loaded {
                            "ready".to_owned()
                        } else {
                            "idle".to_owned()
                        },
                        if busy {
                            "正在生成语音".to_owned()
                        } else if loaded {
                            "模型已加载".to_owned()
                        } else {
                            "服务空闲，模型尚未加载".to_owned()
                        },
                    )
                }
                None => (false, "starting".to_owned(), "等待 TTS 健康检查".to_owned()),
            },
            ComponentId::Voice if running => {
                let fresh = voice_status.updated_epoch > 0
                    && epoch_seconds().saturating_sub(voice_status.updated_epoch)
                        <= STATUS_STALE_SECONDS;
                (
                    voice_status.available && fresh,
                    if fresh {
                        voice_status.state.clone()
                    } else {
                        "starting".to_owned()
                    },
                    if !voice_status.last_error.is_empty() {
                        voice_status.last_error.clone()
                    } else if fresh && voice_status.running {
                        "正在监听关键词".to_owned()
                    } else if fresh {
                        "进程运行中，语音监听已关闭".to_owned()
                    } else {
                        "等待语音状态心跳".to_owned()
                    },
                )
            }
            ComponentId::Ir if running && ir_health => {
                (true, "ready".to_owned(), "红外发射服务已就绪".to_owned())
            }
            ComponentId::Ir if running => (
                false,
                "starting".to_owned(),
                "等待红外 worker 健康检查".to_owned(),
            ),
            _ if !installed => (false, "unavailable".to_owned(), "运行资源未安装".to_owned()),
            _ if !running && component.desired_running => (
                false,
                "starting".to_owned(),
                component.last_error.clone().if_empty("等待组件管理器启动"),
            ),
            _ if !running => (false, "stopped".to_owned(), "进程已停止".to_owned()),
            ComponentId::Ddns => (true, "running".to_owned(), "DDNS worker 运行中".to_owned()),
            _ => (true, "running".to_owned(), "进程运行中".to_owned()),
        };
        ComponentStatus {
            id: id.as_str(),
            installed,
            autostart: state.settings.autostart(id),
            desired_running: component.desired_running,
            running,
            healthy,
            pid,
            started_epoch: component.started_epoch,
            restart_count: component.restart_count,
            state: service_state,
            detail,
            last_exit: component.last_exit.clone(),
            last_error: component.last_error.clone(),
        }
    }
}

#[cfg(feature = "voice-workers")]
fn should_resume(component: &ManagedComponent) -> bool {
    component.desired_running || component.child.is_some()
}

trait StringFallback {
    fn if_empty(self, fallback: &str) -> String;
}

impl StringFallback for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_owned()
        } else {
            self
        }
    }
}

async fn terminate_child(child: &mut Child) {
    if let Some(pid) = child.id() {
        #[cfg(unix)]
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
    }
    if tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .is_err()
    {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}

fn load_settings(path: &Path) -> Result<ComponentSettings> {
    let settings = match fs::read(path) {
        Ok(data) => serde_json::from_slice::<ComponentSettings>(&data)
            .with_context(|| format!("解析组件配置 {}", path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ComponentSettings::default(),
        Err(error) => return Err(error.into()),
    };
    save_settings(path, &settings)?;
    Ok(settings)
}

fn save_settings(path: &Path, settings: &ComponentSettings) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(settings)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(temporary, path)?;
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

    #[test]
    fn parses_only_known_components() {
        assert_eq!(ComponentId::parse("tts").unwrap(), ComponentId::Tts);
        assert_eq!(ComponentId::parse("voice").unwrap(), ComponentId::Voice);
        assert_eq!(ComponentId::parse("ir").unwrap(), ComponentId::Ir);
        assert!(ComponentId::parse("camera-hub").is_err());
    }

    #[test]
    fn defaults_all_existing_workers_to_autostart() {
        let settings = ComponentSettings::default();
        assert!(settings.autostart(ComponentId::Tts));
        assert!(settings.autostart(ComponentId::Voice));
        assert!(settings.autostart(ComponentId::Ddns));
        assert!(settings.autostart(ComponentId::Ir));
    }
}
