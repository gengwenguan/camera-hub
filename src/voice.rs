use crate::config::Config;
use crate::voice_config::{VoiceConfig, VoiceEvent, VoiceTestRequest, VoiceWorkerStatus};
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct VoiceService {
    config_path: PathBuf,
    status_path: PathBuf,
    events_path: PathBuf,
    command_path: PathBuf,
    current: RwLock<VoiceConfig>,
}

impl VoiceService {
    pub fn load(config: &Config) -> Result<Self> {
        let current = match fs::read(&config.voice_config_file) {
            Ok(data) => serde_json::from_slice::<VoiceConfig>(&data)
                .with_context(|| {
                    format!("parse voice config {}", config.voice_config_file.display())
                })?
                .normalize()?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                VoiceConfig::default().normalize()?
            }
            Err(error) => return Err(error.into()),
        };
        let service = Self {
            config_path: config.voice_config_file.clone(),
            status_path: config.voice_status_file.clone(),
            events_path: config.voice_events_file.clone(),
            command_path: config.voice_command_file.clone(),
            current: RwLock::new(current),
        };
        if !service.config_path.is_file() {
            service.save(&service.current())?;
        }
        Ok(service)
    }

    pub fn current(&self) -> VoiceConfig {
        self.current
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn update(&self, mut next: VoiceConfig) -> Result<VoiceConfig> {
        next.revision = self.current().revision.saturating_add(1);
        next = next.normalize()?;
        if next.enabled {
            next.keyword_buffer()?;
        }
        self.save(&next)?;
        *self
            .current
            .write()
            .unwrap_or_else(|error| error.into_inner()) = next.clone();
        Ok(next)
    }

    pub fn status(&self) -> VoiceWorkerStatus {
        fs::read(&self.status_path)
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_else(|| VoiceWorkerStatus {
                state: "stopped".to_owned(),
                last_error: "语音控制进程尚未写入状态".to_owned(),
                ..VoiceWorkerStatus::default()
            })
    }

    pub fn events(&self, limit: usize) -> Vec<VoiceEvent> {
        let Ok(data) = fs::read_to_string(&self.events_path) else {
            return Vec::new();
        };
        let lines = data.lines().collect::<Vec<_>>();
        let start = lines.len().saturating_sub(limit.min(200));
        lines[start..]
            .iter()
            .rev()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    pub fn queue_test(&self, mut request: VoiceTestRequest) -> Result<()> {
        let config = self.current();
        let command = config
            .commands
            .iter()
            .find(|command| command.id == request.command_id)
            .ok_or_else(|| anyhow::anyhow!("语音命令不存在"))?;
        if request.call_url && command.url.is_empty() {
            bail!("该命令尚未配置 URL");
        }
        if !request.call_url && !request.speak_reply {
            bail!("测试至少需要播放回复或请求 URL");
        }
        request.created_epoch = epoch_seconds();
        write_json(&self.command_path, &request)
    }

    pub fn paths(&self) -> (&Path, &Path, &Path) {
        (&self.config_path, &self.status_path, &self.events_path)
    }

    fn save(&self, config: &VoiceConfig) -> Result<()> {
        write_json(&self.config_path, config)
    }
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
