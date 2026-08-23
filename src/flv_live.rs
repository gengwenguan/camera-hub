use crate::benchmark::{BenchmarkAnchor, BenchmarkRegistry};
use crate::frames::{EncodedFrame, FrameSubscription};
use anyhow::{Context, Result, bail};
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncWriteExt, DuplexStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::{info, warn};

const MAX_FLV_SESSIONS: usize = 4;
const OUTPUT_BUFFER_BYTES: usize = 256 * 1024;
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_FLV_TAG_DATA: usize = 0x00ff_ffff;
const AAC_SAMPLE_RATES: [u32; 13] = [
    96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025, 8_000,
    7_350,
];

pub struct FlvLive {
    permits: Arc<Semaphore>,
    benchmark: Arc<BenchmarkRegistry>,
}

pub struct FlvOutput {
    pub stream: DuplexStream,
}

impl FlvLive {
    pub fn new(benchmark: Arc<BenchmarkRegistry>) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(MAX_FLV_SESSIONS)),
            benchmark,
        }
    }

    pub async fn open(
        &self,
        device_id: &str,
        subscription: FrameSubscription,
        benchmark_id: Option<String>,
    ) -> Result<FlvOutput> {
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .context("HTTP-FLV session limit reached")?;
        start_flv(
            device_id,
            subscription,
            permit,
            self.benchmark.clone(),
            benchmark_id,
        )
        .await
    }
}

async fn start_flv(
    device_id: &str,
    mut subscription: FrameSubscription,
    permit: OwnedSemaphorePermit,
    benchmark: Arc<BenchmarkRegistry>,
    benchmark_id: Option<String>,
) -> Result<FlvOutput> {
    let (mut writer, stream) = tokio::io::duplex(OUTPUT_BUFFER_BYTES);
    write_output(&mut writer, &flv_header()).await?;

    let mut muxer = FlvMuxer::default();
    if let Some(initial) = subscription.initial_video.take() {
        muxer.prime_video_config(&initial.data);
    }

    let id = device_id.to_owned();
    tokio::spawn(async move {
        let _permit = permit;
        if let Err(error) = feed_flv(
            &id,
            subscription,
            &mut muxer,
            &mut writer,
            &benchmark,
            benchmark_id.as_deref(),
        )
        .await
        {
            if is_client_disconnect(&error) {
                info!(device_id = %id, "HTTP-FLV client disconnected");
            } else {
                warn!(
                    device_id = %id,
                    error = %format!("{error:#}"),
                    "HTTP-FLV session stopped"
                );
            }
        }
        let _ = writer.shutdown().await;
        info!(device_id = %id, "HTTP-FLV session closed");
    });
    Ok(FlvOutput { stream })
}

async fn feed_flv(
    device_id: &str,
    mut subscription: FrameSubscription,
    muxer: &mut FlvMuxer,
    output: &mut DuplexStream,
    benchmark: &BenchmarkRegistry,
    benchmark_id: Option<&str>,
) -> Result<()> {
    let mut benchmark_anchored = false;
    loop {
        tokio::select! {
            frame = subscription.video.recv() => match frame {
                Ok(frame) => match muxer.video(&frame) {
                    Ok(tags) => {
                        if !benchmark_anchored && frame.key && !tags.is_empty() {
                            if let Some(session_id) = benchmark_id {
                                benchmark.set_anchor_value(
                                    session_id,
                                    "flv",
                                    BenchmarkAnchor {
                                        sequence: frame.sequence,
                                        pts_us: frame.pts_us,
                                        capture_epoch_us: frame.capture_epoch_us,
                                        source_clock: frame.source_clock,
                                        media_time_us: Some(0),
                                    },
                                );
                            }
                            benchmark_anchored = true;
                        }
                        write_tags(output, tags).await?
                    },
                    Err(error) => {
                        warn!(
                            device_id,
                            error = %format!("{error:#}"),
                            "dropping malformed HTTP-FLV video access unit"
                        );
                        muxer.wait_for_keyframe();
                    }
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(device_id, skipped, "HTTP-FLV video input lagged; waiting for IDR");
                    muxer.wait_for_keyframe();
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
            frame = subscription.aac.recv() => match frame {
                Ok(frame) => match muxer.audio(&frame) {
                    Ok(tags) => write_tags(output, tags).await?,
                    Err(error) => {
                        warn!(
                            device_id,
                            error = %format!("{error:#}"),
                            "dropping malformed HTTP-FLV AAC packet"
                        );
                    }
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(device_id, skipped, "HTTP-FLV AAC input lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
        }
    }
    Ok(())
}

async fn write_tags(output: &mut DuplexStream, tags: Vec<Vec<u8>>) -> Result<()> {
    for tag in tags {
        write_output(output, &tag).await?;
    }
    Ok(())
}

async fn write_output(output: &mut DuplexStream, data: &[u8]) -> Result<()> {
    tokio::time::timeout(WRITE_TIMEOUT, output.write_all(data))
        .await
        .context("HTTP-FLV client write timed out")?
        .context("write HTTP-FLV response")
}

fn is_client_disconnect(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.downcast_ref::<io::Error>().is_some_and(|error| {
            matches!(
                error.kind(),
                io::ErrorKind::BrokenPipe
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::ConnectionReset
            )
        })
    })
}

struct FlvMuxer {
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
    sent_video_config: Option<(Vec<u8>, Vec<u8>)>,
    audio_config: Option<AacConfig>,
    sent_audio_config: Option<AacConfig>,
    video_base_pts_us: Option<i64>,
    audio_base_pts_us: Option<i64>,
    last_video_timestamp: u32,
    last_audio_timestamp: u32,
    needs_keyframe: bool,
}

impl Default for FlvMuxer {
    fn default() -> Self {
        Self {
            sps: None,
            pps: None,
            sent_video_config: None,
            audio_config: None,
            sent_audio_config: None,
            video_base_pts_us: None,
            audio_base_pts_us: None,
            last_video_timestamp: 0,
            last_audio_timestamp: 0,
            needs_keyframe: true,
        }
    }
}

impl FlvMuxer {
    fn prime_video_config(&mut self, data: &[u8]) {
        let nals = annex_b_nals(data);
        self.update_parameter_sets(&nals);
    }

    fn wait_for_keyframe(&mut self) {
        self.needs_keyframe = true;
        self.sent_video_config = None;
    }

    fn video(&mut self, frame: &EncodedFrame) -> Result<Vec<Vec<u8>>> {
        let nals = annex_b_nals(&frame.data);
        if nals.is_empty() {
            bail!("H264 access unit has no Annex-B NAL units");
        }
        self.update_parameter_sets(&nals);

        let keyframe = frame.key || nals.iter().any(|nal| nal[0] & 0x1f == 5);
        if self.needs_keyframe && !keyframe {
            return Ok(Vec::new());
        }
        if keyframe && (self.sps.is_none() || self.pps.is_none()) {
            return Ok(Vec::new());
        }

        let base = *self.video_base_pts_us.get_or_insert(frame.pts_us);
        let timestamp = relative_timestamp(frame.pts_us, base, &mut self.last_video_timestamp);
        let mut tags = Vec::with_capacity(3);

        if keyframe {
            let sps = self.sps.as_ref().context("H264 SPS is missing")?;
            let pps = self.pps.as_ref().context("H264 PPS is missing")?;
            let config_changed = match self.sent_video_config.as_ref() {
                Some((sent_sps, sent_pps)) => sent_sps != sps || sent_pps != pps,
                None => true,
            };
            if config_changed {
                tags.push(avc_sequence_tag(timestamp, sps, pps)?);
                self.sent_video_config = Some((sps.clone(), pps.clone()));
            }
            if let Some(config) = self.audio_config
                && self.sent_audio_config != Some(config)
            {
                tags.push(aac_sequence_tag(0, config)?);
                self.sent_audio_config = Some(config);
            }
            self.needs_keyframe = false;
        }

        if let Some(tag) = avc_video_tag(timestamp, keyframe, &nals)? {
            tags.push(tag);
        }
        Ok(tags)
    }

    fn audio(&mut self, frame: &EncodedFrame) -> Result<Vec<Vec<u8>>> {
        let frames = parse_adts_frames(&frame.data)?;
        if frames.is_empty() {
            bail!("AAC packet has no complete ADTS frames");
        }
        self.audio_config = Some(frames[0].config);
        if self.video_base_pts_us.is_none() {
            return Ok(Vec::new());
        }

        let base = *self.audio_base_pts_us.get_or_insert(frame.pts_us);
        let mut offset_us = 0i64;
        let mut tags = Vec::with_capacity(frames.len() + 1);
        for parsed in frames {
            let pts_us = frame.pts_us.saturating_add(offset_us);
            let timestamp = relative_timestamp(pts_us, base, &mut self.last_audio_timestamp);
            self.audio_config = Some(parsed.config);
            if self.sent_audio_config != Some(parsed.config) {
                tags.push(aac_sequence_tag(timestamp, parsed.config)?);
                self.sent_audio_config = Some(parsed.config);
            }
            tags.push(aac_raw_tag(timestamp, parsed.config, parsed.payload)?);
            offset_us = offset_us.saturating_add(parsed.duration_us);
        }
        Ok(tags)
    }

    fn update_parameter_sets(&mut self, nals: &[&[u8]]) {
        for nal in nals {
            match nal[0] & 0x1f {
                7 => self.sps = Some(nal.to_vec()),
                8 => self.pps = Some(nal.to_vec()),
                _ => {}
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AacConfig {
    object_type: u8,
    sample_rate_index: u8,
    channel_config: u8,
}

impl AacConfig {
    fn audio_specific_config(self) -> [u8; 2] {
        [
            (self.object_type << 3) | (self.sample_rate_index >> 1),
            ((self.sample_rate_index & 1) << 7) | (self.channel_config << 3),
        ]
    }

    fn flv_audio_header(self) -> u8 {
        0xae | u8::from(self.channel_config > 1)
    }
}

struct AdtsFrame<'a> {
    config: AacConfig,
    payload: &'a [u8],
    duration_us: i64,
}

fn parse_adts_frames(data: &[u8]) -> Result<Vec<AdtsFrame<'_>>> {
    let mut frames = Vec::new();
    let mut offset = 0usize;
    while data.len().saturating_sub(offset) >= 7 {
        let header = &data[offset..];
        if header[0] != 0xff || header[1] & 0xf6 != 0xf0 {
            bail!("invalid ADTS sync word at offset {offset}");
        }
        let header_len = if header[1] & 1 != 0 { 7 } else { 9 };
        let frame_len = (usize::from(header[3] & 0x03) << 11)
            | (usize::from(header[4]) << 3)
            | usize::from(header[5] >> 5);
        if frame_len < header_len || frame_len > header.len() {
            bail!("invalid ADTS frame length {frame_len} at offset {offset}");
        }

        let sample_rate_index = (header[2] >> 2) & 0x0f;
        let sample_rate = AAC_SAMPLE_RATES
            .get(usize::from(sample_rate_index))
            .copied()
            .context("unsupported ADTS sample-rate index")?;
        let config = AacConfig {
            object_type: ((header[2] >> 6) & 0x03) + 1,
            sample_rate_index,
            channel_config: ((header[2] & 0x01) << 2) | (header[3] >> 6),
        };
        let raw_data_blocks = i64::from(header[6] & 0x03) + 1;
        let duration_us = raw_data_blocks
            .saturating_mul(1024)
            .saturating_mul(1_000_000)
            / i64::from(sample_rate);
        frames.push(AdtsFrame {
            config,
            payload: &header[header_len..frame_len],
            duration_us,
        });
        offset += frame_len;
    }
    if offset != data.len() {
        bail!("trailing incomplete ADTS data");
    }
    Ok(frames)
}

fn annex_b_nals(data: &[u8]) -> Vec<&[u8]> {
    let mut nals = Vec::new();
    let mut offset = 0usize;
    while let Some((start, start_code_len)) = find_start_code(data, offset) {
        let nal_start = start + start_code_len;
        let nal_end = find_start_code(data, nal_start)
            .map(|(next, _)| next)
            .unwrap_or(data.len());
        if nal_end > nal_start {
            nals.push(&data[nal_start..nal_end]);
        }
        if nal_end == data.len() {
            break;
        }
        offset = nal_end;
    }
    nals
}

fn find_start_code(data: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut index = from;
    while index + 3 <= data.len() {
        if index + 4 <= data.len() && data[index..index + 4] == [0, 0, 0, 1] {
            return Some((index, 4));
        }
        if data[index..index + 3] == [0, 0, 1] {
            return Some((index, 3));
        }
        index += 1;
    }
    None
}

fn relative_timestamp(pts_us: i64, base_pts_us: i64, last: &mut u32) -> u32 {
    let elapsed_ms = pts_us.saturating_sub(base_pts_us).max(0) / 1000;
    let timestamp = u32::try_from(elapsed_ms).unwrap_or(u32::MAX).max(*last);
    *last = timestamp;
    timestamp
}

fn flv_header() -> [u8; 13] {
    [b'F', b'L', b'V', 1, 5, 0, 0, 0, 9, 0, 0, 0, 0]
}

fn avc_sequence_tag(timestamp: u32, sps: &[u8], pps: &[u8]) -> Result<Vec<u8>> {
    if sps.len() < 4 {
        bail!("H264 SPS is too short");
    }
    let sps_len = u16::try_from(sps.len()).context("H264 SPS is too large")?;
    let pps_len = u16::try_from(pps.len()).context("H264 PPS is too large")?;
    let mut payload = Vec::with_capacity(16 + sps.len() + pps.len());
    payload.extend_from_slice(&[0x17, 0, 0, 0, 0]);
    payload.extend_from_slice(&[1, sps[1], sps[2], sps[3], 0xff, 0xe1]);
    payload.extend_from_slice(&sps_len.to_be_bytes());
    payload.extend_from_slice(sps);
    payload.push(1);
    payload.extend_from_slice(&pps_len.to_be_bytes());
    payload.extend_from_slice(pps);
    flv_tag(9, timestamp, &payload)
}

fn avc_video_tag(timestamp: u32, keyframe: bool, nals: &[&[u8]]) -> Result<Option<Vec<u8>>> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&[if keyframe { 0x17 } else { 0x27 }, 1, 0, 0, 0]);
    for nal in nals {
        if matches!(nal[0] & 0x1f, 7 | 8) {
            continue;
        }
        let length = u32::try_from(nal.len()).context("H264 NAL unit is too large")?;
        payload.extend_from_slice(&length.to_be_bytes());
        payload.extend_from_slice(nal);
    }
    if payload.len() == 5 {
        return Ok(None);
    }
    Ok(Some(flv_tag(9, timestamp, &payload)?))
}

fn aac_sequence_tag(timestamp: u32, config: AacConfig) -> Result<Vec<u8>> {
    let asc = config.audio_specific_config();
    flv_tag(
        8,
        timestamp,
        &[config.flv_audio_header(), 0, asc[0], asc[1]],
    )
}

fn aac_raw_tag(timestamp: u32, config: AacConfig, data: &[u8]) -> Result<Vec<u8>> {
    let mut payload = Vec::with_capacity(data.len() + 2);
    payload.extend_from_slice(&[config.flv_audio_header(), 1]);
    payload.extend_from_slice(data);
    flv_tag(8, timestamp, &payload)
}

fn flv_tag(tag_type: u8, timestamp: u32, payload: &[u8]) -> Result<Vec<u8>> {
    if payload.len() > MAX_FLV_TAG_DATA {
        bail!("FLV tag payload is too large");
    }
    let data_size = payload.len() as u32;
    let mut output = Vec::with_capacity(11 + payload.len() + 4);
    output.push(tag_type);
    output.extend_from_slice(&[
        ((data_size >> 16) & 0xff) as u8,
        ((data_size >> 8) & 0xff) as u8,
        (data_size & 0xff) as u8,
    ]);
    output.extend_from_slice(&[
        ((timestamp >> 16) & 0xff) as u8,
        ((timestamp >> 8) & 0xff) as u8,
        (timestamp & 0xff) as u8,
        ((timestamp >> 24) & 0xff) as u8,
    ]);
    output.extend_from_slice(&[0, 0, 0]);
    output.extend_from_slice(payload);
    output.extend_from_slice(&(11 + data_size).to_be_bytes());
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(pts_us: i64, key: bool, data: Vec<u8>) -> EncodedFrame {
        EncodedFrame {
            sequence: 1,
            pts_us,
            capture_epoch_us: pts_us + 100,
            received_epoch_us: pts_us + 200,
            source_clock: true,
            key,
            data: Arc::from(data),
        }
    }

    fn tag_payload(tag: &[u8]) -> &[u8] {
        let size = (usize::from(tag[1]) << 16) | (usize::from(tag[2]) << 8) | usize::from(tag[3]);
        &tag[11..11 + size]
    }

    fn tag_timestamp(tag: &[u8]) -> u32 {
        (u32::from(tag[7]) << 24)
            | (u32::from(tag[4]) << 16)
            | (u32::from(tag[5]) << 8)
            | u32::from(tag[6])
    }

    fn key_access_unit() -> Vec<u8> {
        [
            &[0, 0, 0, 1, 0x67, 0x4d, 0x00, 0x1f, 0xaa][..],
            &[0, 0, 1, 0x68, 0xee, 0x3c, 0x80][..],
            &[0, 0, 0, 1, 0x65, 1, 2, 3][..],
        ]
        .concat()
    }

    fn adts(payload: &[u8]) -> Vec<u8> {
        let frame_len = payload.len() + 7;
        let mut output = vec![
            0xff,
            0xf1,
            0x4c,
            0x40 | ((frame_len >> 11) & 0x03) as u8,
            ((frame_len >> 3) & 0xff) as u8,
            (((frame_len & 0x07) << 5) as u8) | 0x1f,
            0xfc,
        ];
        output.extend_from_slice(payload);
        output
    }

    #[test]
    fn writes_standard_flv_header() {
        assert_eq!(
            flv_header(),
            [b'F', b'L', b'V', 1, 5, 0, 0, 0, 9, 0, 0, 0, 0]
        );
    }

    #[test]
    fn converts_annex_b_keyframe_to_avc_tags() {
        let mut muxer = FlvMuxer::default();
        let tags = muxer
            .video(&frame(1_000_000, true, key_access_unit()))
            .unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0][0], 9);
        assert_eq!(&tag_payload(&tags[0])[..6], &[0x17, 0, 0, 0, 0, 1]);
        assert_eq!(tag_payload(&tags[0])[6], 0x4d);
        assert_eq!(&tag_payload(&tags[1])[..5], &[0x17, 1, 0, 0, 0]);
        assert_eq!(&tag_payload(&tags[1])[5..9], &4u32.to_be_bytes());
        assert_eq!(&tag_payload(&tags[1])[9..], &[0x65, 1, 2, 3]);
        assert_eq!(tag_timestamp(&tags[1]), 0);
    }

    #[test]
    fn strips_adts_and_writes_aac_sequence_header() {
        let mut muxer = FlvMuxer::default();
        muxer
            .video(&frame(1_000_000, true, key_access_unit()))
            .unwrap();
        let tags = muxer
            .audio(&frame(500_000, false, adts(&[1, 2, 3])))
            .unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tag_payload(&tags[0]), &[0xae, 0, 0x11, 0x88]);
        assert_eq!(tag_payload(&tags[1]), &[0xae, 1, 1, 2, 3]);
        assert_eq!(tag_timestamp(&tags[1]), 0);
    }

    #[test]
    fn keeps_each_track_timestamp_monotonic_from_its_own_anchor() {
        let mut muxer = FlvMuxer::default();
        muxer
            .video(&frame(1_000_000, true, key_access_unit()))
            .unwrap();
        let video = muxer
            .video(&frame(1_033_333, false, vec![0, 0, 1, 0x41, 4, 5]))
            .unwrap();
        muxer.audio(&frame(500_000, false, adts(&[1]))).unwrap();
        let audio = muxer.audio(&frame(521_333, false, adts(&[2]))).unwrap();
        assert_eq!(tag_timestamp(&video[0]), 33);
        assert_eq!(tag_timestamp(&audio[0]), 21);
    }

    #[test]
    fn waits_for_idr_and_resends_video_config_after_lag() {
        let mut muxer = FlvMuxer::default();
        muxer
            .video(&frame(1_000_000, true, key_access_unit()))
            .unwrap();
        muxer.wait_for_keyframe();
        assert!(
            muxer
                .video(&frame(1_033_333, false, vec![0, 0, 1, 0x41, 4, 5],))
                .unwrap()
                .is_empty()
        );
        let tags = muxer
            .video(&frame(2_000_000, true, key_access_unit()))
            .unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tag_payload(&tags[0])[1], 0);
        assert_eq!(tag_payload(&tags[1])[1], 1);
        assert_eq!(tag_timestamp(&tags[1]), 1000);
    }
}
