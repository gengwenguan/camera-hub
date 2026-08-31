use anyhow::{Result, bail};
use pinyin::ToPinyin;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const VOICE_CONFIG_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct VoiceConfig {
    pub version: u32,
    pub revision: u64,
    pub enabled: bool,
    pub capture_device: String,
    pub playback_device: String,
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
    pub last_error: String,
    pub updated_epoch: u64,
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

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            version: VOICE_CONFIG_VERSION,
            revision: 1,
            enabled: false,
            capture_device: "hw:0,0".to_owned(),
            playback_device: "plughw:0,0".to_owned(),
            capture_rate: 48_000,
            request_timeout_ms: 3_000,
            global_cooldown_ms: 2_000,
            failure_reply: "操作失败，请稍后再试".to_owned(),
            commands: vec![
                VoiceCommand::new("light-on", "小雨开灯", "好的，已经开灯"),
                VoiceCommand::new("light-off", "小雨关灯", "好的，已经关灯"),
                VoiceCommand {
                    trigger_threshold: 0.60,
                    cooldown_ms: 5_000,
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
            reply: reply.to_owned(),
            method: "GET".to_owned(),
            url: String::new(),
            body: String::new(),
            boosting_score: 1.5,
            trigger_threshold: 0.45,
            cooldown_ms: 2_000,
        }
    }
}

impl VoiceConfig {
    pub fn normalize(mut self) -> Result<Self> {
        self.version = VOICE_CONFIG_VERSION;
        self.capture_device = self.capture_device.trim().to_owned();
        self.playback_device = self.playback_device.trim().to_owned();
        self.capture_rate = self.capture_rate.clamp(8_000, 192_000);
        self.request_timeout_ms = self.request_timeout_ms.clamp(500, 30_000);
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
            command.phrase = clean_phrase(&command.phrase)?;
            if !phrases.insert(command.phrase.clone()) {
                bail!("命令短语不能重复：{}", command.phrase);
            }
            command.reply = clean_text(&command.reply, 120, "回复内容")?;
            command.method = command.method.trim().to_ascii_uppercase();
            if !matches!(command.method.as_str(), "GET" | "POST") {
                bail!("命令 {} 仅支持 GET 或 POST", command.phrase);
            }
            command.url = command.url.trim().to_owned();
            if !command.url.is_empty()
                && !(command.url.starts_with("http://") || command.url.starts_with("https://"))
            {
                bail!("命令 {} 的 URL 必须使用 http 或 https", command.phrase);
            }
            if command.url.len() > 2048 || command.body.len() > 8192 {
                bail!("命令 {} 的 URL 或请求体过长", command.phrase);
            }
            command.boosting_score = finite_or(command.boosting_score, 1.5).clamp(0.0, 10.0);
            command.trigger_threshold =
                finite_or(command.trigger_threshold, 0.45).clamp(0.05, 0.95);
            command.cooldown_ms = command.cooldown_ms.clamp(500, 60_000);
        }
        Ok(self)
    }

    pub fn enabled_commands(&self) -> impl Iterator<Item = &VoiceCommand> {
        self.commands
            .iter()
            .filter(|command| command.enabled && !command.url.is_empty())
    }

    pub fn keyword_buffer(&self) -> Result<String> {
        let lines = self
            .enabled_commands()
            .map(VoiceCommand::keyword_line)
            .collect::<Result<Vec<_>>>()?;
        if lines.is_empty() {
            bail!("没有已启用且配置 URL 的语音命令");
        }
        Ok(lines.join("\n"))
    }
}

impl VoiceCommand {
    pub fn keyword_line(&self) -> Result<String> {
        let tokens = partial_pinyin(&self.phrase)?;
        Ok(format!(
            "{} :{:.2} #{:.2} @{}",
            tokens.join(" "),
            self.boosting_score,
            self.trigger_threshold,
            self.phrase.replace(' ', "_")
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
    if value.is_empty()
        || value.chars().count() > max_chars
        || value.contains('\r')
        || value.contains('\n')
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
            command.keyword_line().unwrap(),
            "x iǎo y ǔ k āi d ēng :1.50 #0.45 @小雨开灯"
        );
    }

    #[test]
    fn rejects_duplicate_or_non_chinese_commands() {
        let mut config = VoiceConfig::default();
        config.commands[1].phrase = config.commands[0].phrase.clone();
        assert!(config.normalize().is_err());

        let mut config = VoiceConfig::default();
        config.commands[0].phrase = "hey小雨".to_owned();
        assert!(config.normalize().is_err());
    }
}
