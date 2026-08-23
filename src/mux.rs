use crate::benchmark::BenchmarkAnchor;
use crate::frames::{EncodedFrame, FrameHub, FrameSubscription};
use crate::live::LiveStreams;
use crate::media::MediaStore;
use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket as StdUdpSocket};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::UdpSocket;
use tokio::process::{ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

const MAX_MP4_BOX: usize = 8 * 1024 * 1024;
const UDP_CHUNK: usize = 60_000;

pub struct MediaMuxers {
    frames: Arc<FrameHub>,
    media: Arc<MediaStore>,
    live: Arc<LiveStreams>,
    workers: Mutex<BTreeMap<String, JoinHandle<()>>>,
}

impl MediaMuxers {
    pub fn new(frames: Arc<FrameHub>, media: Arc<MediaStore>, live: Arc<LiveStreams>) -> Self {
        Self {
            frames,
            media,
            live,
            workers: Mutex::new(BTreeMap::new()),
        }
    }

    pub async fn ensure(&self, device_id: &str) {
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
        let media = self.media.clone();
        let live = self.live.clone();
        let worker_id = id.clone();
        workers.insert(
            id,
            tokio::spawn(async move {
                if let Err(error) = run_muxer(&worker_id, subscription, media, live).await {
                    error!(device_id = %worker_id, error = %format!("{error:#}"), "media muxer stopped");
                }
            }),
        );
    }
}

async fn run_muxer(
    device_id: &str,
    mut subscription: FrameSubscription,
    media: Arc<MediaStore>,
    live: Arc<LiveStreams>,
) -> Result<()> {
    let (video_port, audio_port) = reserve_udp_ports()?;
    let video = connected_udp_sender(video_port).await?;
    let audio = connected_udp_sender(audio_port).await?;
    let video_url = format!("udp://127.0.0.1:{video_port}?fifo_size=1000000&overrun_nonfatal=1");
    let audio_url = format!("udp://127.0.0.1:{audio_port}?fifo_size=1000000&overrun_nonfatal=1");
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-fflags",
            "+genpts",
            "-thread_queue_size",
            "512",
            "-framerate",
            "30",
            "-f",
            "h264",
            "-i",
            &video_url,
            "-thread_queue_size",
            "512",
            "-f",
            "aac",
            "-i",
            &audio_url,
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c",
            "copy",
            "-bsf:a",
            "aac_adtstoasc",
            "-metadata",
            "comment=camera-hub-mux",
            "-video_track_timescale",
            "90000",
            "-max_interleave_delta",
            "1000000",
            "-movflags",
            "delay_moov+default_base_moof+frag_keyframe",
            "-frag_duration",
            "1000000",
            "-flush_packets",
            "1",
            "-f",
            "mp4",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .context("start FFmpeg H264/AAC muxer")?;
    let stdout = child.stdout.take().context("open FFmpeg muxer stdout")?;
    let keyframes = Arc::new(std::sync::Mutex::new(VecDeque::<EncodedFrame>::new()));
    let output_device = device_id.to_owned();
    let output_keyframes = keyframes.clone();
    let output = tokio::spawn(async move {
        if let Err(error) =
            consume_fmp4(stdout, &output_device, media, live, output_keyframes).await
        {
            error!(
                device_id = %output_device,
                error = %format!("{error:#}"),
                "read FFmpeg fMP4 output failed"
            );
        }
    });

    tokio::time::sleep(Duration::from_millis(250)).await;
    if let Some(frame) = subscription.initial_video.take() {
        remember_keyframe(&keyframes, &frame);
        send_udp_frame(&video, &frame).await?;
    }
    let mut process_check = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            frame = subscription.video.recv() => match frame {
                Ok(frame) => {
                    remember_keyframe(&keyframes, &frame);
                    send_udp_frame(&video, &frame).await?
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(device_id, skipped, "video mux input lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
            frame = subscription.aac.recv() => match frame {
                Ok(frame) => send_udp_frame(&audio, &frame).await?,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(device_id, skipped, "AAC mux input lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
            _ = process_check.tick() => {
                if let Some(status) = child.try_wait()? {
                    bail!("FFmpeg muxer exited with {status}");
                }
            }
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
    let _ = output.await;
    Ok(())
}

fn reserve_udp_ports() -> Result<(u16, u16)> {
    let video = StdUdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
    let audio = StdUdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
    Ok((video.local_addr()?.port(), audio.local_addr()?.port()))
}

async fn connected_udp_sender(port: u16) -> Result<UdpSocket> {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    socket.connect((Ipv4Addr::LOCALHOST, port)).await?;
    Ok(socket)
}

async fn send_udp_frame(socket: &UdpSocket, frame: &EncodedFrame) -> Result<()> {
    for chunk in frame.data.chunks(UDP_CHUNK) {
        if let Err(error) = socket.send(chunk).await {
            if error.kind() == io::ErrorKind::ConnectionRefused {
                continue;
            }
            return Err(error.into());
        }
    }
    Ok(())
}

async fn consume_fmp4(
    mut output: ChildStdout,
    device_id: &str,
    media: Arc<MediaStore>,
    live: Arc<LiveStreams>,
    keyframes: Arc<std::sync::Mutex<VecDeque<EncodedFrame>>>,
) -> Result<()> {
    let mut init = Vec::new();
    let mut initialized = false;
    let mut fragment = None::<Vec<u8>>;
    while let Some(mp4_box) = read_mp4_box(&mut output).await? {
        let kind = mp4_box.get(4..8).context("MP4 box type is missing")?;
        if kind == b"moof" {
            if !initialized {
                if init.is_empty() {
                    bail!("FFmpeg emitted moof before init");
                }
                let store = media.clone();
                let id = device_id.to_owned();
                let body = init.clone();
                tokio::task::spawn_blocking(move || store.set_init(&id, &body)).await??;
                live.set_init(device_id, &init);
                initialized = true;
            }
            fragment = Some(mp4_box);
            continue;
        }
        if let Some(current) = fragment.as_mut() {
            current.extend_from_slice(&mp4_box);
            if kind == b"mdat" {
                let completed = fragment.take().context("fragment disappeared")?;
                let store = media.clone();
                let id = device_id.to_owned();
                let body = completed.clone();
                tokio::task::spawn_blocking(move || store.write_fragment(&id, &body)).await??;
                let anchor = keyframes
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .pop_front()
                    .map(|frame| BenchmarkAnchor {
                        sequence: frame.sequence,
                        pts_us: frame.pts_us,
                        capture_epoch_us: frame.capture_epoch_us,
                        source_clock: frame.source_clock,
                        media_time_us: video_tfdt(&completed)
                            .and_then(|value| i64::try_from(value).ok())
                            .map(|value| value.saturating_mul(1_000_000) / 90_000),
                    });
                live.broadcast(device_id, &completed, anchor);
            }
        } else if !initialized {
            init.extend_from_slice(&mp4_box);
            if init.len() > MAX_MP4_BOX {
                bail!("fMP4 init is too large");
            }
        }
    }
    info!(device_id, "FFmpeg fMP4 output ended");
    Ok(())
}

fn video_tfdt(data: &[u8]) -> Option<u64> {
    let mut position = 0;
    while position + 8 <= data.len() {
        let (size, kind, header) = mp4_box_header(data, position, data.len())?;
        if kind == *b"moof" {
            let mut child = position + header;
            let end = position + size;
            while child + 8 <= end {
                let (child_size, child_kind, child_header) = mp4_box_header(data, child, end)?;
                if child_kind == *b"traf" {
                    if let Some(value) =
                        traf_video_tfdt(data, child + child_header, child + child_size)
                    {
                        return Some(value);
                    }
                }
                child += child_size;
            }
        }
        position += size;
    }
    None
}

fn traf_video_tfdt(data: &[u8], start: usize, end: usize) -> Option<u64> {
    let mut track_id = None;
    let mut tfdt = None;
    let mut position = start;
    while position + 8 <= end {
        let (size, kind, header) = mp4_box_header(data, position, end)?;
        if kind == *b"tfhd" {
            track_id = read_u32(data, position + header + 4);
        } else if kind == *b"tfdt" {
            let version = *data.get(position + header)?;
            let offset = position + header + 4;
            tfdt = if version == 1 {
                read_u64(data, offset)
            } else {
                read_u32(data, offset).map(u64::from)
            };
        }
        position += size;
    }
    (track_id == Some(1)).then_some(tfdt).flatten()
}

fn mp4_box_header(data: &[u8], offset: usize, end: usize) -> Option<(usize, [u8; 4], usize)> {
    let short = read_u32(data, offset)? as usize;
    let kind = data.get(offset + 4..offset + 8)?.try_into().ok()?;
    let (size, header) = if short == 1 {
        (usize::try_from(read_u64(data, offset + 8)?).ok()?, 16)
    } else {
        (short, 8)
    };
    if size < header || offset.checked_add(size)? > end {
        return None;
    }
    Some((size, kind, header))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_be_bytes(
        data.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

fn remember_keyframe(keyframes: &std::sync::Mutex<VecDeque<EncodedFrame>>, frame: &EncodedFrame) {
    if !frame.key {
        return;
    }
    let mut keyframes = keyframes.lock().unwrap_or_else(|error| error.into_inner());
    if keyframes.len() >= 16 {
        keyframes.pop_front();
    }
    keyframes.push_back(frame.clone());
}

async fn read_mp4_box(output: &mut ChildStdout) -> Result<Option<Vec<u8>>> {
    let mut header = [0u8; 8];
    if let Err(error) = output.read_exact(&mut header).await {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            return Ok(None);
        }
        return Err(error.into());
    }
    let short_size = u32::from_be_bytes(header[..4].try_into()?) as u64;
    let (size, header_size) = if short_size == 1 {
        let mut extended = [0u8; 8];
        output.read_exact(&mut extended).await?;
        (u64::from_be_bytes(extended), 16usize)
    } else {
        (short_size, 8usize)
    };
    let size = usize::try_from(size)?;
    if size < header_size || size > MAX_MP4_BOX {
        bail!("invalid MP4 box size {size}");
    }
    let mut result = Vec::with_capacity(size);
    result.extend_from_slice(&header);
    if header_size == 16 {
        result.extend_from_slice(&(size as u64).to_be_bytes());
    }
    result.resize(size, 0);
    output.read_exact(&mut result[header_size..]).await?;
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::video_tfdt;

    fn mp4_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let size = u32::try_from(8 + payload.len()).unwrap();
        [size.to_be_bytes().as_slice(), kind, payload].concat()
    }

    fn traf(track_id: u32, decode_time: u64) -> Vec<u8> {
        let mut tfhd_payload = vec![0, 0, 0, 0];
        tfhd_payload.extend_from_slice(&track_id.to_be_bytes());
        let tfhd = mp4_box(b"tfhd", &tfhd_payload);
        let mut tfdt_payload = vec![1, 0, 0, 0];
        tfdt_payload.extend_from_slice(&decode_time.to_be_bytes());
        let tfdt = mp4_box(b"tfdt", &tfdt_payload);
        mp4_box(b"traf", &[tfhd, tfdt].concat())
    }

    #[test]
    fn reads_video_track_decode_time_from_fragment() {
        let audio = traf(2, 45_000);
        let video = traf(1, 180_000);
        let fragment = mp4_box(b"moof", &[audio, video].concat());
        assert_eq!(video_tfdt(&fragment), Some(180_000));
    }

    #[test]
    fn rejects_truncated_fragment_boxes() {
        let mut fragment = mp4_box(b"moof", &traf(1, 90_000));
        fragment.pop();
        assert_eq!(video_tfdt(&fragment), None);
    }
}
