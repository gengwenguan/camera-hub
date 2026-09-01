use crate::ai::AiService;
use crate::benchmark::BenchmarkRegistry;
use crate::config::Config;
use crate::flv_live::FlvLive;
use crate::frames::FrameHub;
use crate::live::LiveStreams;
use crate::media::MediaStore;
use crate::moq_live::MoqLive;
use crate::mux::MediaMuxers;
use crate::qq::QqService;
use crate::settings::HubSettingsStore;
use crate::system::SystemMonitor;
use crate::voice::VoiceService;
use crate::webrtc_live::WebRtcRelay;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

const DEVICE_ONLINE_TIMEOUT: Duration = Duration::from_secs(35);

#[derive(Clone, Debug, Default, Deserialize)]
pub struct DeviceHeartbeat {
    #[serde(default)]
    pub firmware: String,
    #[serde(default)]
    pub ipv6: String,
}

struct DeviceEntry {
    heartbeat: DeviceHeartbeat,
    remote: SocketAddr,
    first_seen: SystemTime,
    last_seen: Instant,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeviceSnapshot {
    pub device_id: String,
    pub online: bool,
    pub age_seconds: u64,
    pub first_seen_epoch: u64,
    pub remote: String,
    pub firmware: String,
    pub ipv6: String,
}

pub struct AppState {
    pub config: Config,
    pub benchmark: Arc<BenchmarkRegistry>,
    pub media: Arc<MediaStore>,
    pub flv: Arc<FlvLive>,
    pub live: Arc<LiveStreams>,
    pub frames: Arc<FrameHub>,
    pub muxers: Arc<MediaMuxers>,
    pub moq: Arc<MoqLive>,
    pub qq: Arc<QqService>,
    pub ai: Arc<AiService>,
    pub settings: Arc<HubSettingsStore>,
    pub voice: Arc<VoiceService>,
    pub webrtc: Arc<WebRtcRelay>,
    pub system: Arc<SystemMonitor>,
    devices: RwLock<BTreeMap<String, DeviceEntry>>,
    links: RwLock<BTreeMap<String, u64>>,
    next_link: AtomicU64,
    started: Instant,
}

impl AppState {
    pub fn new(
        config: Config,
        settings: Arc<HubSettingsStore>,
        media: Arc<MediaStore>,
        ai: Arc<AiService>,
        qq: Arc<QqService>,
        voice: Arc<VoiceService>,
        frames: Arc<FrameHub>,
    ) -> Self {
        let system = Arc::new(SystemMonitor::new(config.data_dir.clone()));
        let benchmark = Arc::new(BenchmarkRegistry::default());
        let live = Arc::new(LiveStreams::default());
        let moq = MoqLive::start(&config, frames.clone());
        Self {
            config,
            benchmark: benchmark.clone(),
            media: media.clone(),
            flv: Arc::new(FlvLive::new(benchmark.clone())),
            live: live.clone(),
            webrtc: Arc::new(WebRtcRelay::new(frames.clone(), benchmark.clone())),
            muxers: Arc::new(MediaMuxers::new(frames.clone(), media, live)),
            moq,
            qq,
            frames,
            ai,
            settings,
            voice,
            system,
            devices: RwLock::new(BTreeMap::new()),
            links: RwLock::new(BTreeMap::new()),
            next_link: AtomicU64::new(1),
            started: Instant::now(),
        }
    }

    pub async fn begin_link(&self, device_id: &str) -> u64 {
        let generation = self.next_link.fetch_add(1, Ordering::Relaxed);
        self.links
            .write()
            .await
            .insert(device_id.to_owned(), generation);
        generation
    }

    pub async fn link_is_current(&self, device_id: &str, generation: u64) -> bool {
        self.links.read().await.get(device_id) == Some(&generation)
    }

    pub async fn heartbeat(&self, device_id: &str, heartbeat: DeviceHeartbeat, remote: SocketAddr) {
        let mut devices = self.devices.write().await;
        match devices.get_mut(device_id) {
            Some(entry) => {
                entry.heartbeat = heartbeat;
                entry.remote = remote;
                entry.last_seen = Instant::now();
            }
            None => {
                devices.insert(
                    device_id.to_owned(),
                    DeviceEntry {
                        heartbeat,
                        remote,
                        first_seen: SystemTime::now(),
                        last_seen: Instant::now(),
                    },
                );
            }
        }
    }

    pub async fn devices(&self) -> Vec<DeviceSnapshot> {
        self.devices
            .read()
            .await
            .iter()
            .map(|(device_id, entry)| {
                let age = entry.last_seen.elapsed();
                DeviceSnapshot {
                    device_id: device_id.clone(),
                    online: age <= DEVICE_ONLINE_TIMEOUT,
                    age_seconds: age.as_secs(),
                    first_seen_epoch: entry
                        .first_seen
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    remote: entry.remote.to_string(),
                    firmware: entry.heartbeat.firmware.clone(),
                    ipv6: entry.heartbeat.ipv6.clone(),
                }
            })
            .collect()
    }

    pub fn uptime_seconds(&self) -> u64 {
        self.started.elapsed().as_secs()
    }
}
