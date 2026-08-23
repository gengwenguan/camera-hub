use crate::benchmark::BenchmarkRegistry;
use crate::frames::{EncodedFrame, FrameHub, FrameSubscription};
use anyhow::{Context, Result, bail};
use bytes::Bytes;
use std::collections::BTreeMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket as StdUdpSocket};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use webrtc::api::APIBuilder;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MediaEngine};
use webrtc::api::setting_engine::SettingEngine;
use webrtc::ice::mdns::MulticastDnsMode;
use webrtc::ice::network_type::NetworkType;
use webrtc::interceptor::registry::Registry;
use webrtc::media::Sample;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::RTCPFeedback;
use webrtc::rtp_transceiver::rtp_codec::{
    RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType,
};
use webrtc::track::track_local::TrackLocal;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

pub struct WebRtcRelay {
    frames: Arc<FrameHub>,
    benchmark: Arc<BenchmarkRegistry>,
    sessions: Mutex<BTreeMap<String, Session>>,
}

impl WebRtcRelay {
    pub fn new(frames: Arc<FrameHub>, benchmark: Arc<BenchmarkRegistry>) -> Self {
        Self {
            frames,
            benchmark,
            sessions: Mutex::new(BTreeMap::new()),
        }
    }

    pub async fn answer(
        &self,
        device_id: &str,
        offer: String,
        benchmark_id: Option<String>,
    ) -> Result<String> {
        if offer.len() > 256 * 1024 || !offer.starts_with("v=0") || !offer.contains("\nm=") {
            bail!("invalid WebRTC offer");
        }
        let subscription = self
            .frames
            .subscribe(device_id)
            .context("device frame stream is not ready")?;
        let (session, answer) = create_session(
            device_id,
            offer,
            subscription,
            benchmark_id.map(|id| (self.benchmark.clone(), id)),
        )
        .await?;
        let previous = self
            .sessions
            .lock()
            .await
            .insert(device_id.to_owned(), session);
        if let Some(previous) = previous {
            previous.close().await;
        }
        Ok(answer)
    }

    pub async fn close(&self, device_id: &str) {
        if let Some(session) = self.sessions.lock().await.remove(device_id) {
            session.close().await;
        }
    }
}

struct Session {
    peer: Arc<RTCPeerConnection>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Session {
    async fn close(self) {
        for task in self.tasks {
            task.abort();
            let _ = task.await;
        }
        let _ = self.peer.close().await;
    }
}

async fn create_session(
    device_id: &str,
    offer_sdp: String,
    subscription: FrameSubscription,
    benchmark: Option<(Arc<BenchmarkRegistry>, String)>,
) -> Result<(Session, String)> {
    let mut media_engine = MediaEngine::default();
    media_engine.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48_000,
                channels: 2,
                sdp_fmtp_line: "minptime=10;useinbandfec=1".to_owned(),
                ..Default::default()
            },
            payload_type: 111,
            ..Default::default()
        },
        RTPCodecType::Audio,
    )?;
    media_engine.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                clock_rate: 90_000,
                sdp_fmtp_line:
                    "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=4d001f"
                        .to_owned(),
                rtcp_feedback: vec![
                    RTCPFeedback {
                        typ: "nack".to_owned(),
                        parameter: String::new(),
                    },
                    RTCPFeedback {
                        typ: "nack".to_owned(),
                        parameter: "pli".to_owned(),
                    },
                ],
                ..Default::default()
            },
            payload_type: 117,
            ..Default::default()
        },
        RTPCodecType::Video,
    )?;
    let registry = register_default_interceptors(Registry::new(), &mut media_engine)?;
    let mut settings = SettingEngine::default();
    settings.set_network_types(vec![NetworkType::Udp6]);
    settings.set_ice_multicast_dns_mode(MulticastDnsMode::Disabled);
    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_setting_engine(settings)
        .build();
    let peer = Arc::new(api.new_peer_connection(RTCConfiguration::default()).await?);

    let video_track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_H264.to_owned(),
            ..Default::default()
        },
        "camera-video".to_owned(),
        device_id.to_owned(),
    ));
    let video_sender = peer
        .add_track(video_track.clone() as Arc<dyn TrackLocal + Send + Sync>)
        .await?;
    let audio_track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            ..Default::default()
        },
        "camera-audio".to_owned(),
        device_id.to_owned(),
    ));
    let audio_sender = peer
        .add_track(audio_track.clone() as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    let id = device_id.to_owned();
    peer.on_peer_connection_state_change(Box::new(move |state| {
        info!(device_id = %id, ?state, "camera-hub WebRTC state changed");
        Box::pin(async {})
    }));
    peer.set_remote_description(RTCSessionDescription::offer(offer_sdp)?)
        .await?;
    let answer = peer.create_answer(None).await?;
    let mut gathered = peer.gathering_complete_promise().await;
    peer.set_local_description(answer).await?;
    let _ = tokio::time::timeout(Duration::from_secs(3), gathered.recv()).await;
    let answer_sdp = peer
        .local_description()
        .await
        .context("WebRTC local description missing")?
        .sdp;

    let initial_video = if benchmark.is_some() {
        None
    } else {
        subscription.initial_video
    };
    let video_task = tokio::spawn(send_video(
        initial_video,
        subscription.video,
        video_track,
        benchmark,
    ));
    let audio_task = tokio::spawn(send_transcoded_audio(
        device_id.to_owned(),
        subscription.aac,
        audio_track,
    ));
    let video_rtcp_task =
        tokio::spawn(async move { while video_sender.read_rtcp().await.is_ok() {} });
    let audio_rtcp_task =
        tokio::spawn(async move { while audio_sender.read_rtcp().await.is_ok() {} });
    Ok((
        Session {
            peer,
            tasks: vec![video_task, audio_task, video_rtcp_task, audio_rtcp_task],
        },
        answer_sdp,
    ))
}

async fn send_video(
    initial: Option<EncodedFrame>,
    mut receiver: tokio::sync::broadcast::Receiver<EncodedFrame>,
    track: Arc<TrackLocalStaticSample>,
    benchmark: Option<(Arc<BenchmarkRegistry>, String)>,
) {
    let mut previous_pts = None;
    let mut benchmark_anchored = false;
    if let Some(frame) = initial {
        if write_video_sample(&track, &mut previous_pts, frame)
            .await
            .is_err()
        {
            return;
        }
    }
    loop {
        match receiver.recv().await {
            Ok(frame) => {
                if benchmark.is_some() && !benchmark_anchored {
                    if !frame.key {
                        continue;
                    }
                    if let Some((registry, session_id)) = benchmark.as_ref() {
                        registry.set_anchor(session_id, "webrtc", &frame);
                    }
                    benchmark_anchored = true;
                }
                if write_video_sample(&track, &mut previous_pts, frame)
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}

async fn write_video_sample(
    track: &TrackLocalStaticSample,
    previous_pts: &mut Option<i64>,
    frame: EncodedFrame,
) -> Result<()> {
    let duration_us = previous_pts
        .map(|previous| frame.pts_us.saturating_sub(previous))
        .filter(|value| (10_000..=200_000).contains(value))
        .unwrap_or(33_333);
    *previous_pts = Some(frame.pts_us);
    track
        .write_sample(&Sample {
            data: Bytes::copy_from_slice(&frame.data),
            duration: Duration::from_micros(duration_us as u64),
            ..Default::default()
        })
        .await?;
    Ok(())
}

async fn send_transcoded_audio(
    device_id: String,
    mut receiver: tokio::sync::broadcast::Receiver<EncodedFrame>,
    track: Arc<TrackLocalStaticSample>,
) {
    if let Err(error) = transcode_aac_to_opus(&device_id, &mut receiver, track).await {
        error!(
            device_id,
            error = %format!("{error:#}"),
            "AAC to Opus transcoder stopped"
        );
    }
}

async fn transcode_aac_to_opus(
    device_id: &str,
    receiver: &mut tokio::sync::broadcast::Receiver<EncodedFrame>,
    track: Arc<TrackLocalStaticSample>,
) -> Result<()> {
    let input_port = reserve_udp_port()?;
    let output = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let output_port = output.local_addr()?.port();
    let input_url = format!("udp://127.0.0.1:{input_port}?fifo_size=500000&overrun_nonfatal=1");
    let output_url = format!("rtp://127.0.0.1:{output_port}?pkt_size=1200");
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-fflags",
            "+nobuffer",
            "-flags",
            "low_delay",
            "-f",
            "aac",
            "-i",
            &input_url,
            "-map",
            "0:a:0",
            "-c:a",
            "libopus",
            "-application",
            "lowdelay",
            "-frame_duration",
            "20",
            "-b:a",
            "32k",
            "-vbr",
            "off",
            "-ar",
            "48000",
            "-ac",
            "1",
            "-payload_type",
            "111",
            "-metadata",
            "comment=camera-hub-opus",
            "-f",
            "rtp",
            &output_url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .context("start FFmpeg AAC to Opus transcoder")?;
    let input = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    input.connect((Ipv4Addr::LOCALHOST, input_port)).await?;
    tokio::time::sleep(Duration::from_millis(250)).await;
    let mut packet = vec![0u8; 2048];
    let mut process_check = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            frame = receiver.recv() => match frame {
                Ok(frame) => {
                    if let Err(error) = input.send(&frame.data).await
                        && error.kind() != io::ErrorKind::ConnectionRefused
                    {
                        return Err(error.into());
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(device_id, skipped, "AAC WebRTC input lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
            received = output.recv(&mut packet) => {
                let length = received?;
                let Some(payload) = rtp_payload(&packet[..length]) else {
                    continue;
                };
                if track
                    .write_sample(&Sample {
                        data: Bytes::copy_from_slice(payload),
                        duration: Duration::from_millis(20),
                        ..Default::default()
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
            _ = process_check.tick() => {
                if let Some(status) = child.try_wait()? {
                    bail!("FFmpeg AAC to Opus exited with {status}");
                }
            }
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
    Ok(())
}

fn reserve_udp_port() -> Result<u16> {
    let socket = StdUdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
    Ok(socket.local_addr()?.port())
}

fn rtp_payload(packet: &[u8]) -> Option<&[u8]> {
    if packet.len() < 12 || packet[0] >> 6 != 2 {
        return None;
    }
    let padding = packet[0] & 0x20 != 0;
    let extension = packet[0] & 0x10 != 0;
    let mut offset = 12 + usize::from(packet[0] & 0x0f) * 4;
    if extension {
        let words = u16::from_be_bytes(packet.get(offset + 2..offset + 4)?.try_into().ok()?);
        offset = offset.checked_add(4 + usize::from(words) * 4)?;
    }
    let padding = if padding {
        usize::from(*packet.last()?)
    } else {
        0
    };
    let end = packet.len().checked_sub(padding)?;
    (offset < end).then_some(&packet[offset..end])
}

#[cfg(test)]
mod tests {
    use super::rtp_payload;

    #[test]
    fn extracts_rtp_payload_with_csrc_extension_and_padding() {
        let packet = [
            0xb1, 111, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, // header + one CSRC
            0, 0, 0, 4, 0xbe, 0xde, 0, 1, // extension header
            1, 2, 3, 4, // extension data
            9, 8, 7, // payload
            0, 0, 3, // padding
        ];
        assert_eq!(rtp_payload(&packet), Some(&[9, 8, 7][..]));
    }
}
