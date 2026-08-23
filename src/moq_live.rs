use crate::auth::constant_time_eq;
use crate::config::Config;
use crate::frames::{EncodedFrame, FrameHub, FrameSubscription};
use anyhow::{Context, Result};
use base64::Engine as _;
use bytes::Bytes;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

// Keep audio groups short so a delayed QUIC stream never holds a full second of
// sound. AAC frames are independent; ~200ms groups let the subscriber skip or
// prioritize audio without accumulating seconds of A/V drift.
const AUDIO_GROUP_US: i64 = 200_000;
const AAC_FRAME_US: i64 = 21_333;
const VIDEO_TRACK: &str = "video";
const AUDIO_TRACK: &str = "audio";
const AAC_AUDIO_SPECIFIC_CONFIG: &[u8] = &[0x11, 0x88];
const MSF_CATALOG_FORMAT: &str = "msf-draft-01";
const LOC_CONTAINER_FORMAT: &str = "loc-draft-04";

#[derive(Clone, Debug, Serialize)]
pub struct MoqStatus {
    pub enabled: bool,
    pub running: bool,
    pub bind: String,
    pub active_publishers: usize,
    pub video_frames: u64,
    pub audio_frames: u64,
    pub fingerprints: Vec<String>,
    pub auth_token: String,
    pub catalog_format: &'static str,
    pub container_format: &'static str,
    pub last_error: String,
}

struct SharedStatus {
    running: AtomicBool,
    video_frames: AtomicU64,
    audio_frames: AtomicU64,
    last_error: Mutex<String>,
}

impl Default for SharedStatus {
    fn default() -> Self {
        Self {
            running: AtomicBool::new(false),
            video_frames: AtomicU64::new(0),
            audio_frames: AtomicU64::new(0),
            last_error: Mutex::new(String::new()),
        }
    }
}

pub struct MoqLive {
    enabled: bool,
    bind: String,
    frames: Arc<FrameHub>,
    origin: Option<moq_net::origin::Producer>,
    certificates: Option<moq_native::tls::Certificates>,
    auth_token: String,
    workers: AsyncMutex<BTreeMap<String, JoinHandle<()>>>,
    status: Arc<SharedStatus>,
}

impl MoqLive {
    pub fn start(config: &Config, frames: Arc<FrameHub>) -> Arc<Self> {
        let status = Arc::new(SharedStatus::default());
        if !config.moq_enabled {
            return Arc::new(Self {
                enabled: false,
                bind: config.moq_bind.to_string(),
                frames,
                origin: None,
                certificates: None,
                auth_token: String::new(),
                workers: AsyncMutex::new(BTreeMap::new()),
                status,
            });
        }

        let origin = moq_net::Origin::random().produce();
        let mut server_config = moq_native::ServerConfig::default();
        server_config.bind = Some(config.moq_bind.to_string());
        server_config.tls.cert = vec![config.tls_cert.clone()];
        server_config.tls.key = vec![config.tls_key.clone()];

        let (certificates, server) = match server_config.init() {
            Ok(server) => (Some(server.certificates()), Some(server)),
            Err(error) => {
                *status
                    .last_error
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = format!("{error:#}");
                (None, None)
            }
        };

        if let Some(server) = server {
            let consumer = origin.consume();
            let task_status = status.clone();
            let bind = config.moq_bind;
            let auth_token = config.moq_auth_token.clone();
            status.running.store(true, Ordering::Release);
            tokio::spawn(async move {
                info!(%bind, "camera-hub MoQ/WebTransport started");
                if let Err(error) = serve_authorized(server, consumer, auth_token).await {
                    let message = format!("{error:#}");
                    error!(error = %message, "camera-hub MoQ/WebTransport stopped");
                    *task_status
                        .last_error
                        .lock()
                        .unwrap_or_else(|error| error.into_inner()) = message;
                }
                task_status.running.store(false, Ordering::Release);
            });
        }

        Arc::new(Self {
            enabled: true,
            bind: config.moq_bind.to_string(),
            frames,
            origin: Some(origin),
            certificates,
            auth_token: config.moq_auth_token.clone(),
            workers: AsyncMutex::new(BTreeMap::new()),
            status,
        })
    }

    pub async fn ensure(&self, device_id: &str) {
        if !self.status.running.load(Ordering::Acquire) {
            return;
        }
        let Some(origin) = self.origin.clone() else {
            return;
        };
        let mut workers = self.workers.lock().await;
        if workers
            .get(device_id)
            .is_some_and(|worker| !worker.is_finished())
        {
            return;
        }
        if let Some(worker) = workers.remove(device_id) {
            worker.abort();
        }
        let Some(subscription) = self.frames.subscribe(device_id) else {
            return;
        };
        let id = device_id.to_owned();
        let worker_id = id.clone();
        let status = self.status.clone();
        workers.insert(
            id,
            tokio::spawn(async move {
                if let Err(error) =
                    publish_device(&worker_id, origin, subscription, status.clone()).await
                {
                    let message = format!("{error:#}");
                    error!(device_id = %worker_id, error = %message, "MoQ publisher stopped");
                    *status
                        .last_error
                        .lock()
                        .unwrap_or_else(|error| error.into_inner()) = message;
                }
            }),
        );
    }

    pub async fn status(&self) -> MoqStatus {
        let active_publishers = self
            .workers
            .lock()
            .await
            .values()
            .filter(|worker| !worker.is_finished())
            .count();
        MoqStatus {
            enabled: self.enabled,
            running: self.status.running.load(Ordering::Acquire),
            bind: self.bind.clone(),
            active_publishers,
            video_frames: self.status.video_frames.load(Ordering::Relaxed),
            audio_frames: self.status.audio_frames.load(Ordering::Relaxed),
            fingerprints: self
                .certificates
                .as_ref()
                .map(moq_native::tls::Certificates::fingerprints)
                .unwrap_or_default(),
            auth_token: self.auth_token.clone(),
            catalog_format: MSF_CATALOG_FORMAT,
            container_format: LOC_CONTAINER_FORMAT,
            last_error: self
                .status
                .last_error
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone(),
        }
    }
}

async fn serve_authorized(
    mut server: moq_native::Server,
    origin: moq_net::origin::Consumer,
    auth_token: String,
) -> Result<()> {
    while let Some(request) = server.accept().await {
        let authorized = request.url().is_some_and(|url| {
            url.query_pairs().any(|(key, value)| {
                key == "token" && constant_time_eq(value.as_bytes(), auth_token.as_bytes())
            })
        });
        if !authorized {
            warn!("rejected unauthorized MoQ/WebTransport session");
            if let Err(error) = request.close(401).await {
                warn!(error = %error, "failed to close unauthorized MoQ session");
            }
            continue;
        }
        let origin = origin.clone();
        tokio::spawn(async move {
            match request.with_publisher(origin).ok().await {
                Ok(session) => {
                    let _ = session.closed().await;
                }
                Err(error) => warn!(error = %error, "MoQ session failed"),
            }
        });
    }
    Ok(())
}

struct LocTrack {
    track: moq_net::track::Producer,
    group: Option<moq_net::group::Producer>,
}

impl LocTrack {
    fn new(track: moq_net::track::Producer) -> Self {
        Self { track, group: None }
    }

    fn needs_keyframe(&self) -> bool {
        self.group.is_none()
    }

    fn write(
        &mut self,
        timestamp: moq_net::Timestamp,
        payload: &[u8],
        keyframe: bool,
    ) -> Result<()> {
        if keyframe {
            self.cut()?;
        }
        if self.group.is_none() {
            if !keyframe {
                anyhow::bail!("LOC group must start with a keyframe");
            }
            self.group = Some(self.track.append_group()?);
        }

        let timestamp = timestamp.convert(moq_net::Timescale::MICRO)?;
        let payload = moq_loc::encode(timestamp.value(), payload)?;
        self.group
            .as_mut()
            .expect("LOC group exists after keyframe")
            .write_frame(timestamp, payload)?;
        Ok(())
    }

    fn cut(&mut self) -> Result<()> {
        if let Some(mut group) = self.group.take() {
            group.finish()?;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        self.cut()?;
        self.track.finish()?;
        Ok(())
    }
}

struct DevicePublication {
    broadcast: moq_net::broadcast::Producer,
    msf_catalog: moq_net::track::Producer,
    video: LocTrack,
    audio: LocTrack,
    audio_group_start: Option<i64>,
}

impl DevicePublication {
    fn new(device_id: &str, origin: moq_net::origin::Producer) -> Result<Self> {
        let path = broadcast_name(device_id);
        let mut broadcast = origin
            .create_broadcast(
                path.as_str(),
                moq_net::broadcast::Route::new().with_announce(true),
            )
            .context("create MSF MoQ broadcast")?;

        let mut msf_catalog = broadcast.create_track(moq_msf::DEFAULT_NAME, None)?;
        let media_info = moq_net::track::Info::default()
            .with_timescale(moq_net::Timescale::MICRO)
            .with_latency_max(Duration::from_secs(2));
        let video_track = broadcast.create_track(VIDEO_TRACK, media_info.clone())?;
        let audio_track = broadcast.create_track(AUDIO_TRACK, media_info)?;

        publish_catalog(&mut msf_catalog)?;

        Ok(Self {
            broadcast,
            msf_catalog,
            video: LocTrack::new(video_track),
            audio: LocTrack::new(audio_track),
            audio_group_start: None,
        })
    }

    fn cut_video(&mut self) -> Result<()> {
        self.video.cut()?;
        Ok(())
    }

    fn cut_audio(&mut self) -> Result<()> {
        self.audio.cut()?;
        self.audio_group_start = None;
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        self.video.finish()?;
        self.audio.finish()?;
        self.msf_catalog.finish()?;
        self.broadcast.finish();
        Ok(())
    }
}

fn broadcast_name(device_id: &str) -> String {
    format!("{device_id}.msf")
}

fn msf_catalog() -> moq_msf::Catalog {
    let mut video = moq_msf::Track::new(VIDEO_TRACK, moq_msf::Packaging::Loc);
    video.is_live = true;
    video.role = Some(moq_msf::Role::Video);
    video.codec = Some("avc3.4d001f".to_owned());
    video.width = Some(640);
    video.height = Some(480);
    video.framerate = Some(30.0);
    video.render_group = Some(1);
    video.max_grp_sap_starting_type = Some(1);
    video.max_obj_sap_starting_type = Some(1);
    video.jitter = Some(Duration::from_millis(34));

    let mut audio = moq_msf::Track::new(AUDIO_TRACK, moq_msf::Packaging::Loc);
    audio.is_live = true;
    audio.role = Some(moq_msf::Role::Audio);
    audio.codec = Some("mp4a.40.2".to_owned());
    audio.samplerate = Some(48_000);
    audio.channel_config = Some("1".to_owned());
    audio.init_data =
        Some(base64::engine::general_purpose::STANDARD.encode(AAC_AUDIO_SPECIFIC_CONFIG));
    audio.render_group = Some(1);
    audio.max_grp_sap_starting_type = Some(1);
    audio.max_obj_sap_starting_type = Some(1);
    audio.jitter = Some(Duration::from_millis(22));

    moq_msf::Catalog::new(vec![video, audio])
}

fn publish_catalog(msf_track: &mut moq_net::track::Producer) -> Result<()> {
    let mut msf_group = msf_track.append_group()?;
    msf_group.write_frame(
        moq_net::Timestamp::now(),
        Bytes::from(msf_catalog().to_json()?),
    )?;
    msf_group.finish()?;
    Ok(())
}

async fn publish_device(
    device_id: &str,
    origin: moq_net::origin::Producer,
    subscription: FrameSubscription,
    status: Arc<SharedStatus>,
) -> Result<()> {
    let mut publication = DevicePublication::new(device_id, origin)?;

    // Do not inject FrameHub::initial_video here. It is only the latest IDR, not
    // the complete GOP from that IDR to the live edge; current P frames may depend
    // on missing intermediate frames. The board emits an IDR every ~1 second, so
    // waiting for the next live IDR is both decodable and still low latency.
    let mut video_receiver = subscription.video;
    let mut audio_receiver = subscription.aac;

    loop {
        tokio::select! {
            frame = video_receiver.recv() => match frame {
                Ok(frame) => {
                    if let Err(error) =
                        write_video(&mut publication.video, frame, &status)
                    {
                        warn!(device_id, error = %format!("{error:#}"), "dropping MoQ video frame");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(device_id, skipped, "MoQ video input lagged; waiting for IDR");
                    publication.cut_video()?;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
            frame = audio_receiver.recv() => match frame {
                Ok(frame) => {
                    write_audio(
                        &mut publication.audio,
                        &mut publication.audio_group_start,
                        frame,
                        &status,
                    )?;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(device_id, skipped, "MoQ AAC input lagged; cutting audio group");
                    publication.cut_audio()?;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
        }
    }
    publication.finish()?;
    Ok(())
}

fn write_video(producer: &mut LocTrack, frame: EncodedFrame, status: &SharedStatus) -> Result<()> {
    if producer.needs_keyframe() && !frame.key {
        return Ok(());
    }
    producer.write(timestamp(frame.pts_us)?, &frame.data, frame.key)?;
    status.video_frames.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

fn write_audio(
    producer: &mut LocTrack,
    group_start: &mut Option<i64>,
    frame: EncodedFrame,
    status: &SharedStatus,
) -> Result<()> {
    for (index, payload) in adts_payloads(&frame.data).into_iter().enumerate() {
        let pts_us = frame.pts_us.saturating_add(
            i64::try_from(index)
                .unwrap_or(i64::MAX)
                .saturating_mul(AAC_FRAME_US),
        );
        if group_start.is_some_and(|start| pts_us.saturating_sub(start) >= AUDIO_GROUP_US) {
            producer.cut()?;
            *group_start = None;
        }
        let keyframe = producer.needs_keyframe();
        producer.write(timestamp(pts_us)?, payload, keyframe)?;
        if keyframe {
            *group_start = Some(pts_us);
        }
        status.audio_frames.fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}

fn timestamp(pts_us: i64) -> Result<moq_net::Timestamp> {
    Ok(moq_net::Timestamp::from_micros(
        u64::try_from(pts_us.max(0)).unwrap_or_default(),
    )?)
}

fn adts_payloads(data: &[u8]) -> Vec<&[u8]> {
    let mut frames = Vec::new();
    let mut offset = 0usize;
    while data.len().saturating_sub(offset) >= 7 {
        let header = &data[offset..];
        if header[0] != 0xff || header[1] & 0xf6 != 0xf0 {
            break;
        }
        let header_len = if header[1] & 1 != 0 { 7 } else { 9 };
        let frame_len = (usize::from(header[3] & 0x03) << 11)
            | (usize::from(header[4]) << 3)
            | usize::from(header[5] >> 5);
        if frame_len < header_len || frame_len > header.len() {
            break;
        }
        frames.push(&header[header_len..frame_len]);
        offset += frame_len;
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::{LocTrack, adts_payloads, broadcast_name, msf_catalog, timestamp};

    #[test]
    fn strips_adts_headers_and_splits_frames() {
        let mut packet = Vec::new();
        packet.extend_from_slice(&[0xff, 0xf1, 0x4c, 0x40, 0x01, 0x5f, 0xfc, 1, 2, 3]);
        packet.extend_from_slice(&[0xff, 0xf1, 0x4c, 0x40, 0x01, 0x3f, 0xfc, 4, 5]);
        assert_eq!(adts_payloads(&packet), vec![&[1, 2, 3][..], &[4, 5][..]]);
    }

    #[test]
    fn uses_msf_broadcast_name() {
        assert_eq!(broadcast_name("front"), "front.msf");
    }

    #[test]
    fn msf_catalog_advertises_loc_media() {
        let msf = msf_catalog();
        assert_eq!(msf.tracks.len(), 2);
        assert!(
            msf.tracks
                .iter()
                .all(|track| track.packaging == moq_msf::Packaging::Loc)
        );
        assert_eq!(
            msf.tracks
                .iter()
                .find(|track| track.name == "audio")
                .and_then(|track| track.init_data.as_deref()),
            Some("EYg=")
        );

        let json = serde_json::from_str::<serde_json::Value>(&msf.to_json().unwrap()).unwrap();
        assert_eq!(json["version"], "draft-01");
        assert_eq!(json["tracks"][0]["packaging"], "loc");
        assert_eq!(json["tracks"][1]["packaging"], "loc");
    }

    #[tokio::test]
    async fn loc_track_starts_on_keyframe_and_writes_loc_frames() {
        let mut broadcast = moq_net::broadcast::Info::new().produce();
        let track = broadcast
            .create_track(
                "video",
                moq_net::track::Info::default().with_timescale(moq_net::Timescale::MICRO),
            )
            .unwrap();
        let mut subscriber = track.subscribe(None);
        let mut producer = LocTrack::new(track);

        assert!(
            producer
                .write(timestamp(900).unwrap(), &[0], false)
                .is_err()
        );
        producer
            .write(timestamp(1_000).unwrap(), &[1, 2], true)
            .unwrap();
        producer
            .write(timestamp(2_000).unwrap(), &[3, 4], false)
            .unwrap();
        producer.finish().unwrap();

        let mut group = subscriber.next_group().await.unwrap().unwrap();
        let first = moq_loc::decode(group.read_frame().await.unwrap().unwrap().payload).unwrap();
        let second = moq_loc::decode(group.read_frame().await.unwrap().unwrap().payload).unwrap();
        assert_eq!(first.timestamp, 1_000);
        assert_eq!(&first.payload[..], &[1, 2]);
        assert_eq!(second.timestamp, 2_000);
        assert_eq!(&second.payload[..], &[3, 4]);
        assert!(group.read_frame().await.unwrap().is_none());
        broadcast.finish();
    }
}
