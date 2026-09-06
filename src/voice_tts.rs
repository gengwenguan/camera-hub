#![allow(dead_code)]

use anyhow::{Result, bail};
use base64::Engine;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::time::Duration;

pub const DEFAULT_VOICE_PROFILE_ID: &str = "camera-hub-default";
pub const VOICE_REFERENCE_PROMPT: &str =
    "你好，我正在为智能语音助手录制声音样本。今天天气不错，希望接下来的语音自然、清晰、稳定。";
pub const MAX_REFERENCE_WAV_BYTES: usize = 1024 * 1024;
pub const MAX_SYNTHESIS_TEXT_CHARS: usize = 120;
pub const MAX_SYNTHESIZED_WAV_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_GENERATION_MILLIS: u64 = 120_000;
const MAX_TTS_ERROR_BYTES: usize = 16 * 1024;
const MAX_TTS_JSON_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize, Serialize)]
pub struct VoiceReferenceUpload {
    pub audio_base64: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TtsEnrollRequest {
    pub audio_base64: String,
    pub transcript: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TtsProfileResponse {
    pub profile_id: String,
    pub fingerprint: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TtsSynthesizeRequest {
    pub profile_id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_generation_ms: Option<u64>,
}

#[derive(Clone)]
pub struct VoiceTtsClient {
    client: Client,
    base_url: String,
    token: String,
}

impl VoiceTtsClient {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let parsed = reqwest::Url::parse(base_url)?;
        if !matches!(parsed.scheme(), "http" | "https") {
            bail!("TTS URL 必须使用 http 或 https");
        }
        let loopback = parsed.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        });
        if parsed.scheme() == "http" && !loopback {
            bail!("非本机 TTS 服务必须使用 HTTPS");
        }
        let base_url = base_url.trim_end_matches('/').to_owned();
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(300))
                .build()?,
            base_url,
            token: token.to_owned(),
        })
    }

    pub async fn enroll(
        &self,
        profile_id: &str,
        wav: &[u8],
        transcript: &str,
    ) -> Result<TtsProfileResponse> {
        if !valid_profile_id(profile_id) {
            bail!("声纹 profile ID 无效");
        }
        validate_reference_wav(wav)?;
        let request = TtsEnrollRequest {
            audio_base64: base64::engine::general_purpose::STANDARD.encode(wav),
            transcript: transcript.to_owned(),
        };
        let response = self
            .authorize(
                self.client
                    .put(format!("{}/v1/profiles/{profile_id}", self.base_url)),
            )
            .json(&request)
            .send()
            .await?;
        parse_json_response(response).await
    }

    pub async fn delete_profile(&self, profile_id: &str) -> Result<()> {
        if !valid_profile_id(profile_id) {
            bail!("声纹 profile ID 无效");
        }
        let response = self
            .authorize(
                self.client
                    .delete(format!("{}/v1/profiles/{profile_id}", self.base_url)),
            )
            .send()
            .await?;
        ensure_success(response).await?;
        Ok(())
    }

    pub async fn synthesize(&self, profile_id: &str, text: &str) -> Result<Vec<u8>> {
        self.synthesize_with_timeout(
            profile_id,
            text,
            Duration::from_millis(MAX_GENERATION_MILLIS),
        )
        .await
    }

    pub async fn synthesize_with_timeout(
        &self,
        profile_id: &str,
        text: &str,
        max_generation: Duration,
    ) -> Result<Vec<u8>> {
        let text = validate_synthesis_text(text)?;
        if !valid_profile_id(profile_id) {
            bail!("声纹 profile ID 无效");
        }
        let max_generation_ms = u64::try_from(max_generation.as_millis())
            .unwrap_or(u64::MAX)
            .clamp(1_000, MAX_GENERATION_MILLIS);
        let response = self
            .authorize(self.client.post(format!("{}/v1/synthesize", self.base_url)))
            .json(&TtsSynthesizeRequest {
                profile_id: profile_id.to_owned(),
                text,
                max_generation_ms: Some(max_generation_ms),
            })
            .send()
            .await?;
        let response = ensure_success(response).await?;
        read_limited(
            response,
            MAX_SYNTHESIZED_WAV_BYTES as usize,
            "TTS 返回的 WAV 过大",
        )
        .await
    }

    fn authorize(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.token.is_empty() {
            request
        } else {
            request.bearer_auth(&self.token)
        }
    }
}

async fn ensure_success(response: reqwest::Response) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let message = read_limited(response, MAX_TTS_ERROR_BYTES, "TTS 错误响应过大")
        .await
        .map(|data| String::from_utf8_lossy(&data).into_owned())
        .unwrap_or_else(|_| "无法读取 TTS 错误响应".to_owned());
    bail!("TTS HTTP {status}: {message}");
}

async fn parse_json_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T> {
    let response = ensure_success(response).await?;
    let data = read_limited(response, MAX_TTS_JSON_BYTES, "TTS JSON 响应过大").await?;
    Ok(serde_json::from_slice(&data)?)
}

async fn read_limited(
    response: reqwest::Response,
    limit: usize,
    too_large: &'static str,
) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        bail!(too_large);
    }
    let mut body = response.bytes_stream();
    let mut data = Vec::new();
    while let Some(chunk) = body.next().await {
        let chunk = chunk?;
        if data.len().saturating_add(chunk.len()) > limit {
            bail!(too_large);
        }
        data.extend_from_slice(&chunk);
    }
    Ok(data)
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VoiceStudioSessionResponse {
    pub session_token: String,
    pub reference_prompt: String,
    pub expires_epoch: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VoiceStudioReferenceRequest {
    pub session_token: String,
    pub audio_base64: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VoiceStudioSynthesizeRequest {
    pub session_token: String,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WavInfo {
    pub sample_rate: u32,
    pub sample_count: u32,
}

impl WavInfo {
    pub fn duration_seconds(self) -> f64 {
        f64::from(self.sample_count) / f64::from(self.sample_rate)
    }
}

pub fn validate_reference_wav(data: &[u8]) -> Result<WavInfo> {
    if data.len() > MAX_REFERENCE_WAV_BYTES {
        bail!("参考录音不能超过 1 MiB");
    }
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        bail!("参考录音必须是 WAV 文件");
    }

    let mut offset = 12usize;
    let mut format = None;
    let mut data_size = None;
    while offset.checked_add(8).is_some_and(|end| end <= data.len()) {
        let chunk_id = &data[offset..offset + 4];
        let chunk_size =
            u32::from_le_bytes(data[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let start = offset + 8;
        let Some(end) = start.checked_add(chunk_size) else {
            bail!("WAV 数据块长度无效");
        };
        if end > data.len() {
            bail!("WAV 数据块不完整");
        }
        if chunk_id == b"fmt " {
            if chunk_size < 16 {
                bail!("WAV fmt 数据块无效");
            }
            let audio_format = u16::from_le_bytes(data[start..start + 2].try_into().unwrap());
            let channels = u16::from_le_bytes(data[start + 2..start + 4].try_into().unwrap());
            let sample_rate = u32::from_le_bytes(data[start + 4..start + 8].try_into().unwrap());
            let bits_per_sample =
                u16::from_le_bytes(data[start + 14..start + 16].try_into().unwrap());
            format = Some((audio_format, channels, sample_rate, bits_per_sample));
        } else if chunk_id == b"data" {
            data_size = Some(chunk_size);
        }
        offset = end + (chunk_size & 1);
    }

    let Some((audio_format, channels, sample_rate, bits_per_sample)) = format else {
        bail!("WAV 缺少 fmt 数据块");
    };
    if audio_format != 1 || channels != 1 || bits_per_sample != 16 {
        bail!("参考录音必须是单声道 16-bit PCM WAV");
    }
    if sample_rate != 24_000 {
        bail!("参考录音采样率必须是 24 kHz");
    }
    let data_size = data_size.ok_or_else(|| anyhow::anyhow!("WAV 缺少音频数据"))?;
    let sample_count =
        u32::try_from(data_size / usize::from(channels) / usize::from(bits_per_sample / 8))?;
    let info = WavInfo {
        sample_rate,
        sample_count,
    };
    if !(3.0..=20.0).contains(&info.duration_seconds()) {
        bail!("参考录音时长必须在 3–20 秒之间");
    }
    Ok(info)
}

pub fn decode_reference_audio(value: &str) -> Result<Vec<u8>> {
    let estimated_size = value.len().saturating_mul(3) / 4;
    if estimated_size > MAX_REFERENCE_WAV_BYTES {
        bail!("参考录音不能超过 1 MiB");
    }
    let data = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| anyhow::anyhow!("参考录音 Base64 无效"))?;
    validate_reference_wav(&data)?;
    Ok(data)
}

pub fn validate_synthesis_text(text: &str) -> Result<String> {
    let text = text.trim();
    let count = text.chars().count();
    if count == 0 || count > MAX_SYNTHESIS_TEXT_CHARS || text.chars().any(char::is_control) {
        bail!("合成文本不能为空、不能换行且最多 120 个字符");
    }
    Ok(text.to_owned())
}

pub fn valid_profile_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pcm_wav(seconds: u32) -> Vec<u8> {
        let sample_rate = 24_000u32;
        let data_len = sample_rate * seconds * 2;
        let mut wav = Vec::with_capacity(44 + data_len as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.resize(44 + data_len as usize, 0);
        wav
    }

    #[test]
    fn validates_reference_wav_shape_and_duration() {
        let info = validate_reference_wav(&pcm_wav(5)).unwrap();
        assert_eq!(info.sample_rate, 24_000);
        assert_eq!(info.duration_seconds(), 5.0);
        assert!(validate_reference_wav(&pcm_wav(2)).is_err());
        assert!(validate_reference_wav(b"not-wave").is_err());
    }

    #[test]
    fn validates_profile_ids_and_synthesis_text() {
        assert!(valid_profile_id("public-123_ab"));
        assert!(!valid_profile_id("../profile"));
        assert_eq!(validate_synthesis_text(" 你好 ").unwrap(), "你好");
        assert!(validate_synthesis_text("hello\nworld").is_err());
        assert!(validate_synthesis_text("hello\0world").is_err());
    }

    #[test]
    fn requires_https_for_remote_tts_service() {
        assert!(VoiceTtsClient::new("http://127.0.0.1:39081", "").is_ok());
        assert!(VoiceTtsClient::new("http://192.0.2.1:39081", "secret").is_err());
        assert!(VoiceTtsClient::new("https://tts.example.com", "secret").is_ok());
    }
}
