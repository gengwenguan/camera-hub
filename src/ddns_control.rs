use anyhow::Result;
use camera_hub::ddns::{
    DdnsConfigStore, DdnsConfigUpdate, DdnsPreview, DdnsPreviewRequest, DdnsPublicConfig,
    DdnsStatus, epoch_seconds, load_status, preview_request,
};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const STATUS_STALE_SECONDS: u64 = 90;

#[derive(Clone, Debug, Serialize)]
pub struct DdnsOverview {
    pub config: DdnsPublicConfig,
    pub status: DdnsStatus,
    pub worker_online: bool,
    pub config_path: PathBuf,
    pub status_path: PathBuf,
}

pub struct DdnsControl {
    store: Mutex<DdnsConfigStore>,
    status_path: PathBuf,
}

impl DdnsControl {
    pub fn load(config_path: PathBuf, status_path: PathBuf) -> Result<Self> {
        Ok(Self {
            store: Mutex::new(DdnsConfigStore::load(config_path)?),
            status_path,
        })
    }

    pub fn overview(&self) -> DdnsOverview {
        let store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        let status = load_status(&self.status_path).unwrap_or_default();
        let worker_online = status.updated_epoch > 0
            && epoch_seconds().saturating_sub(status.updated_epoch) <= STATUS_STALE_SECONDS;
        DdnsOverview {
            config: store.current().public(),
            status,
            worker_online,
            config_path: store.path().to_path_buf(),
            status_path: self.status_path.clone(),
        }
    }

    pub fn update(&self, update: DdnsConfigUpdate) -> Result<DdnsPublicConfig> {
        self.store
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .update(update)
    }

    pub fn preview(&self, request: DdnsPreviewRequest) -> Result<DdnsPreview> {
        preview_request(request)
    }

    pub fn request_reconcile(&self) -> Result<u64> {
        self.store
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .request_reconcile()
    }

    pub fn status_path(&self) -> &Path {
        &self.status_path
    }
}
