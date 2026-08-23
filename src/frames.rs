use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

const VIDEO_CLOCK_CAPACITY: usize = 1_024;
const CLOCK_SYNC_CAPACITY: usize = 32;
const AUDIO_ALIGN_THRESHOLD_US: i64 = 100_000;
const AUDIO_CORRECTION_STEP_US: i64 = 500;

// AI, recording, MSE, and WebRTC all consume the encoded H264/AAC frame bus.
#[derive(Clone, Debug)]
pub struct EncodedFrame {
    pub sequence: u32,
    pub pts_us: i64,
    pub capture_epoch_us: i64,
    pub received_epoch_us: i64,
    pub source_clock: bool,
    pub key: bool,
    pub data: Arc<[u8]>,
}

struct DeviceFrames {
    video: broadcast::Sender<EncodedFrame>,
    aac: broadcast::Sender<EncodedFrame>,
    latest_keyframe: Option<EncodedFrame>,
    video_frames: u64,
    aac_frames: u64,
    video_pts_us: i64,
    aac_pts_us: i64,
    aac_raw_pts_us: i64,
    video_pts_rewinds: u64,
    aac_pts_rewinds: u64,
    video_epoch_offset_us: Option<i64>,
    audio_pts_correction_us: Option<i64>,
    video_clock: VecDeque<FrameClock>,
    clock_sync: VecDeque<ClockSync>,
    last_seen: Instant,
}

impl DeviceFrames {
    fn new() -> Self {
        Self {
            video: broadcast::channel(16).0,
            aac: broadcast::channel(128).0,
            latest_keyframe: None,
            video_frames: 0,
            aac_frames: 0,
            video_pts_us: 0,
            aac_pts_us: 0,
            aac_raw_pts_us: 0,
            video_pts_rewinds: 0,
            aac_pts_rewinds: 0,
            video_epoch_offset_us: None,
            audio_pts_correction_us: None,
            video_clock: VecDeque::with_capacity(VIDEO_CLOCK_CAPACITY),
            clock_sync: VecDeque::with_capacity(CLOCK_SYNC_CAPACITY),
            last_seen: Instant::now(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct FrameClock {
    pub sequence: u32,
    pub pts_us: i64,
    pub capture_epoch_us: i64,
    pub received_epoch_us: i64,
    pub key: bool,
    pub source_clock: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct FrameStatus {
    pub device_id: String,
    pub video_frames: u64,
    pub aac_frames: u64,
    pub video_pts_us: i64,
    pub aac_pts_us: i64,
    pub av_delta_ms: i64,
    pub video_pts_rewinds: u64,
    pub aac_pts_rewinds: u64,
    pub audio_pts_correction_ms: i64,
    pub age_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClockSync {
    pub source_to_server_offset_us: i64,
    pub rtt_us: u64,
}

pub struct FrameSubscription {
    pub initial_video: Option<EncodedFrame>,
    pub video: broadcast::Receiver<EncodedFrame>,
    pub aac: broadcast::Receiver<EncodedFrame>,
}

#[derive(Default)]
pub struct FrameHub {
    devices: Mutex<BTreeMap<String, DeviceFrames>>,
}

impl FrameHub {
    pub fn push(
        &self,
        device_id: &str,
        kind: u8,
        sequence: u32,
        pts_us: i64,
        capture_epoch_us: Option<i64>,
        key: bool,
        data: Arc<[u8]>,
    ) {
        let mut devices = self
            .devices
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let device = devices
            .entry(device_id.to_owned())
            .or_insert_with(DeviceFrames::new);
        device.last_seen = Instant::now();
        let received_epoch_us = epoch_us();
        let source_epoch_us = capture_epoch_us.unwrap_or(received_epoch_us);
        let pts_us = match kind {
            1 => {
                if device.video_frames > 0 && pts_us < device.video_pts_us {
                    device.video_epoch_offset_us = None;
                    device.audio_pts_correction_us = None;
                }
                device.video_epoch_offset_us = Some(source_epoch_us.saturating_sub(pts_us));
                pts_us
            }
            2 => align_audio_pts(device, pts_us, source_epoch_us),
            _ => pts_us,
        };
        let frame = EncodedFrame {
            sequence,
            pts_us,
            capture_epoch_us: capture_epoch_us.unwrap_or(received_epoch_us),
            received_epoch_us,
            source_clock: capture_epoch_us.is_some(),
            key,
            data,
        };
        match kind {
            1 => {
                device.video_frames = device.video_frames.saturating_add(1);
                if device.video_frames > 1 && frame.pts_us < device.video_pts_us {
                    device.video_pts_rewinds = device.video_pts_rewinds.saturating_add(1);
                }
                device.video_pts_us = frame.pts_us;
                if device.video_clock.len() == VIDEO_CLOCK_CAPACITY {
                    device.video_clock.pop_front();
                }
                device.video_clock.push_back(FrameClock {
                    sequence: frame.sequence,
                    pts_us: frame.pts_us,
                    capture_epoch_us: frame.capture_epoch_us,
                    received_epoch_us: frame.received_epoch_us,
                    key: frame.key,
                    source_clock: frame.source_clock,
                });
                if frame.key {
                    device.latest_keyframe = Some(frame.clone());
                }
                let _ = device.video.send(frame);
            }
            2 => {
                device.aac_frames = device.aac_frames.saturating_add(1);
                if device.aac_frames > 1 && frame.pts_us < device.aac_pts_us {
                    device.aac_pts_rewinds = device.aac_pts_rewinds.saturating_add(1);
                }
                device.aac_pts_us = frame.pts_us;
                let _ = device.aac.send(frame);
            }
            _ => {}
        }
    }

    pub fn statuses(&self) -> Vec<FrameStatus> {
        self.devices
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .map(|(device_id, device)| FrameStatus {
                device_id: device_id.clone(),
                video_frames: device.video_frames,
                aac_frames: device.aac_frames,
                video_pts_us: device.video_pts_us,
                aac_pts_us: device.aac_pts_us,
                av_delta_ms: device.video_pts_us.saturating_sub(device.aac_pts_us) / 1000,
                video_pts_rewinds: device.video_pts_rewinds,
                aac_pts_rewinds: device.aac_pts_rewinds,
                audio_pts_correction_ms: device.audio_pts_correction_us.unwrap_or_default() / 1000,
                age_ms: device.last_seen.elapsed().as_millis() as u64,
            })
            .collect()
    }

    pub fn latest_keyframes(&self) -> Vec<(String, EncodedFrame)> {
        self.devices
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter_map(|(device_id, device)| {
                device
                    .latest_keyframe
                    .clone()
                    .map(|frame| (device_id.clone(), frame))
            })
            .collect()
    }

    pub fn video_clock(&self, device_id: &str, after: Option<u32>) -> Vec<FrameClock> {
        self.devices
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(device_id)
            .map(|device| {
                device
                    .video_clock
                    .iter()
                    .filter(|frame| after.is_none_or(|value| sequence_after(frame.sequence, value)))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn update_clock_sync(
        &self,
        device_id: &str,
        server_send_us: i64,
        source_receive_us: i64,
        source_send_us: i64,
        server_receive_us: i64,
    ) -> bool {
        let Some(sample) = calculate_clock_sync(
            server_send_us,
            source_receive_us,
            source_send_us,
            server_receive_us,
        ) else {
            return false;
        };
        let mut devices = self
            .devices
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let device = devices
            .entry(device_id.to_owned())
            .or_insert_with(DeviceFrames::new);
        if device.clock_sync.len() == CLOCK_SYNC_CAPACITY {
            device.clock_sync.pop_front();
        }
        device.clock_sync.push_back(sample);
        true
    }

    pub fn reset_clock_sync(&self, device_id: &str) {
        if let Some(device) = self
            .devices
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get_mut(device_id)
        {
            device.clock_sync.clear();
        }
    }

    pub fn clock_sync(&self, device_id: &str) -> Option<ClockSync> {
        self.devices
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(device_id)?
            .clock_sync
            .iter()
            .min_by_key(|sample| sample.rtt_us)
            .cloned()
    }

    pub fn subscribe(&self, device_id: &str) -> Option<FrameSubscription> {
        let devices = self
            .devices
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let device = devices.get(device_id)?;
        Some(FrameSubscription {
            initial_video: device.latest_keyframe.clone(),
            video: device.video.subscribe(),
            aac: device.aac.subscribe(),
        })
    }
}

pub fn epoch_us() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| i64::try_from(value.as_micros()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

fn sequence_after(sequence: u32, previous: u32) -> bool {
    let distance = sequence.wrapping_sub(previous);
    distance != 0 && distance < (1 << 31)
}

fn calculate_clock_sync(
    server_send_us: i64,
    source_receive_us: i64,
    source_send_us: i64,
    server_receive_us: i64,
) -> Option<ClockSync> {
    if source_send_us < source_receive_us || server_receive_us < server_send_us {
        return None;
    }
    let round_trip = i128::from(server_receive_us - server_send_us)
        - i128::from(source_send_us - source_receive_us);
    if !(0..=10_000_000).contains(&round_trip) {
        return None;
    }
    let source_minus_server = (i128::from(source_receive_us - server_send_us)
        + i128::from(source_send_us - server_receive_us))
        / 2;
    Some(ClockSync {
        source_to_server_offset_us: i64::try_from(-source_minus_server).ok()?,
        rtt_us: u64::try_from(round_trip).ok()?,
    })
}

fn align_audio_pts(device: &mut DeviceFrames, pts_us: i64, source_epoch_us: i64) -> i64 {
    let Some(video_offset_us) = device.video_epoch_offset_us else {
        return pts_us;
    };
    if device.aac_frames > 0 && pts_us < device.aac_raw_pts_us {
        device.audio_pts_correction_us = None;
    }
    device.aac_raw_pts_us = pts_us;
    let target = source_epoch_us
        .saturating_sub(video_offset_us)
        .saturating_sub(pts_us);
    let correction = match device.audio_pts_correction_us {
        Some(current) => current.saturating_add(
            target
                .saturating_sub(current)
                .clamp(-AUDIO_CORRECTION_STEP_US, AUDIO_CORRECTION_STEP_US),
        ),
        None if target.abs() >= AUDIO_ALIGN_THRESHOLD_US => target,
        None => 0,
    };
    device.audio_pts_correction_us = Some(correction);
    pts_us.saturating_add(correction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_each_encoded_frame_type() {
        let hub = FrameHub::default();
        hub.push("front", 1, 1, 1000, Some(10_000), true, Arc::from([1u8]));
        hub.push("front", 2, 2, 2000, Some(11_000), false, Arc::from([2u8]));
        let status = hub.statuses().pop().unwrap();
        assert_eq!(status.video_frames, 1);
        assert_eq!(status.aac_frames, 1);
        let (device_id, keyframe) = hub.latest_keyframes().pop().unwrap();
        assert_eq!(device_id, "front");
        assert_eq!(keyframe.sequence, 1);
        assert_eq!(&*keyframe.data, &[1]);
        let subscription = hub.subscribe("front").unwrap();
        assert_eq!(subscription.initial_video.unwrap().sequence, 1);
        assert_eq!(hub.video_clock("front", None)[0].capture_epoch_us, 10_000);
    }

    #[test]
    fn aligns_large_audio_clock_offset_to_video_timeline() {
        let hub = FrameHub::default();
        hub.push(
            "front",
            1,
            1,
            1_000_000,
            Some(10_000_000),
            true,
            Arc::from([1u8]),
        );
        hub.push(
            "front",
            2,
            2,
            500_000,
            Some(10_033_000),
            false,
            Arc::from([2u8]),
        );
        let status = hub.statuses().pop().unwrap();
        assert_eq!(status.aac_pts_us, 1_033_000);
        assert_eq!(status.av_delta_ms, -33);
        assert_eq!(status.audio_pts_correction_ms, 533);
    }

    #[test]
    fn calculates_ntp_style_source_clock_offset_without_network_delay() {
        let hub = FrameHub::default();
        assert!(hub.update_clock_sync("front", 10_000_000, 7_010_000, 7_011_000, 10_021_000,));
        let sync = hub.clock_sync("front").unwrap();
        assert_eq!(sync.source_to_server_offset_us, 3_000_000);
        assert_eq!(sync.rtt_us, 20_000);
    }
}
