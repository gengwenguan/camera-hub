use crate::frames::EncodedFrame;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const SESSION_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Debug, Serialize)]
pub struct BenchmarkAnchor {
    pub sequence: u32,
    pub pts_us: i64,
    pub capture_epoch_us: i64,
    pub source_clock: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_time_us: Option<i64>,
}

struct Session {
    updated: Instant,
    anchors: BTreeMap<String, BenchmarkAnchor>,
}

#[derive(Default)]
pub struct BenchmarkRegistry {
    sessions: Mutex<BTreeMap<String, Session>>,
}

impl BenchmarkRegistry {
    pub fn set_anchor(&self, session_id: &str, protocol: &str, frame: &EncodedFrame) {
        self.set_anchor_value(
            session_id,
            protocol,
            BenchmarkAnchor {
                sequence: frame.sequence,
                pts_us: frame.pts_us,
                capture_epoch_us: frame.capture_epoch_us,
                source_clock: frame.source_clock,
                media_time_us: None,
            },
        );
    }

    pub fn set_anchor_value(&self, session_id: &str, protocol: &str, anchor: BenchmarkAnchor) {
        if !valid_session_id(session_id) {
            return;
        }
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        sessions.retain(|_, session| session.updated.elapsed() < SESSION_TTL);
        let session = sessions
            .entry(session_id.to_owned())
            .or_insert_with(|| Session {
                updated: Instant::now(),
                anchors: BTreeMap::new(),
            });
        session.updated = Instant::now();
        session.anchors.entry(protocol.to_owned()).or_insert(anchor);
    }

    pub fn anchors(&self, session_id: &str) -> BTreeMap<String, BenchmarkAnchor> {
        if !valid_session_id(session_id) {
            return BTreeMap::new();
        }
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        sessions.retain(|_, session| session.updated.elapsed() < SESSION_TTL);
        sessions
            .get_mut(session_id)
            .map(|session| {
                session.updated = Instant::now();
                session.anchors.clone()
            })
            .unwrap_or_default()
    }

    pub fn remove(&self, session_id: &str) {
        self.sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(session_id);
    }
}

pub fn valid_session_id(value: &str) -> bool {
    (8..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn records_only_the_first_protocol_anchor() {
        let registry = BenchmarkRegistry::default();
        let frame = |sequence, pts_us| EncodedFrame {
            sequence,
            pts_us,
            capture_epoch_us: pts_us + 100,
            received_epoch_us: pts_us + 200,
            source_clock: true,
            key: true,
            data: Arc::from([1u8]),
        };
        registry.set_anchor("session-123", "flv", &frame(1, 1_000));
        registry.set_anchor("session-123", "flv", &frame(2, 2_000));
        let anchors = registry.anchors("session-123");
        assert_eq!(anchors["flv"].sequence, 1);
        registry.remove("session-123");
        assert!(registry.anchors("session-123").is_empty());
    }
}
