use anyhow::{Result, bail};
use pinyin::ToPinyin;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const VOICE_CONFIG_VERSION: u32 = 4;
pub const VOICE_TEST_REQUEST_MAX_AGE_SECS: u64 = 60;
pub const MAX_PHRASES_PER_COMMAND: usize = 8;
const MAX_TOTAL_PHRASES: usize = 96;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct VoiceConfig {
    pub version: u32,
    pub revision: u64,
    pub enabled: bool,
    pub nlu_enabled: bool,
    pub capture_device: String,
    pub playback_device: String,
    pub playback_volume: u8,
    pub voice_profile_revision: u64,
    pub capture_rate: i32,
    pub request_timeout_ms: u64,
    pub global_cooldown_ms: u64,
    pub failure_reply: String,
    pub commands: Vec<VoiceCommand>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct VoiceCommand {
    pub id: String,
    pub enabled: bool,
    pub phrase: String,
    pub phrases: Vec<String>,
    pub reply: String,
    pub method: String,
    pub url: String,
    pub body: String,
    pub boosting_score: f64,
    pub trigger_threshold: f64,
    pub cooldown_ms: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct VoiceWorkerStatus {
    pub available: bool,
    pub running: bool,
    pub state: String,
    pub model: String,
    pub capture_device: String,
    pub playback_device: String,
    pub config_revision: u64,
    pub detected_count: u64,
    pub audio_rms: f32,
    pub last_keyword: String,
    pub tts_state: String,
    pub last_error: String,
    pub updated_epoch: u64,
    pub asr_available: bool,
    pub asr_state: String,
    pub asr_transcript: String,
    pub asr_transcript_epoch: u64,
    pub asr_error: String,
    pub nlu_state: crate::voice_nlu::ExecutionState,
    pub nlu_intent: Option<crate::ir::AirconCommand>,
    pub nlu_message: String,
    pub nlu_epoch: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct VoiceEvent {
    pub epoch: u64,
    pub command_id: String,
    pub phrase: String,
    pub source: String,
    pub success: bool,
    pub http_status: u16,
    pub elapsed_ms: u64,
    pub message: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct VoiceTestRequest {
    pub command_id: String,
    pub call_url: bool,
    pub speak_reply: bool,
    pub created_epoch: u64,
}

impl VoiceTestRequest {
    pub fn is_fresh(&self, now: u64) -> bool {
        self.created_epoch != 0
            && self.created_epoch <= now.saturating_add(5)
            && now.saturating_sub(self.created_epoch) <= VOICE_TEST_REQUEST_MAX_AGE_SECS
    }
}

/// On-demand streaming ASR request dropped by the Web UI for the voice worker.
///
/// 手动测试始终只转写与预览规则解析，不触发任何命令执行。
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct VoiceTranscribeRequest {
    pub window_ms: u64,
    pub created_epoch: u64,
}

impl VoiceTranscribeRequest {
    pub fn is_fresh(&self, now: u64) -> bool {
        self.created_epoch != 0
            && self.created_epoch <= now.saturating_add(5)
            && now.saturating_sub(self.created_epoch) <= VOICE_TEST_REQUEST_MAX_AGE_SECS
    }

    #[cfg_attr(not(feature = "voice-workers"), allow(dead_code))]
    pub fn clamped_window(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.window_ms.clamp(1_000, 15_000))
    }
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            version: VOICE_CONFIG_VERSION,
            revision: 1,
            enabled: false,
            nlu_enabled: false,
            capture_device: "hw:0,0".to_owned(),
            playback_device: "plughw:0,0".to_owned(),
            playback_volume: 60,
            voice_profile_revision: 0,
            capture_rate: 48_000,
            request_timeout_ms: 3_000,
            global_cooldown_ms: 500,
            failure_reply: "操作失败，请稍后再试".to_owned(),
            commands: vec![
                VoiceCommand::new("light-on", "小雨开灯", "好的，已经开灯"),
                VoiceCommand::new("light-off", "小雨关灯", "好的，已经关灯"),
                VoiceCommand {
                    trigger_threshold: 0.60,
                    ..VoiceCommand::new("door-open", "小雨开门", "好的，正在开门")
                },
                VoiceCommand::new("delivery", "小雨外卖", "好的，正在处理外卖请求"),
            ],
        }
    }
}

impl Default for VoiceCommand {
    fn default() -> Self {
        Self::new("", "", "")
    }
}

impl VoiceCommand {
    fn new(id: &str, phrase: &str, reply: &str) -> Self {
        Self {
            id: id.to_owned(),
            enabled: false,
            phrase: phrase.to_owned(),
            phrases: vec![phrase.to_owned()],
            reply: reply.to_owned(),
            method: "GET".to_owned(),
            url: String::new(),
            body: String::new(),
            boosting_score: 1.5,
            trigger_threshold: 0.05,
            cooldown_ms: 500,
        }
    }
}

impl VoiceConfig {
    pub fn normalize(mut self) -> Result<Self> {
        let previous_version = self.version;
        self.version = VOICE_CONFIG_VERSION;
        self.capture_device = self.capture_device.trim().to_owned();
        self.playback_device = self.playback_device.trim().to_owned();
        self.playback_volume = self.playback_volume.min(100);
        self.capture_rate = self.capture_rate.clamp(8_000, 192_000);
        self.request_timeout_ms = self.request_timeout_ms.clamp(500, 30_000);
        if previous_version < 3 && self.global_cooldown_ms == 2_000 {
            self.global_cooldown_ms = 500;
        }
        self.global_cooldown_ms = self.global_cooldown_ms.clamp(500, 60_000);
        self.failure_reply = clean_text(&self.failure_reply, 120, "失败回复")?;
        if self.capture_device.is_empty() || self.playback_device.is_empty() {
            bail!("录音和播放设备不能为空");
        }
        if self.commands.len() > 32 {
            bail!("语音命令最多支持 32 条");
        }

        let mut ids = HashSet::new();
        let mut phrases = HashSet::new();
        let mut phrase_count = 0;
        for command in &mut self.commands {
            command.id = command.id.trim().to_owned();
            if command.id.is_empty()
                || command.id.len() > 64
                || !command
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                bail!("命令 ID 只能包含 1-64 个字母、数字、短横线或下划线");
            }
            if !ids.insert(command.id.clone()) {
                bail!("命令 ID 不能重复：{}", command.id);
            }
            let legacy_phrase = command.phrase.trim().to_owned();
            if command.phrases.is_empty() {
                command.phrases.push(legacy_phrase);
            } else if !legacy_phrase.is_empty() {
                command.phrases[0] = legacy_phrase;
            }
            if command.phrases.len() > MAX_PHRASES_PER_COMMAND {
                bail!("每个命令最多支持 {MAX_PHRASES_PER_COMMAND} 个触发短语");
            }
            for phrase in &mut command.phrases {
                *phrase = clean_phrase(phrase)?;
                if !phrases.insert(phrase.clone()) {
                    bail!("命令短语不能重复：{phrase}");
                }
            }
            phrase_count += command.phrases.len();
            command.phrase = command.phrases[0].clone();
            command.reply = clean_text(&command.reply, 120, "回复内容")?;
            command.method = command.method.trim().to_ascii_uppercase();
            if !matches!(command.method.as_str(), "GET" | "POST") {
                bail!("命令 {} 仅支持 GET 或 POST", command.phrase);
            }
            command.url = command.url.trim().to_owned();
            command.body = command.body.trim().to_owned();
            if !(command.url.is_empty()
                || command.url.starts_with("http://")
                || command.url.starts_with("https://"))
            {
                bail!("命令 {} 的 URL 必须使用 http 或 https", command.phrase);
            }
            if command.url.len() > 2048 || command.body.len() > 8192 {
                bail!("命令 {} 的 URL 或请求体过长", command.phrase);
            }
            if command.method == "POST"
                && !command.body.trim().is_empty()
                && let Err(error) = serde_json::from_str::<serde_json::Value>(&command.body)
            {
                bail!("命令 {} 的 POST JSON 无效：{error}", command.phrase);
            }
            command.boosting_score = finite_or(command.boosting_score, 1.5).clamp(0.0, 10.0);
            command.trigger_threshold =
                finite_or(command.trigger_threshold, 0.05).clamp(0.01, 0.95);
            if previous_version < 3 && matches!(command.cooldown_ms, 2_000 | 5_000) {
                command.cooldown_ms = 500;
            }
            command.cooldown_ms = command.cooldown_ms.clamp(500, 60_000);
        }
        if phrase_count > MAX_TOTAL_PHRASES {
            bail!("语音配置最多支持 {MAX_TOTAL_PHRASES} 个触发短语");
        }
        Ok(self)
    }

    pub fn enabled_commands(&self) -> impl Iterator<Item = &VoiceCommand> {
        self.commands
            .iter()
            .filter(|command| command.enabled && !command.url.is_empty())
    }

    pub fn keyword_buffer(&self) -> Result<String> {
        let mut lines = Vec::new();
        for command in self.enabled_commands() {
            lines.extend(command.keyword_lines()?);
        }
        if lines.is_empty() {
            bail!("没有已启用且配置 URL 的语音命令");
        }
        Ok(lines.join("\n"))
    }
}

impl VoiceCommand {
    pub fn keyword_lines(&self) -> Result<Vec<String>> {
        self.phrases
            .iter()
            .map(|phrase| self.keyword_line_for(phrase))
            .collect()
    }

    fn keyword_line_for(&self, phrase: &str) -> Result<String> {
        let tokens = partial_pinyin(phrase)?;
        Ok(format!(
            "{} :{:.2} #{:.2} @{}",
            tokens.join(" "),
            self.boosting_score,
            self.trigger_threshold,
            phrase.replace(' ', "_")
        ))
    }
}

fn clean_phrase(value: &str) -> Result<String> {
    let value = value.trim().replace(' ', "");
    let count = value.chars().count();
    if !(2..=24).contains(&count)
        || !value
            .chars()
            .all(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch))
    {
        bail!("命令短语必须是 2-24 个中文字符");
    }
    Ok(value)
}

fn clean_text(value: &str, max_chars: usize, label: &str) -> Result<String> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.chars().count() > max_chars || value.chars().any(char::is_control)
    {
        bail!("{label}不能为空、不能换行且最多 {max_chars} 个字符");
    }
    Ok(value)
}

fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

fn partial_pinyin(text: &str) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for item in text.to_pinyin() {
        let pinyin = item.ok_or_else(|| anyhow::anyhow!("无法转换命令短语中的汉字"))?;
        let syllable = pinyin.with_tone();
        let (initial, final_part) = split_initial(syllable);
        if !initial.is_empty() {
            result.push(initial.to_owned());
        }
        if !final_part.is_empty() {
            result.push(final_part.to_owned());
        }
    }
    Ok(result)
}

fn split_initial(syllable: &str) -> (&str, &str) {
    const INITIALS: [&str; 23] = [
        "zh", "ch", "sh", "b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h", "j", "q", "x",
        "r", "z", "c", "s", "y", "w",
    ];
    for initial in INITIALS {
        if let Some(final_part) = syllable.strip_prefix(initial) {
            return (&syllable[..initial.len()], final_part);
        }
    }
    ("", syllable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_expected_chinese_keyword_tokens() {
        let command = VoiceCommand::new("light-on", "小雨开灯", "好的");
        assert_eq!(
            command.keyword_lines().unwrap()[0],
            "x iǎo y ǔ k āi d ēng :1.50 #0.05 @小雨开灯"
        );

        let mut command = command;
        command.phrases.push("小雨打开灯".to_owned());
        assert_eq!(command.keyword_lines().unwrap().len(), 2);
    }

    #[test]
    fn rejects_duplicate_or_non_chinese_commands() {
        let mut config = VoiceConfig::default();
        config.commands[1].phrase = config.commands[0].phrase.clone();
        assert!(config.normalize().is_err());

        let mut config = VoiceConfig::default();
        config.commands[0].phrase = "hey小雨".to_owned();
        assert!(config.normalize().is_err());

        let mut config = VoiceConfig::default();
        let duplicate = config.commands[0].phrase.clone();
        config.commands[1].phrases.push(duplicate);
        assert!(config.normalize().is_err());
    }

    #[test]
    fn migrates_legacy_phrase_to_phrase_list() {
        let mut value =
            serde_json::to_value(VoiceCommand::new("light-on", "小雨开灯", "好的")).unwrap();
        value.as_object_mut().unwrap().remove("phrases");
        let command: VoiceCommand = serde_json::from_value(value).unwrap();
        let config = VoiceConfig {
            commands: vec![command],
            ..VoiceConfig::default()
        }
        .normalize()
        .unwrap();
        assert_eq!(config.commands[0].phrases, ["小雨开灯"]);
    }

    #[test]
    fn loads_default_playback_volume_from_legacy_config() {
        let mut value = serde_json::to_value(VoiceConfig::default()).unwrap();
        value.as_object_mut().unwrap().remove("playback_volume");
        let config: VoiceConfig = serde_json::from_value(value).unwrap();
        assert_eq!(config.playback_volume, 60);
    }

    #[test]
    fn validates_post_json_body() {
        let mut config = VoiceConfig::default();
        config.commands[0].method = "POST".to_owned();
        config.commands[0].body = "{invalid".to_owned();
        assert!(config.normalize().is_err());

        let mut config = VoiceConfig::default();
        config.commands[0].method = "POST".to_owned();
        config.commands[0].body = r#"{"enabled":true}"#.to_owned();
        assert!(config.normalize().is_ok());
    }

    #[test]
    fn supports_one_percent_threshold_precision() {
        let mut config = VoiceConfig::default();
        config.commands[0].trigger_threshold = 0.056;
        let config = config.normalize().unwrap();
        assert_eq!(config.commands[0].trigger_threshold, 0.056);

        let mut config = VoiceConfig::default();
        config.commands[0].trigger_threshold = 0.001;
        let config = config.normalize().unwrap();
        assert_eq!(config.commands[0].trigger_threshold, 0.01);
    }

    #[test]
    fn migrates_legacy_default_cooldowns_to_500_ms() {
        let mut config = VoiceConfig::default();
        config.version = 2;
        config.global_cooldown_ms = 2_000;
        config.commands[0].cooldown_ms = 2_000;
        config.commands[1].cooldown_ms = 5_000;
        config.commands[2].cooldown_ms = 10_000;

        let config = config.normalize().unwrap();

        assert_eq!(config.global_cooldown_ms, 500);
        assert_eq!(config.commands[0].cooldown_ms, 500);
        assert_eq!(config.commands[1].cooldown_ms, 500);
        assert_eq!(config.commands[2].cooldown_ms, 10_000);
    }

    #[test]
    fn expires_stale_or_future_test_requests() {
        let now = 1_000;
        let mut request = VoiceTestRequest {
            created_epoch: now,
            ..VoiceTestRequest::default()
        };
        assert!(request.is_fresh(now));

        request.created_epoch = now - VOICE_TEST_REQUEST_MAX_AGE_SECS - 1;
        assert!(!request.is_fresh(now));
        request.created_epoch = now + 6;
        assert!(!request.is_fresh(now));
    }
}
