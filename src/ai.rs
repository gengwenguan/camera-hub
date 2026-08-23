use crate::config::Config;
use crate::frames::FrameHub;
use crate::settings::HubSettingsStore;
use anyhow::{Context, Result, bail};
use chrono::{Duration as ChronoDuration, Timelike, Utc};
use ort::session::Session;
use ort::value::TensorRef;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};
use tracing::warn;

const INPUT: usize = 416;
const FRAME_BYTES: usize = INPUT * INPUT * 3;
const SOURCE_WIDTH: f32 = 640.0;
const SOURCE_HEIGHT: f32 = 480.0;
const MODEL_CONTENT_HEIGHT: f32 = 312.0;
const MAX_DETECTIONS: usize = 10;
const NMS_IOU_THRESHOLD: f32 = 0.45;
const YOLOX_STRIDES: [usize; 3] = [8, 16, 32];

#[derive(Clone, Debug)]
struct Detection {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    confidence: f32,
}

#[derive(Debug)]
struct PersonInference {
    confidence: f32,
    detections: Vec<Detection>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AiStatus {
    pub enabled: bool,
    pub available: bool,
    pub running: bool,
    pub model: String,
    pub interval_ms: u64,
    pub threshold: f32,
    pub min_person_area_ratio: f32,
    pub snapshot_quality: u8,
    pub inference_count: u64,
    pub detection_count: u64,
    pub last_device_id: String,
    pub last_confidence: f32,
    pub last_inference_ms: f64,
    pub last_snapshot: String,
    pub last_error: String,
}

pub struct AiService {
    status: Arc<Mutex<AiStatus>>,
    settings: Arc<HubSettingsStore>,
    data_dir: PathBuf,
    cleanup: Arc<Mutex<()>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AiService {
    pub fn start(
        config: &Config,
        settings: Arc<HubSettingsStore>,
        frames: Arc<FrameHub>,
    ) -> Result<Arc<Self>> {
        let initial = settings.current();
        let status = Arc::new(Mutex::new(AiStatus {
            enabled: initial.ai_enabled,
            available: false,
            running: false,
            model: config.ai_model.to_string_lossy().into_owned(),
            interval_ms: initial.ai_interval_ms,
            threshold: initial.ai_threshold,
            min_person_area_ratio: initial.ai_min_person_area_ratio,
            snapshot_quality: initial.ai_snapshot_quality,
            inference_count: 0,
            detection_count: 0,
            last_device_id: String::new(),
            last_confidence: 0.0,
            last_inference_ms: 0.0,
            last_snapshot: String::new(),
            last_error: String::new(),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let cleanup = Arc::new(Mutex::new(()));
        let worker = AiConfig {
            runtime: config.ai_runtime.clone(),
            model: config.ai_model.clone(),
            data_dir: config.data_dir.clone(),
        };
        let thread_status = status.clone();
        let thread_stop = stop.clone();
        let thread_settings = settings.clone();
        let thread_cleanup = cleanup.clone();
        let thread = Some(
            thread::Builder::new()
                .name("camera-hub-ai".to_owned())
                .spawn(move || {
                    ai_loop(
                        worker,
                        thread_settings,
                        frames,
                        thread_status,
                        thread_cleanup,
                        thread_stop,
                    )
                })?,
        );
        Ok(Arc::new(Self {
            status,
            settings,
            data_dir: config.data_dir.clone(),
            cleanup,
            stop,
            thread,
        }))
    }

    pub fn status(&self) -> AiStatus {
        self.status
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn clean_snapshots(&self) -> Result<SnapshotCleanup> {
        let _guard = self
            .cleanup
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let settings = self.settings.current();
        clean_all_snapshots(&self.data_dir, settings.ai_snapshot_max_count)
    }
}

impl Drop for AiService {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct AiConfig {
    runtime: PathBuf,
    model: PathBuf,
    data_dir: PathBuf,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SnapshotCleanup {
    pub removed_files: u64,
    pub removed_bytes: u64,
    pub remaining_files: u64,
}

fn ai_loop(
    config: AiConfig,
    settings: Arc<HubSettingsStore>,
    frames: Arc<FrameHub>,
    status: Arc<Mutex<AiStatus>>,
    cleanup: Arc<Mutex<()>>,
    stop: Arc<AtomicBool>,
) {
    if let Err(error) = run_ai(config, settings, frames, status.clone(), cleanup, stop) {
        let mut current = status.lock().unwrap_or_else(|error| error.into_inner());
        current.running = false;
        current.available = false;
        current.last_error = format!("{error:#}");
    }
}

fn run_ai(
    config: AiConfig,
    settings: Arc<HubSettingsStore>,
    frames: Arc<FrameHub>,
    status: Arc<Mutex<AiStatus>>,
    cleanup: Arc<Mutex<()>>,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    if !config.runtime.is_file() {
        bail!("ONNX Runtime is missing: {}", config.runtime.display());
    }
    if !config.model.is_file() {
        bail!("AI model is missing: {}", config.model.display());
    }
    let _ = ort::init_from(&config.runtime).map_err(ort_error)?.commit();
    let builder = Session::builder().map_err(ort_error)?;
    let mut builder = builder.with_intra_threads(4).map_err(ort_error)?;
    let mut session = builder.commit_from_file(&config.model).map_err(ort_error)?;
    {
        let mut current = status.lock().unwrap_or_else(|error| error.into_inner());
        current.available = true;
        current.running = true;
        current.last_error.clear();
    }

    let mut snapshots = BTreeMap::<String, Instant>::new();
    let mut processed = BTreeMap::<String, u32>::new();
    while !stop.load(Ordering::Acquire) {
        let started = Instant::now();
        let current_settings = settings.current();
        {
            let mut current = status.lock().unwrap_or_else(|error| error.into_inner());
            current.enabled = current_settings.ai_enabled;
            current.interval_ms = current_settings.ai_interval_ms;
            current.threshold = current_settings.ai_threshold;
            current.min_person_area_ratio = current_settings.ai_min_person_area_ratio;
            current.snapshot_quality = current_settings.ai_snapshot_quality;
        }
        if !current_settings.ai_enabled {
            sleep_interruptible(&stop, Duration::from_millis(250));
            continue;
        }
        for (device_id, keyframe) in frames.latest_keyframes() {
            if stop.load(Ordering::Acquire) {
                break;
            }
            if processed.get(&device_id) == Some(&keyframe.sequence) {
                continue;
            }
            processed.insert(device_id.clone(), keyframe.sequence);
            let frame = match decode_h264(&keyframe.data) {
                Ok(frame) => frame,
                Err(error) => {
                    update_error(&status, &device_id, error);
                    continue;
                }
            };
            let inference = Instant::now();
            let result = match infer_person(
                &mut session,
                &frame,
                current_settings.ai_threshold,
                current_settings.ai_min_person_area_ratio,
            ) {
                Ok(result) => result,
                Err(error) => {
                    update_error(&status, &device_id, error);
                    continue;
                }
            };
            let confidence = result.confidence;
            let inference_ms = inference.elapsed().as_secs_f64() * 1000.0;
            let mut snapshot = None;
            if !result.detections.is_empty()
                && snapshots.get(&device_id).is_none_or(|last| {
                    last.elapsed() >= Duration::from_secs(current_settings.ai_min_snapshot_seconds)
                })
            {
                match save_snapshot(
                    &config.data_dir,
                    &device_id,
                    &frame,
                    &result.detections,
                    current_settings.ai_snapshot_quality,
                ) {
                    Ok(path) => {
                        snapshots.insert(device_id.clone(), Instant::now());
                        let snapshot_root = config.data_dir.join(&device_id).join("snapshot");
                        let _guard = cleanup.lock().unwrap_or_else(|error| error.into_inner());
                        if let Err(error) = clean_snapshot_root(
                            &snapshot_root,
                            current_settings.ai_snapshot_max_count,
                        ) {
                            warn!(device_id, error = %format!("{error:#}"), "AI snapshot cleanup failed");
                        }
                        snapshot = Some(path);
                    }
                    Err(error) => update_error(&status, &device_id, error),
                }
            }
            let mut current = status.lock().unwrap_or_else(|error| error.into_inner());
            current.inference_count = current.inference_count.saturating_add(1);
            current.last_device_id.clone_from(&device_id);
            current.last_confidence = confidence;
            current.last_inference_ms = inference_ms;
            current.last_error.clear();
            if let Some(path) = snapshot {
                current.detection_count = current.detection_count.saturating_add(1);
                current.last_snapshot = path.to_string_lossy().into_owned();
            }
        }
        sleep_interruptible(
            &stop,
            Duration::from_millis(current_settings.ai_interval_ms)
                .saturating_sub(started.elapsed()),
        );
    }
    Ok(())
}

fn decode_h264(access_unit: &[u8]) -> Result<Vec<u8>> {
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "h264",
            "-i",
            "pipe:0",
        ])
        .args([
            "-frames:v",
            "1",
            "-vf",
            "scale=416:312,pad=416:416:0:0:color=0x727272",
            "-pix_fmt",
            "bgr24",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("start FFmpeg AI decoder")?;
    child
        .stdin
        .take()
        .context("open FFmpeg AI stdin")?
        .write_all(access_unit)
        .context("write H264 keyframe to FFmpeg")?;
    let output = child
        .wait_with_output()
        .context("wait for FFmpeg AI decoder")?;
    if !output.status.success() {
        bail!(
            "FFmpeg AI decode failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    if output.stdout.len() != FRAME_BYTES {
        bail!(
            "FFmpeg AI frame has {} bytes, expected {FRAME_BYTES}",
            output.stdout.len()
        );
    }
    Ok(output.stdout)
}

fn infer_person(
    session: &mut Session,
    frame: &[u8],
    threshold: f32,
    min_person_area_ratio: f32,
) -> Result<PersonInference> {
    let plane = INPUT * INPUT;
    let mut input = vec![0f32; FRAME_BYTES];
    for (index, pixel) in frame.chunks_exact(3).enumerate() {
        input[index] = f32::from(pixel[0]);
        input[plane + index] = f32::from(pixel[1]);
        input[plane * 2 + index] = f32::from(pixel[2]);
    }
    let tensor = TensorRef::from_array_view(([1usize, 3, INPUT, INPUT], input.as_slice()))
        .map_err(ort_error)?;
    let outputs = session.run(ort::inputs![tensor]).map_err(ort_error)?;
    let (_, values) = outputs[0].try_extract_tensor::<f32>().map_err(ort_error)?;
    decode_people(values, threshold, min_person_area_ratio)
}

fn decode_people(
    values: &[f32],
    threshold: f32,
    min_person_area_ratio: f32,
) -> Result<PersonInference> {
    const ATTRIBUTES: usize = 85;
    if values.len() % ATTRIBUTES != 0 {
        bail!("unexpected YOLOX output length {}", values.len());
    }
    let scale_x = SOURCE_WIDTH / INPUT as f32;
    let scale_y = SOURCE_HEIGHT / MODEL_CONTENT_HEIGHT;
    let mut confidence = 0.0f32;
    let prediction_count = values.len() / ATTRIBUTES;
    let expected_count = YOLOX_STRIDES
        .iter()
        .map(|stride| (INPUT / stride).pow(2))
        .sum::<usize>();
    if prediction_count != expected_count {
        bail!("unexpected YOLOX prediction count {prediction_count}, expected {expected_count}");
    }
    let mut detections = values
        .chunks_exact(ATTRIBUTES)
        .enumerate()
        .filter_map(|(index, prediction)| {
            let score = prediction[4] * prediction[5];
            confidence = confidence.max(score);
            if score < threshold {
                return None;
            }
            let (center_x, center_y, width, height) = decode_yolox_box(index, prediction)?;
            let left = (center_x - width * 0.5).clamp(0.0, INPUT as f32);
            let top = (center_y - height * 0.5).clamp(0.0, MODEL_CONTENT_HEIGHT);
            let right = (center_x + width * 0.5).clamp(0.0, INPUT as f32);
            let bottom = (center_y + height * 0.5).clamp(0.0, MODEL_CONTENT_HEIGHT);
            let detection = Detection {
                x: left * scale_x,
                y: top * scale_y,
                width: (right - left) * scale_x,
                height: (bottom - top) * scale_y,
                confidence: score,
            };
            (right > left + 1.0
                && bottom > top + 1.0
                && person_area_ratio(&detection) >= min_person_area_ratio)
                .then_some(detection)
        })
        .collect::<Vec<_>>();
    detections.sort_by(|left, right| right.confidence.total_cmp(&left.confidence));
    let mut selected = Vec::<Detection>::new();
    for detection in detections {
        if selected
            .iter()
            .all(|existing| intersection_over_union(&detection, existing) < NMS_IOU_THRESHOLD)
        {
            selected.push(detection);
            if selected.len() == MAX_DETECTIONS {
                break;
            }
        }
    }
    Ok(PersonInference {
        confidence,
        detections: selected,
    })
}

fn person_area_ratio(detection: &Detection) -> f32 {
    detection.width * detection.height / (SOURCE_WIDTH * SOURCE_HEIGHT)
}

fn decode_yolox_box(index: usize, prediction: &[f32]) -> Option<(f32, f32, f32, f32)> {
    let mut offset = 0usize;
    for stride in YOLOX_STRIDES {
        let grid_size = INPUT / stride;
        let cells = grid_size * grid_size;
        if index < offset + cells {
            let cell = index - offset;
            let grid_x = cell % grid_size;
            let grid_y = cell / grid_size;
            let stride = stride as f32;
            return Some((
                (prediction[0] + grid_x as f32) * stride,
                (prediction[1] + grid_y as f32) * stride,
                prediction[2].exp() * stride,
                prediction[3].exp() * stride,
            ));
        }
        offset += cells;
    }
    None
}

fn intersection_over_union(left: &Detection, right: &Detection) -> f32 {
    let x1 = left.x.max(right.x);
    let y1 = left.y.max(right.y);
    let x2 = (left.x + left.width).min(right.x + right.width);
    let y2 = (left.y + left.height).min(right.y + right.height);
    let intersection = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let union = left.width * left.height + right.width * right.height - intersection;
    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn save_snapshot(
    data_dir: &Path,
    device_id: &str,
    frame: &[u8],
    detections: &[Detection],
    quality: u8,
) -> Result<PathBuf> {
    let china = Utc::now() + ChronoDuration::hours(8);
    let directory = data_dir
        .join(device_id)
        .join("snapshot")
        .join(china.format("%Y%m%d").to_string());
    std::fs::create_dir_all(&directory)?;
    let name = format!(
        "{}_{:03}.jpg",
        china.format("%Y%m%d_%H%M%S"),
        china.nanosecond() / 1_000_000
    );
    let path = directory.join(name);
    let filter = snapshot_filter(detections);
    let mut child = Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pixel_format",
            "bgr24",
            "-video_size",
            "416x416",
            "-i",
            "pipe:0",
            "-frames:v",
            "1",
            "-vf",
        ])
        .arg(filter)
        .arg("-q:v")
        .arg(jpeg_qscale(quality).to_string())
        .arg("-y")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn FFmpeg AI snapshot")?;
    child
        .stdin
        .take()
        .context("FFmpeg AI snapshot stdin missing")?
        .write_all(frame)?;
    let output = child
        .wait_with_output()
        .context("wait for FFmpeg AI snapshot")?;
    if !output.status.success() {
        bail!(
            "FFmpeg AI snapshot failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(path)
}

#[derive(Debug)]
struct SnapshotFile {
    path: PathBuf,
    modified: SystemTime,
    size: u64,
}

fn clean_all_snapshots(data_dir: &Path, max_count: u64) -> Result<SnapshotCleanup> {
    let mut total = SnapshotCleanup::default();
    let devices = match fs::read_dir(data_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(total),
        Err(error) => return Err(error.into()),
    };
    for device in devices.flatten().filter(|entry| entry.path().is_dir()) {
        let cleanup = clean_snapshot_root(&device.path().join("snapshot"), max_count)?;
        total.removed_files = total.removed_files.saturating_add(cleanup.removed_files);
        total.removed_bytes = total.removed_bytes.saturating_add(cleanup.removed_bytes);
        total.remaining_files = total
            .remaining_files
            .saturating_add(cleanup.remaining_files);
    }
    Ok(total)
}

fn clean_snapshot_root(root: &Path, max_count: u64) -> Result<SnapshotCleanup> {
    let days = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SnapshotCleanup::default());
        }
        Err(error) => return Err(error.into()),
    };
    let mut files = Vec::<SnapshotFile>::new();
    for day in days.flatten().filter(|entry| entry.path().is_dir()) {
        for entry in fs::read_dir(day.path())?.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if !path.is_file() || !managed_snapshot_name(&name.to_string_lossy()) {
                continue;
            }
            let metadata = entry.metadata()?;
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            files.push(SnapshotFile {
                path,
                modified,
                size: metadata.len(),
            });
        }
    }
    files.sort_by(|left, right| {
        left.modified
            .cmp(&right.modified)
            .then_with(|| left.path.cmp(&right.path))
    });

    let mut cleanup = SnapshotCleanup::default();
    let max_count = usize::try_from(max_count).unwrap_or(usize::MAX);
    let remove_count = files.len().saturating_sub(max_count);
    for file in files.iter().take(remove_count) {
        remove_snapshot_file(file, &mut cleanup)?;
    }
    cleanup.remaining_files = u64::try_from(files.len() - remove_count).unwrap_or(u64::MAX);
    remove_empty_snapshot_days(root);
    Ok(cleanup)
}

fn remove_snapshot_file(file: &SnapshotFile, cleanup: &mut SnapshotCleanup) -> Result<()> {
    match fs::remove_file(&file.path) {
        Ok(()) => {
            cleanup.removed_files = cleanup.removed_files.saturating_add(1);
            cleanup.removed_bytes = cleanup.removed_bytes.saturating_add(file.size);
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn remove_empty_snapshot_days(root: &Path) {
    let Ok(days) = fs::read_dir(root) else {
        return;
    };
    for day in days.flatten().filter(|entry| entry.path().is_dir()) {
        let _ = fs::remove_dir(day.path());
    }
}

fn managed_snapshot_name(name: &str) -> bool {
    name.ends_with(".jpg")
        && name
            .get(..8)
            .is_some_and(|date| date.bytes().all(|byte| byte.is_ascii_digit()))
}

fn jpeg_qscale(quality: u8) -> u8 {
    let quality = u16::from(quality.clamp(1, 100));
    let distance = 100 - quality;
    u8::try_from(2 + (distance * 29 + 49) / 99).unwrap_or(31)
}

fn snapshot_filter(detections: &[Detection]) -> String {
    let mut filters = vec!["crop=416:312:0:0".to_owned(), "scale=640:480".to_owned()];
    for detection in detections {
        let x = detection.x.round().clamp(0.0, SOURCE_WIDTH - 1.0) as i32;
        let y = detection.y.round().clamp(0.0, SOURCE_HEIGHT - 1.0) as i32;
        let width = detection.width.round().clamp(1.0, SOURCE_WIDTH - x as f32) as i32;
        let height = detection
            .height
            .round()
            .clamp(1.0, SOURCE_HEIGHT - y as f32) as i32;
        filters.push(format!(
            "drawbox=x={x}:y={y}:w={width}:h={height}:color=0x36e6a5:t=3"
        ));
        filters.push(format!(
            "drawtext=x={x}:y=max(0\\,{y}-22):text='person {:.2}':\
             expansion=none:fontsize=18:fontcolor=white:box=1:boxcolor=black@0.65",
            detection.confidence
        ));
    }
    filters.join(",")
}

fn update_error(status: &Mutex<AiStatus>, device_id: &str, error: anyhow::Error) {
    let mut current = status.lock().unwrap_or_else(|error| error.into_inner());
    current.last_device_id = device_id.to_owned();
    current.last_error = format!("{error:#}");
}

fn ort_error(error: impl std::fmt::Display) -> anyhow::Error {
    anyhow::anyhow!(error.to_string())
}

fn sleep_interruptible(stop: &AtomicBool, duration: Duration) {
    let mut remaining = duration;
    while !stop.load(Ordering::Acquire) && !remaining.is_zero() {
        let slice = remaining.min(Duration::from_millis(100));
        thread::sleep(slice);
        remaining = remaining.saturating_sub(slice);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_person_box_and_confidence() {
        let mut values = vec![0.0f32; 85 * 3549];
        let index = 10 * 52 + 20;
        let prediction = &mut values[index * 85..(index + 1) * 85];
        prediction[0] = 0.5;
        prediction[1] = 0.5;
        prediction[2] = 80.0f32.ln() - 8.0f32.ln();
        prediction[3] = 160.0f32.ln() - 8.0f32.ln();
        prediction[4] = 0.9;
        prediction[5] = 0.7;
        let result = decode_people(&values, 0.3, 0.02).unwrap();
        assert!((result.confidence - 0.63).abs() < 0.0001);
        assert_eq!(result.detections.len(), 1);
        let detection = &result.detections[0];
        assert!((detection.x - 190.769).abs() < 0.01);
        assert!((detection.y - 6.153).abs() < 0.01);
        assert!((detection.width - 123.076).abs() < 0.01);
        assert!((detection.height - 246.153).abs() < 0.01);
    }

    #[test]
    fn suppresses_overlapping_person_boxes() {
        let mut values = vec![0.0f32; 85 * 3549];
        for index in [10 * 52 + 20, 10 * 52 + 21] {
            let prediction = &mut values[index * 85..(index + 1) * 85];
            prediction[0] = if index % 52 == 20 { 0.5 } else { -0.5 };
            prediction[1] = 0.5;
            prediction[2] = 100.0f32.ln() - 8.0f32.ln();
            prediction[3] = 140.0f32.ln() - 8.0f32.ln();
            prediction[4] = 0.9;
            prediction[5] = 0.8;
        }
        values[(10 * 52 + 21) * 85 + 4] = 0.8;
        let result = decode_people(&values, 0.3, 0.02).unwrap();
        assert_eq!(result.detections.len(), 1);
    }

    #[test]
    fn rejects_person_boxes_below_minimum_frame_area() {
        let mut values = vec![0.0f32; 85 * 3549];
        let index = 10 * 52 + 20;
        let prediction = &mut values[index * 85..(index + 1) * 85];
        prediction[0] = 0.5;
        prediction[1] = 0.5;
        prediction[2] = 32.0f32.ln() - 8.0f32.ln();
        prediction[3] = 64.0f32.ln() - 8.0f32.ln();
        prediction[4] = 0.9;
        prediction[5] = 0.7;

        let result = decode_people(&values, 0.3, 0.02).unwrap();

        assert!(result.detections.is_empty());
    }

    #[test]
    fn keeps_person_box_at_minimum_frame_area() {
        let detection = Detection {
            x: 10.0,
            y: 20.0,
            width: SOURCE_WIDTH * 0.1,
            height: SOURCE_HEIGHT * 0.2,
            confidence: 0.81,
        };

        assert!((person_area_ratio(&detection) - 0.02).abs() < f32::EPSILON);
        assert!(person_area_ratio(&detection) >= 0.02);
    }

    #[test]
    fn builds_snapshot_box_and_label_filter() {
        let filter = snapshot_filter(&[Detection {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 200.0,
            confidence: 0.81,
        }]);
        assert!(filter.starts_with("crop=416:312:0:0,scale=640:480"));
        assert!(filter.contains("drawbox=x=10:y=20:w=100:h=200"));
        assert!(filter.contains("text='person 0.81'"));
    }

    #[test]
    fn limits_snapshot_count_without_removing_unknown_files() {
        let root = temporary_snapshot_root("count");
        let day = root.join("20260814");
        fs::create_dir_all(&day).unwrap();
        for name in [
            "20260814_100000_000.jpg",
            "20260814_100001_000.jpg",
            "20260814_100002_000.jpg",
        ] {
            fs::write(day.join(name), name.as_bytes()).unwrap();
            thread::sleep(Duration::from_millis(2));
        }
        fs::write(day.join("notes.txt"), b"keep").unwrap();

        let cleanup = clean_snapshot_root(&root, 2).unwrap();

        assert_eq!(cleanup.removed_files, 1);
        assert_eq!(cleanup.remaining_files, 2);
        assert!(day.join("notes.txt").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn maps_snapshot_quality_to_ffmpeg_jpeg_qscale() {
        assert_eq!(jpeg_qscale(100), 2);
        assert_eq!(jpeg_qscale(95), 3);
        assert_eq!(jpeg_qscale(1), 31);
    }

    fn temporary_snapshot_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "camera-hub-ai-snapshot-{label}-{}-{nonce}",
            std::process::id()
        ))
    }
}
