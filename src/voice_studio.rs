use crate::config::Config;
use crate::voice_tts::{
    TtsProfileResponse, VOICE_REFERENCE_PROMPT, VoiceStudioReferenceRequest,
    VoiceStudioSessionResponse, VoiceStudioSynthesizeRequest, VoiceTtsClient,
    decode_reference_audio, validate_synthesis_text,
};
use anyhow::Result;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;
use tracing::error;

const SESSION_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_PUBLIC_OPERATIONS: usize = 1;
type HmacSha256 = Hmac<Sha256>;

pub struct VoiceStudio {
    tts: VoiceTtsClient,
    session_key: [u8; 32],
    capacity: Arc<Semaphore>,
}

impl VoiceStudio {
    pub fn new(config: &Config) -> Result<Self> {
        let mut session_key = [0_u8; 32];
        if config.tts_token.is_empty() {
            getrandom::fill(&mut session_key)?;
        } else {
            session_key.copy_from_slice(
                &Sha256::digest(format!(
                    "camera-hub-voice-studio:{}:{}",
                    config.tts_url, config.tts_token
                ))[..],
            );
        }
        Ok(Self {
            tts: VoiceTtsClient::new(&config.tts_url, &config.tts_token)?,
            session_key,
            capacity: Arc::new(Semaphore::new(MAX_PUBLIC_OPERATIONS)),
        })
    }

    pub fn create_session(&self) -> Result<VoiceStudioSessionResponse, VoiceStudioError> {
        let nonce = random_nonce().map_err(VoiceStudioError::internal)?;
        let expires_epoch = epoch_seconds().saturating_add(SESSION_TTL.as_secs());
        let payload = format!("{nonce}.{expires_epoch}");
        let token = format!("{payload}.{}", self.sign(&payload));
        Ok(VoiceStudioSessionResponse {
            session_token: token,
            reference_prompt: VOICE_REFERENCE_PROMPT.to_owned(),
            expires_epoch,
        })
    }

    pub async fn enroll(
        &self,
        request: VoiceStudioReferenceRequest,
    ) -> Result<TtsProfileResponse, VoiceStudioError> {
        let _permit = self
            .capacity
            .try_acquire()
            .map_err(|_| VoiceStudioError::busy())?;
        let wav = decode_reference_audio(&request.audio_base64)
            .map_err(VoiceStudioError::bad_request_error)?;
        let profile_id = self.session_profile(&request.session_token)?;
        self.tts
            .enroll(&profile_id, &wav, VOICE_REFERENCE_PROMPT)
            .await
            .map_err(VoiceStudioError::service_unavailable)
    }

    pub async fn synthesize(
        &self,
        request: VoiceStudioSynthesizeRequest,
    ) -> Result<Vec<u8>, VoiceStudioError> {
        let _permit = self
            .capacity
            .try_acquire()
            .map_err(|_| VoiceStudioError::busy())?;
        let profile_id = self.session_profile(&request.session_token)?;
        let text =
            validate_synthesis_text(&request.text).map_err(VoiceStudioError::bad_request_error)?;
        self.tts
            .synthesize(&profile_id, &text)
            .await
            .map_err(VoiceStudioError::service_unavailable)
    }

    fn session_profile(&self, token: &str) -> Result<String, VoiceStudioError> {
        let mut parts = token.split('.');
        let (Some(nonce), Some(expires), Some(signature), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(VoiceStudioError::unauthorized("声纹会话无效"));
        };
        if nonce.len() != 32 || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(VoiceStudioError::unauthorized("声纹会话无效"));
        }
        let expires_epoch = expires
            .parse::<u64>()
            .map_err(|_| VoiceStudioError::unauthorized("声纹会话无效"))?;
        if epoch_seconds() >= expires_epoch {
            return Err(VoiceStudioError::unauthorized("声纹会话已过期"));
        }
        let signature =
            hex::decode(signature).map_err(|_| VoiceStudioError::unauthorized("声纹会话无效"))?;
        let payload = format!("{nonce}.{expires}");
        let mut mac = HmacSha256::new_from_slice(&self.session_key).expect("valid HMAC key");
        mac.update(payload.as_bytes());
        if mac.verify_slice(&signature).is_err() {
            return Err(VoiceStudioError::unauthorized("声纹会话无效"));
        }
        Ok(format!("public-{}", nonce.to_ascii_lowercase()))
    }

    fn sign(&self, payload: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.session_key).expect("valid HMAC key");
        mac.update(payload.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }
}

fn random_nonce() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)?;
    Ok(hex::encode(bytes))
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug)]
pub struct VoiceStudioError {
    status: StatusCode,
    message: String,
}

impl VoiceStudioError {
    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn bad_request_error(error: anyhow::Error) -> Self {
        Self::bad_request(format!("{error:#}"))
    }

    fn busy() -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: "当前请求较多，请稍后重试".to_owned(),
        }
    }

    fn service_unavailable(error: anyhow::Error) -> Self {
        error!(%error, "public voice studio TTS request failed");
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: "语音生成服务暂不可用，请稍后重试".to_owned(),
        }
    }

    fn internal(error: anyhow::Error) -> Self {
        error!(%error, "public voice studio internal error");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "服务内部错误".to_owned(),
        }
    }
}

impl IntoResponse for VoiceStudioError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"ok":false,"error":self.message}))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn studio() -> VoiceStudio {
        VoiceStudio {
            tts: VoiceTtsClient::new("http://127.0.0.1:39081", "").unwrap(),
            session_key: [7; 32],
            capacity: Arc::new(Semaphore::new(MAX_PUBLIC_OPERATIONS)),
        }
    }

    #[test]
    fn creates_random_session_nonces() {
        let first = random_nonce().unwrap();
        let second = random_nonce().unwrap();
        assert_eq!(first.len(), 32);
        assert_ne!(first, second);
    }

    #[test]
    fn validates_signed_stateless_sessions() {
        let studio = studio();
        let session = studio.create_session().unwrap();
        let profile = studio.session_profile(&session.session_token).unwrap();
        assert!(profile.starts_with("public-"));

        let mut tampered = session.session_token;
        let replacement = if tampered.starts_with('f') { "e" } else { "f" };
        tampered.replace_range(0..1, replacement);
        assert!(studio.session_profile(&tampered).is_err());
    }
}
