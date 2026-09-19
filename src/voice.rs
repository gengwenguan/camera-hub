use crate::config::Config;
use crate::voice_config::{
    MAX_PHRASES_PER_COMMAND, VOICE_CONFIG_VERSION, VoiceCommand, VoiceConfig, VoiceEvent,
    VoiceTestRequest, VoiceTranscribeRequest, VoiceWorkerStatus,
};
use crate::voice_tts::{
    DEFAULT_VOICE_PROFILE_ID, TtsProfileResponse, VOICE_REFERENCE_PROMPT, VoiceReferenceUpload,
    VoiceTtsClient, decode_reference_audio,
};
use anyhow::{Context, Result, bail};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex as AsyncMutex;
use tracing::warn;

const MAX_EVENT_READ_BYTES: u64 = 256 * 1024;

#[derive(Debug)]
pub struct VoiceRevisionConflict;

impl std::fmt::Display for VoiceRevisionConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("语音配置已被其他会话更新，请重新加载后再保存")
    }
}

impl std::error::Error for VoiceRevisionConflict {}

pub struct VoiceService {
    config_path: PathBuf,
    status_path: PathBuf,
    events_path: PathBuf,
    command_path: PathBuf,
    transcribe_path: PathBuf,
    current: RwLock<VoiceConfig>,
    test_queue: Mutex<()>,
    profile_update: AsyncMutex<()>,
    tts: VoiceTtsClient,
}

impl VoiceService {
    pub fn load(config: &Config) -> Result<Self> {
        let mut config_migrated = false;
        let mut current = match fs::read(&config.voice_config_file) {
            Ok(data) => match parse_voice_config(&data, &config.voice_config_file) {
                Ok((loaded, migrated)) => {
                    config_migrated = migrated;
                    loaded
                }
                Err(error) => {
                    let fallback = VoiceConfig::default().normalize()?;
                    let backup = replace_invalid_config(&config.voice_config_file, &fallback)?;
                    warn!(
                        path = %config.voice_config_file.display(),
                        backup = %backup.display(),
                        %error,
                        "invalid voice config moved aside and replaced with defaults"
                    );
                    fallback
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                VoiceConfig::default().normalize()?
            }
            Err(error) => return Err(error.into()),
        };
        let commands_added = merge_air_conditioner_commands(&mut current, &config.ir_url)?;
        let service = Self {
            config_path: config.voice_config_file.clone(),
            status_path: config.voice_status_file.clone(),
            events_path: config.voice_events_file.clone(),
            command_path: config.voice_command_file.clone(),
            transcribe_path: config.voice_transcribe_file.clone(),
            current: RwLock::new(current),
            test_queue: Mutex::new(()),
            profile_update: AsyncMutex::new(()),
            tts: VoiceTtsClient::new(&config.tts_url, &config.tts_token)?,
        };
        if !service.config_path.is_file() || config_migrated || commands_added {
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
        let mut current = self
            .current
            .write()
            .unwrap_or_else(|error| error.into_inner());
        if next.revision != current.revision {
            return Err(VoiceRevisionConflict.into());
        }
        next.voice_profile_revision = current.voice_profile_revision;
        next.revision = current.revision.saturating_add(1);
        next = next.normalize()?;
        if next.enabled && !next.nlu_enabled && !self.status().asr_available {
            next.keyword_buffer()?;
        }
        self.save(&next)?;
        *current = next.clone();
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
        let Ok(mut file) = File::open(&self.events_path) else {
            return Vec::new();
        };
        let Ok(length) = file.metadata().map(|metadata| metadata.len()) else {
            return Vec::new();
        };
        let offset = length.saturating_sub(MAX_EVENT_READ_BYTES);
        if file.seek(SeekFrom::Start(offset)).is_err() {
            return Vec::new();
        }
        let mut data = Vec::with_capacity((length - offset) as usize);
        if file.read_to_end(&mut data).is_err() {
            return Vec::new();
        }
        let data = String::from_utf8_lossy(&data);
        let mut lines = data.lines().collect::<Vec<_>>();
        if offset > 0 && !lines.is_empty() {
            lines.remove(0);
        }
        let start = lines.len().saturating_sub(limit.min(200));
        lines[start..]
            .iter()
            .rev()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    pub fn queue_test(&self, mut request: VoiceTestRequest) -> Result<()> {
        let _queue = self
            .test_queue
            .lock()
            .unwrap_or_else(|error| error.into_inner());
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
        let now = epoch_seconds();
        match fs::read(&self.command_path) {
            Ok(data) => {
                let pending = serde_json::from_slice::<VoiceTestRequest>(&data).ok();
                if pending.is_some_and(|pending| pending.is_fresh(now)) {
                    bail!("已有语音测试等待执行，请稍后重试");
                }
                match fs::remove_file(&self.command_path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        request.created_epoch = now;
        write_json(&self.command_path, &request)
    }

    /// 投递一次「唤醒 → 转写」请求（Phase 1）。worker 会在限时窗口内转写整句，
    /// 结果写入 `VoiceWorkerStatus.asr_transcript`，不触发任何命令执行。
    pub fn queue_transcribe(&self, window_ms: u64) -> Result<VoiceTranscribeRequest> {
        let _queue = self
            .test_queue
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let status = self.status();
        if !status.asr_available {
            bail!("自然语言识别模型尚未安装");
        }
        let now = epoch_seconds();
        match fs::read(&self.transcribe_path) {
            Ok(data) => {
                let pending = serde_json::from_slice::<VoiceTranscribeRequest>(&data).ok();
                if pending.is_some_and(|pending| pending.is_fresh(now)) {
                    bail!("已有语音转写等待执行，请稍后重试");
                }
                match fs::remove_file(&self.transcribe_path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let request = VoiceTranscribeRequest {
            window_ms: window_ms.clamp(1_000, 15_000),
            created_epoch: now,
        };
        write_json(&self.transcribe_path, &request)?;
        Ok(request)
    }

    pub async fn enroll_reference(
        &self,
        upload: VoiceReferenceUpload,
    ) -> Result<(VoiceConfig, TtsProfileResponse)> {
        let wav = decode_reference_audio(&upload.audio_base64)?;
        let _operation = self.profile_update.lock().await;
        let profile = self
            .tts
            .enroll(DEFAULT_VOICE_PROFILE_ID, &wav, VOICE_REFERENCE_PROMPT)
            .await?;
        let mut current = self
            .current
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let mut next = current.clone();
        next.voice_profile_revision = next.voice_profile_revision.saturating_add(1).max(1);
        next.revision = next.revision.saturating_add(1);
        self.save(&next)?;
        *current = next.clone();
        Ok((next, profile))
    }

    pub async fn delete_reference(&self) -> Result<VoiceConfig> {
        let _operation = self.profile_update.lock().await;
        self.tts.delete_profile(DEFAULT_VOICE_PROFILE_ID).await?;
        let mut current = self
            .current
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let mut next = current.clone();
        next.voice_profile_revision = 0;
        next.revision = next.revision.saturating_add(1);
        self.save(&next)?;
        *current = next.clone();
        Ok(next)
    }

    pub fn reference_prompt(&self) -> &'static str {
        VOICE_REFERENCE_PROMPT
    }

    pub fn paths(&self) -> (&Path, &Path, &Path) {
        (&self.config_path, &self.status_path, &self.events_path)
    }

    fn save(&self, config: &VoiceConfig) -> Result<()> {
        write_json(&self.config_path, config)
    }
}

fn parse_voice_config(data: &[u8], path: &Path) -> Result<(VoiceConfig, bool)> {
    let config = serde_json::from_slice::<VoiceConfig>(data)
        .with_context(|| format!("parse voice config {}", path.display()))?;
    let migrated = config.version < VOICE_CONFIG_VERSION
        || config
            .commands
            .iter()
            .any(|command| command.phrases.is_empty());
    let mut config = config.normalize()?;
    if migrated {
        config.revision = config.revision.saturating_add(1);
    }
    Ok((config, migrated))
}

fn merge_air_conditioner_commands(config: &mut VoiceConfig, ir_url: &str) -> Result<bool> {
    let base_url = ir_url.trim_end_matches('/');
    let candidates = [
        VoiceCommand {
            id: "ac-on".to_owned(),
            enabled: true,
            phrase: "小雨打开空调".to_owned(),
            phrases: vec!["小雨打开空调".to_owned(), "小雨开空调".to_owned()],
            reply: "好的，空调已设置为制冷二十六度并开启节能模式".to_owned(),
            method: "POST".to_owned(),
            url: format!("{base_url}/v1/actions/ac-on"),
            body: String::new(),
            boosting_score: 1.5,
            trigger_threshold: 0.05,
            cooldown_ms: 500,
        },
        VoiceCommand {
            id: "ac-cool".to_owned(),
            enabled: true,
            phrase: "小雨制冷模式".to_owned(),
            phrases: vec![
                "小雨制冷模式".to_owned(),
                "小雨打开制冷".to_owned(),
                "小雨开制冷".to_owned(),
            ],
            reply: "好的，空调已设置为制冷二十六度".to_owned(),
            method: "POST".to_owned(),
            url: format!("{base_url}/v1/actions/ac-cool"),
            body: String::new(),
            boosting_score: 1.5,
            trigger_threshold: 0.05,
            cooldown_ms: 500,
        },
        VoiceCommand {
            id: "ac-dry".to_owned(),
            enabled: true,
            phrase: "小雨抽湿模式".to_owned(),
            phrases: vec![
                "小雨抽湿模式".to_owned(),
                "小雨打开抽湿".to_owned(),
                "小雨除湿模式".to_owned(),
            ],
            reply: "好的，空调已设置为抽湿模式二十六度".to_owned(),
            method: "POST".to_owned(),
            url: format!("{base_url}/v1/actions/ac-dry"),
            body: String::new(),
            boosting_score: 1.5,
            trigger_threshold: 0.05,
            cooldown_ms: 500,
        },
        VoiceCommand {
            id: "ac-off".to_owned(),
            enabled: true,
            phrase: "小雨关闭空调".to_owned(),
            phrases: vec!["小雨关闭空调".to_owned(), "小雨关空调".to_owned()],
            reply: "好的，空调已关闭".to_owned(),
            method: "POST".to_owned(),
            url: format!("{base_url}/v1/actions/ac-off"),
            body: String::new(),
            boosting_score: 1.5,
            trigger_threshold: 0.05,
            cooldown_ms: 500,
        },
    ];
    let mut used_phrases = config
        .commands
        .iter()
        .flat_map(|command| command.phrases.iter().cloned())
        .collect::<std::collections::HashSet<_>>();
    let mut changed = false;
    for mut command in candidates {
        if let Some(current) = config
            .commands
            .iter_mut()
            .find(|current| current.id == command.id)
        {
            if current.url == command.url {
                if current.trigger_threshold == 0.45 {
                    current.trigger_threshold = command.trigger_threshold;
                    changed = true;
                }
                for phrase in command.phrases {
                    if current.phrases.len() >= MAX_PHRASES_PER_COMMAND {
                        break;
                    }
                    if used_phrases.insert(phrase.clone()) {
                        current.phrases.push(phrase);
                        changed = true;
                    }
                }
            }
            continue;
        }
        if config.commands.len() >= 32 {
            warn!("voice command limit reached; skip built-in air conditioner commands");
            break;
        }
        command
            .phrases
            .retain(|phrase| used_phrases.insert(phrase.clone()));
        let Some(primary) = command.phrases.first().cloned() else {
            continue;
        };
        command.phrase = primary;
        config.commands.push(command);
        changed = true;
    }
    if changed {
        config.revision = config.revision.saturating_add(1);
        *config = config.clone().normalize()?;
    }
    Ok(changed)
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&temporary, path)?;
    Ok(())
}

fn replace_invalid_config(path: &Path, fallback: &VoiceConfig) -> Result<PathBuf> {
    let backup = path.with_extension(format!(
        "json.invalid-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::rename(path, &backup)
        .with_context(|| format!("backup invalid voice config {}", path.display()))?;
    if let Err(error) = write_json(path, fallback) {
        let _ = fs::rename(&backup, path);
        return Err(error).context("replace invalid voice config with defaults");
    }
    Ok(backup)
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

    fn service(root: &Path) -> VoiceService {
        VoiceService {
            config_path: root.join("voice.json"),
            status_path: root.join("voice-status.json"),
            events_path: root.join("events.jsonl"),
            command_path: root.join("voice-command.json"),
            transcribe_path: root.join("voice-transcribe.json"),
            current: RwLock::new(VoiceConfig::default()),
            test_queue: Mutex::new(()),
            profile_update: AsyncMutex::new(()),
            tts: VoiceTtsClient::new("http://127.0.0.1:39081", "").unwrap(),
        }
    }

    fn temporary_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "camera-hub-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn rejects_stale_config_revision() {
        let root = temporary_root("voice-revision");
        let service = service(&root);
        let mut stale = service.current();
        stale.revision = 0;

        assert!(service.update(stale).is_err());
        assert_eq!(service.current().revision, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn allows_natural_control_without_fixed_commands_and_preserves_opt_in() {
        let root = temporary_root("voice-nlu");
        let service = service(&root);
        let mut config = service.current();
        config.enabled = true;
        config.commands.clear();
        assert!(service.update(config.clone()).is_err());
        config.nlu_enabled = true;
        let saved = service.update(config).unwrap();
        assert!(saved.enabled && saved.nlu_enabled);
        assert!(saved.commands.is_empty());
        let mut legacy = serde_json::to_value(&saved).unwrap();
        legacy.as_object_mut().unwrap().remove("nlu_enabled");
        legacy["version"] = serde_json::json!(3);
        let (migrated, changed) = parse_voice_config(
            &serde_json::to_vec(&legacy).unwrap(),
            Path::new("voice.json"),
        )
        .unwrap();
        assert!(changed);
        assert!(!migrated.nlu_enabled);
        assert_eq!(migrated.version, VOICE_CONFIG_VERSION);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn migrates_voice_config_and_advances_revision() {
        let mut legacy = VoiceConfig::default();
        legacy.version = 2;
        legacy.revision = 17;
        legacy.global_cooldown_ms = 2_000;
        legacy.commands[0].cooldown_ms = 5_000;
        let data = serde_json::to_vec(&legacy).unwrap();

        let (migrated, changed) = parse_voice_config(&data, Path::new("voice.json")).unwrap();

        assert!(changed);
        assert_eq!(migrated.version, VOICE_CONFIG_VERSION);
        assert_eq!(migrated.revision, 18);
        assert_eq!(migrated.global_cooldown_ms, 500);
        assert_eq!(migrated.commands[0].cooldown_ms, 500);
    }

    #[test]
    fn rejects_fresh_test_but_replaces_stale_test() {
        let root = temporary_root("voice-queue");
        let service = service(&root);
        let request = VoiceTestRequest {
            command_id: "light-on".to_owned(),
            call_url: false,
            speak_reply: true,
            created_epoch: 0,
        };
        service.queue_test(request.clone()).unwrap();
        assert!(service.queue_test(request.clone()).is_err());

        let mut stale = request.clone();
        stale.created_epoch = epoch_seconds()
            .saturating_sub(crate::voice_config::VOICE_TEST_REQUEST_MAX_AGE_SECS + 1);
        write_json(&service.command_path, &stale).unwrap();
        service.queue_test(request).unwrap();

        let queued: VoiceTestRequest =
            serde_json::from_slice(&fs::read(&service.command_path).unwrap()).unwrap();
        assert!(queued.is_fresh(epoch_seconds()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn queues_transcribe_only_when_asr_available_and_replaces_stale() {
        let root = temporary_root("voice-transcribe");
        let service = service(&root);

        assert!(service.queue_transcribe(6_000).is_err());

        let status = VoiceWorkerStatus {
            asr_available: true,
            ..VoiceWorkerStatus::default()
        };
        write_json(&service.status_path, &status).unwrap();

        let request = service.queue_transcribe(60_000).unwrap();
        assert_eq!(request.window_ms, 15_000);
        assert!(service.queue_transcribe(6_000).is_err());

        let mut stale = request;
        stale.created_epoch = epoch_seconds()
            .saturating_sub(crate::voice_config::VOICE_TEST_REQUEST_MAX_AGE_SECS + 1);
        write_json(&service.transcribe_path, &stale).unwrap();
        let refreshed = service.queue_transcribe(500).unwrap();
        assert_eq!(refreshed.window_ms, 1_000);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn backs_up_invalid_config_before_installing_defaults() {
        let root = temporary_root("voice-invalid");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("voice.json");
        fs::write(&path, b"{invalid").unwrap();
        let fallback = VoiceConfig::default().normalize().unwrap();

        let backup = replace_invalid_config(&path, &fallback).unwrap();

        assert_eq!(fs::read(&backup).unwrap(), b"{invalid");
        let stored: VoiceConfig = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(stored, fallback);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn merges_air_conditioner_commands_without_overwriting_existing_commands() {
        let mut config = VoiceConfig::default();
        let original = config.commands.clone();
        assert!(merge_air_conditioner_commands(&mut config, "http://127.0.0.1:39182/").unwrap());
        assert_eq!(&config.commands[..original.len()], original.as_slice());
        assert!(config.commands.iter().any(|command| {
            command.id == "ac-on"
                && command.enabled
                && command.url == "http://127.0.0.1:39182/v1/actions/ac-on"
                && command.phrases.contains(&"小雨开空调".to_owned())
        }));
        assert!(config.commands.iter().any(|command| {
            command.id == "ac-cool"
                && command.enabled
                && command.url == "http://127.0.0.1:39182/v1/actions/ac-cool"
                && command.phrases.contains(&"小雨制冷模式".to_owned())
        }));
        assert!(config.commands.iter().any(|command| {
            command.id == "ac-dry"
                && command.enabled
                && command.url == "http://127.0.0.1:39182/v1/actions/ac-dry"
                && command.phrases.contains(&"小雨抽湿模式".to_owned())
        }));
        assert!(config.commands.iter().any(|command| {
            command.id == "ac-off"
                && command.enabled
                && command.url == "http://127.0.0.1:39182/v1/actions/ac-off"
                && command.phrases.contains(&"小雨关空调".to_owned())
        }));
        let cool = config
            .commands
            .iter_mut()
            .find(|command| command.id == "ac-cool")
            .unwrap();
        cool.trigger_threshold = 0.45;
        assert!(merge_air_conditioner_commands(&mut config, "http://127.0.0.1:39182").unwrap());
        assert_eq!(
            config
                .commands
                .iter()
                .find(|command| command.id == "ac-cool")
                .unwrap()
                .trigger_threshold,
            0.05
        );
        let revision = config.revision;
        assert!(!merge_air_conditioner_commands(&mut config, "http://example.invalid").unwrap());
        assert_eq!(config.revision, revision);
    }
}
