use crate::config::Config;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub const DEFAULT_AI_MIN_PERSON_AREA_RATIO: f32 = 0.02;
pub const DEFAULT_AI_SNAPSHOT_MAX_COUNT: u64 = 500;
pub const DEFAULT_AI_SNAPSHOT_QUALITY: u8 = 95;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HubSettings {
    pub ai_enabled: bool,
    pub ai_interval_ms: u64,
    pub ai_threshold: f32,
    #[serde(default = "default_ai_min_person_area_ratio")]
    pub ai_min_person_area_ratio: f32,
    pub ai_min_snapshot_seconds: u64,
    #[serde(default = "default_ai_snapshot_max_count")]
    pub ai_snapshot_max_count: u64,
    #[serde(default = "default_ai_snapshot_quality")]
    pub ai_snapshot_quality: u8,
    pub segment_seconds: u64,
    pub max_bytes: u64,
    pub retain_days: u64,
    #[serde(default = "default_true")]
    pub record_enabled: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct HubSettingsPatch {
    pub ai_enabled: Option<bool>,
    pub ai_interval_ms: Option<u64>,
    pub ai_threshold: Option<f32>,
    pub ai_min_person_area_ratio: Option<f32>,
    pub ai_min_snapshot_seconds: Option<u64>,
    pub ai_snapshot_max_count: Option<u64>,
    pub ai_snapshot_quality: Option<u8>,
    pub segment_seconds: Option<u64>,
    pub max_bytes: Option<u64>,
    pub retain_days: Option<u64>,
    pub record_enabled: Option<bool>,
}

pub struct HubSettingsStore {
    path: PathBuf,
    current: RwLock<HubSettings>,
}

impl HubSettings {
    pub fn from_config(config: &Config) -> Self {
        Self {
            ai_enabled: config.ai_enabled,
            ai_interval_ms: config.ai_interval_ms,
            ai_threshold: config.ai_threshold,
            ai_min_person_area_ratio: config.ai_min_person_area_ratio,
            ai_min_snapshot_seconds: config.ai_min_snapshot_seconds,
            ai_snapshot_max_count: config.ai_snapshot_max_count,
            ai_snapshot_quality: config.ai_snapshot_quality,
            segment_seconds: config.segment_seconds,
            max_bytes: config.max_bytes,
            retain_days: config.retain_days,
            record_enabled: config.record_enabled,
        }
        .normalize()
    }

    fn normalize(mut self) -> Self {
        self.ai_interval_ms = self.ai_interval_ms.clamp(500, 60_000);
        self.ai_threshold = self.ai_threshold.clamp(0.05, 0.95);
        self.ai_min_person_area_ratio = if self.ai_min_person_area_ratio.is_finite() {
            self.ai_min_person_area_ratio.clamp(0.0, 1.0)
        } else {
            DEFAULT_AI_MIN_PERSON_AREA_RATIO
        };
        self.ai_min_snapshot_seconds = self.ai_min_snapshot_seconds.clamp(1, 3600);
        self.ai_snapshot_max_count = self.ai_snapshot_max_count.clamp(1, 100_000);
        self.ai_snapshot_quality = self.ai_snapshot_quality.clamp(1, 100);
        self.segment_seconds = self.segment_seconds.clamp(10, 3600);
        self.max_bytes = self
            .max_bytes
            .clamp(64 * 1024 * 1024, 512 * 1024 * 1024 * 1024);
        self.retain_days = self.retain_days.clamp(1, 365);
        self
    }

    fn apply(&mut self, patch: HubSettingsPatch) {
        if let Some(value) = patch.ai_enabled {
            self.ai_enabled = value;
        }
        if let Some(value) = patch.ai_interval_ms {
            self.ai_interval_ms = value;
        }
        if let Some(value) = patch.ai_threshold {
            self.ai_threshold = value;
        }
        if let Some(value) = patch.ai_min_person_area_ratio {
            self.ai_min_person_area_ratio = value;
        }
        if let Some(value) = patch.ai_min_snapshot_seconds {
            self.ai_min_snapshot_seconds = value;
        }
        if let Some(value) = patch.ai_snapshot_max_count {
            self.ai_snapshot_max_count = value;
        }
        if let Some(value) = patch.ai_snapshot_quality {
            self.ai_snapshot_quality = value;
        }
        if let Some(value) = patch.segment_seconds {
            self.segment_seconds = value;
        }
        if let Some(value) = patch.max_bytes {
            self.max_bytes = value;
        }
        if let Some(value) = patch.retain_days {
            self.retain_days = value;
        }
        if let Some(value) = patch.record_enabled {
            self.record_enabled = value;
        }
        *self = self.clone().normalize();
    }
}

fn default_true() -> bool {
    true
}

fn default_ai_min_person_area_ratio() -> f32 {
    DEFAULT_AI_MIN_PERSON_AREA_RATIO
}

fn default_ai_snapshot_max_count() -> u64 {
    DEFAULT_AI_SNAPSHOT_MAX_COUNT
}

fn default_ai_snapshot_quality() -> u8 {
    DEFAULT_AI_SNAPSHOT_QUALITY
}

impl HubSettingsStore {
    pub fn load(path: PathBuf, defaults: HubSettings) -> Result<Self> {
        let current = match fs::read(&path) {
            Ok(data) => serde_json::from_slice::<HubSettings>(&data)
                .with_context(|| format!("parse camera-hub settings {}", path.display()))?
                .normalize(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => defaults.normalize(),
            Err(error) => return Err(error.into()),
        };
        let store = Self {
            path,
            current: RwLock::new(current),
        };
        if !store.path.is_file() {
            store.save(&store.current())?;
        }
        Ok(store)
    }

    pub fn current(&self) -> HubSettings {
        self.current
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn update(&self, patch: HubSettingsPatch) -> Result<HubSettings> {
        let mut next = self.current();
        next.apply(patch);
        self.save(&next)?;
        *self
            .current
            .write()
            .unwrap_or_else(|error| error.into_inner()) = next.clone();
        Ok(next)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn save(&self, settings: &HubSettings) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.path.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(settings)?;
        fs::write(&temporary, data)?;
        fs::rename(&temporary, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn defaults() -> HubSettings {
        HubSettings {
            ai_enabled: true,
            ai_interval_ms: 1000,
            ai_threshold: 0.3,
            ai_min_person_area_ratio: DEFAULT_AI_MIN_PERSON_AREA_RATIO,
            ai_min_snapshot_seconds: 10,
            ai_snapshot_max_count: DEFAULT_AI_SNAPSHOT_MAX_COUNT,
            ai_snapshot_quality: DEFAULT_AI_SNAPSHOT_QUALITY,
            segment_seconds: 600,
            max_bytes: 8 * 1024 * 1024 * 1024,
            retain_days: 7,
            record_enabled: true,
        }
    }

    #[test]
    fn persists_and_clamps_runtime_settings() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "camera-hub-settings-{}-{nonce}",
            std::process::id()
        ));
        let path = root.join("camera-hub.json");
        let store = HubSettingsStore::load(path.clone(), defaults()).unwrap();
        let updated = store
            .update(HubSettingsPatch {
                ai_threshold: Some(2.0),
                ai_min_person_area_ratio: Some(2.0),
                ai_snapshot_max_count: Some(200_000),
                ai_snapshot_quality: Some(0),
                segment_seconds: Some(1),
                retain_days: Some(400),
                ..HubSettingsPatch::default()
            })
            .unwrap();
        assert_eq!(updated.ai_threshold, 0.95);
        assert_eq!(updated.ai_min_person_area_ratio, 1.0);
        assert_eq!(updated.ai_snapshot_max_count, 100_000);
        assert_eq!(updated.ai_snapshot_quality, 1);
        assert_eq!(updated.segment_seconds, 10);
        assert_eq!(updated.retain_days, 365);

        let loaded = HubSettingsStore::load(path, defaults()).unwrap().current();
        assert_eq!(loaded.ai_threshold, 0.95);
        assert_eq!(loaded.ai_min_person_area_ratio, 1.0);
        assert_eq!(loaded.ai_snapshot_max_count, 100_000);
        assert_eq!(loaded.ai_snapshot_quality, 1);
        assert_eq!(loaded.segment_seconds, 10);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loads_default_person_area_ratio_from_legacy_settings() {
        let value = serde_json::json!({
            "ai_enabled": true,
            "ai_interval_ms": 1000,
            "ai_threshold": 0.3,
            "ai_min_snapshot_seconds": 10,
            "segment_seconds": 600,
            "max_bytes": 8 * 1024 * 1024 * 1024_u64,
            "retain_days": 7,
            "record_enabled": true
        });

        let settings: HubSettings = serde_json::from_value(value).unwrap();

        assert_eq!(
            settings.ai_min_person_area_ratio,
            DEFAULT_AI_MIN_PERSON_AREA_RATIO
        );
        assert_eq!(
            settings.ai_snapshot_max_count,
            DEFAULT_AI_SNAPSHOT_MAX_COUNT
        );
        assert_eq!(settings.ai_snapshot_quality, DEFAULT_AI_SNAPSHOT_QUALITY);
    }
}
