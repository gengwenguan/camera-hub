use crate::settings::HubSettingsStore;
use anyhow::{Context, Result, bail};
use chrono::{Duration as ChronoDuration, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize)]
pub struct MediaStatus {
    pub device_id: String,
    pub initialized: bool,
    pub recording: bool,
    pub file: String,
    pub bytes: u64,
    pub fragments: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecordDay {
    pub date: String,
    pub files: u64,
    pub bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Recording {
    pub name: String,
    pub size: u64,
    pub modified_epoch: u64,
    pub time: String,
    pub active: bool,
}

pub struct MediaStore {
    root: PathBuf,
    settings: Arc<HubSettingsStore>,
    recorders: Mutex<BTreeMap<String, DeviceRecorder>>,
}

impl MediaStore {
    pub fn new(root: PathBuf, settings: Arc<HubSettingsStore>) -> Result<Self> {
        fs::create_dir_all(&root)
            .with_context(|| format!("create media root {}", root.display()))?;
        Ok(Self {
            root,
            settings,
            recorders: Mutex::new(BTreeMap::new()),
        })
    }

    pub fn set_init(&self, device_id: &str, data: &[u8]) -> Result<()> {
        if data.len() < 8 || data.get(4..8) != Some(b"ftyp") {
            bail!("media init does not start with ftyp");
        }
        let mut recorders = self
            .recorders
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let recorder = recorders
            .entry(device_id.to_owned())
            .or_insert_with(|| DeviceRecorder::new(self.root.join(device_id)));
        if recorder.file.is_some() {
            recorder.stop()?;
        }
        recorder.set_init(data);
        Ok(())
    }

    pub fn write_fragment(&self, device_id: &str, data: &[u8]) -> Result<MediaStatus> {
        if data.len() < 8 || data.get(4..8) != Some(b"moof") {
            bail!("media fragment does not start with moof");
        }
        let mut recorders = self
            .recorders
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let recorder = recorders
            .entry(device_id.to_owned())
            .or_insert_with(|| DeviceRecorder::new(self.root.join(device_id)));
        let settings = self.settings.current();
        if settings.record_enabled {
            recorder.write_fragment(data, settings.segment_seconds, true)?;
        } else {
            recorder.stop()?;
        }
        Ok(recorder.status(device_id))
    }

    pub fn statuses(&self) -> Vec<MediaStatus> {
        self.recorders
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .map(|(device_id, recorder)| recorder.status(device_id))
            .collect()
    }

    pub fn record_days(&self, device_id: &str) -> Result<Vec<RecordDay>> {
        let root = self.root.join(device_id).join("record");
        let entries = match fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut days = entries
            .flatten()
            .filter_map(|entry| {
                let file_type = entry.file_type().ok()?;
                let date = entry.file_name().to_string_lossy().into_owned();
                if !file_type.is_dir() || !valid_date(&date) {
                    return None;
                }
                let (files, bytes) = recording_totals(&entry.path(), &date);
                Some(RecordDay { date, files, bytes })
            })
            .collect::<Vec<_>>();
        days.sort_by(|left, right| right.date.cmp(&left.date));
        Ok(days)
    }

    pub fn recordings(&self, device_id: &str, date: &str) -> Result<Vec<Recording>> {
        if !valid_date(date) {
            bail!("invalid record date");
        }
        let active = self
            .recorders
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(device_id)
            .map(|recorder| recorder.current.clone());
        let root = self.root.join(device_id).join("record").join(date);
        let entries = match fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut recordings = entries
            .flatten()
            .filter_map(|entry| {
                let file_type = entry.file_type().ok()?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if !file_type.is_file() || !valid_recording_name(&name, date) {
                    return None;
                }
                let metadata = entry.metadata().ok()?;
                Some(Recording {
                    time: format!(
                        "{}:{}:{}",
                        name.get(9..11)?,
                        name.get(11..13)?,
                        name.get(13..15)?
                    ),
                    active: active.as_ref().is_some_and(|path| path == &entry.path()),
                    name,
                    size: metadata.len(),
                    modified_epoch: metadata
                        .modified()
                        .unwrap_or(UNIX_EPOCH)
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                })
            })
            .collect::<Vec<_>>();
        recordings.sort_by(|left, right| right.name.cmp(&left.name));
        Ok(recordings)
    }

    pub fn record_path(&self, device_id: &str, date: &str, name: &str) -> Result<PathBuf> {
        if !valid_date(date) || !valid_record_file(name, date) {
            bail!("invalid record path");
        }
        Ok(self
            .root
            .join(device_id)
            .join("record")
            .join(date)
            .join(name))
    }

    pub fn clean(&self) -> Result<()> {
        let current = self
            .recorders
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .filter(|recorder| !recorder.current.as_os_str().is_empty())
            .map(|recorder| recorder.current.clone())
            .collect::<BTreeSet<_>>();
        let settings = self.settings.current();
        clean_files(
            &self.root,
            &current,
            settings.retain_days,
            settings.max_bytes,
        )
    }
}

fn recording_totals(root: &Path, date: &str) -> (u64, u64) {
    fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !file_type.is_file() || !valid_recording_name(&name, date) {
                return None;
            }
            Some(entry.metadata().ok()?.len())
        })
        .fold((0u64, 0u64), |(files, bytes), size| {
            (files.saturating_add(1), bytes.saturating_add(size))
        })
}

fn valid_date(value: &str) -> bool {
    value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_recording_name(name: &str, date: &str) -> bool {
    name.len() == 19
        && name.get(..8) == Some(date)
        && name.as_bytes().get(8) == Some(&b'_')
        && name
            .as_bytes()
            .get(9..15)
            .is_some_and(|value| value.iter().all(u8::is_ascii_digit))
        && name.get(15..) == Some(".mp4")
}

fn valid_record_file(name: &str, date: &str) -> bool {
    valid_recording_name(name, date)
        || name
            .strip_suffix(".idx")
            .is_some_and(|recording| valid_recording_name(recording, date))
}

struct DeviceRecorder {
    root: PathBuf,
    init: Vec<u8>,
    file: Option<BufWriter<File>>,
    index: Option<BufWriter<File>>,
    index_first: bool,
    current: PathBuf,
    bytes: u64,
    fragments: u64,
    started: Instant,
    tfdt_base: BTreeMap<u32, u64>,
    persistent: bool,
}

impl DeviceRecorder {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            init: Vec::new(),
            file: None,
            index: None,
            index_first: true,
            current: PathBuf::new(),
            bytes: 0,
            fragments: 0,
            started: Instant::now(),
            tfdt_base: BTreeMap::new(),
            persistent: false,
        }
    }

    fn set_init(&mut self, data: &[u8]) {
        self.init.clear();
        self.init.extend_from_slice(data);
    }

    fn write_fragment(
        &mut self,
        data: &[u8],
        segment_seconds: u64,
        persistent: bool,
    ) -> Result<()> {
        if self.init.is_empty() {
            bail!("media init has not been received");
        }
        if self.file.is_some() && self.persistent != persistent {
            self.stop()?;
        }
        if self.file.is_none() || self.started.elapsed() >= Duration::from_secs(segment_seconds) {
            self.roll(persistent)?;
        }

        let mut fragment = data.to_vec();
        let tfdt = rewrite_tfdt(&mut fragment, &mut self.tfdt_base).unwrap_or(0);
        let offset = self.bytes;
        self.file
            .as_mut()
            .context("record file is not open")?
            .write_all(&fragment)?;
        self.file
            .as_mut()
            .context("record file is not open")?
            .flush()?;
        self.bytes = self.bytes.saturating_add(fragment.len() as u64);
        self.fragments = self.fragments.saturating_add(1);

        if let Some(index) = self.index.as_mut() {
            if !self.index_first {
                index.write_all(b",")?;
            }
            write!(index, "[{tfdt},{offset},{}]", fragment.len())?;
            index.flush()?;
            self.index_first = false;
        }
        Ok(())
    }

    fn roll(&mut self, persistent: bool) -> Result<()> {
        self.close_current()?;
        if !self.persistent && !self.current.as_os_str().is_empty() {
            remove_pair(&self.current);
        }
        let china = Utc::now() + ChronoDuration::hours(8);
        let (directory, name) = if persistent {
            (
                self.root
                    .join("record")
                    .join(china.format("%Y%m%d").to_string()),
                china.format("%Y%m%d_%H%M%S.mp4").to_string(),
            )
        } else {
            (self.root.join("spool"), "ai-live.mp4".to_owned())
        };
        fs::create_dir_all(&directory)?;
        let path = directory.join(name);

        let mut file = BufWriter::with_capacity(64 * 1024, File::create(&path)?);
        file.write_all(&self.init)?;
        let index_path = PathBuf::from(format!("{}.idx", path.display()));
        let mut index = BufWriter::new(
            OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(index_path)?,
        );
        write!(
            index,
            "{{\"v\":1,\"ts\":90000,\"init\":{},\"frags\":[",
            self.init.len()
        )?;
        index.flush()?;

        self.file = Some(file);
        self.index = Some(index);
        self.index_first = true;
        self.current = path;
        self.bytes = self.init.len() as u64;
        self.fragments = 0;
        self.started = Instant::now();
        self.tfdt_base.clear();
        self.persistent = persistent;
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.close_current()?;
        if !self.persistent && !self.current.as_os_str().is_empty() {
            remove_pair(&self.current);
            self.current.clear();
        }
        Ok(())
    }

    fn close_current(&mut self) -> Result<()> {
        if let Some(mut index) = self.index.take() {
            index.write_all(b"]}\n")?;
            index.flush()?;
        }
        if let Some(mut file) = self.file.take() {
            file.flush()?;
        }
        Ok(())
    }

    fn status(&self, device_id: &str) -> MediaStatus {
        MediaStatus {
            device_id: device_id.to_owned(),
            initialized: !self.init.is_empty(),
            recording: self.persistent && self.file.is_some(),
            file: self.current.to_string_lossy().into_owned(),
            bytes: self.bytes,
            fragments: self.fragments,
        }
    }
}

impl Drop for DeviceRecorder {
    fn drop(&mut self) {
        let _ = self.close_current();
    }
}

#[derive(Debug)]
struct StoredFile {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
}

fn clean_files(
    root: &Path,
    current: &BTreeSet<PathBuf>,
    retain_days: u64,
    max_bytes: u64,
) -> Result<()> {
    let mut files = Vec::new();
    collect_files(root, &mut files)?;
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(retain_days.saturating_mul(86_400)))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    for file in &files {
        if file.modified < cutoff && !protected(&file.path, current) {
            remove_pair(&file.path);
        }
    }

    files.clear();
    collect_files(root, &mut files)?;
    files.sort_by_key(|file| file.modified);
    let mut total = files.iter().map(|file| file.size).sum::<u64>();
    let target = max_bytes.saturating_mul(9) / 10;
    if total <= max_bytes {
        return Ok(());
    }
    for file in files {
        if total <= target {
            break;
        }
        if protected(&file.path, current) {
            continue;
        }
        if fs::remove_file(&file.path).is_ok() {
            total = total.saturating_sub(file.size);
        }
    }
    Ok(())
}

fn collect_files(root: &Path, output: &mut Vec<StoredFile>) -> Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, output)?;
        } else if let Ok(metadata) = entry.metadata() {
            output.push(StoredFile {
                path,
                size: metadata.len(),
                modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            });
        }
    }
    Ok(())
}

fn protected(path: &Path, current: &BTreeSet<PathBuf>) -> bool {
    current.iter().any(|item| {
        path == item || path == PathBuf::from(format!("{}.idx", item.to_string_lossy()))
    })
}

fn remove_pair(path: &Path) {
    let _ = fs::remove_file(path);
    if path.extension().is_some_and(|extension| extension == "mp4") {
        let _ = fs::remove_file(format!("{}.idx", path.display()));
    }
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

fn box_header(data: &[u8], offset: usize, end: usize) -> Option<(usize, [u8; 4], usize)> {
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

fn rewrite_tfdt(data: &mut [u8], base: &mut BTreeMap<u32, u64>) -> Option<u64> {
    let mut video_tfdt = None;
    let mut position = 0;
    while position + 8 <= data.len() {
        let (size, kind, header) = box_header(data, position, data.len())?;
        if &kind == b"moof" {
            let mut child = position + header;
            let end = position + size;
            while child + 8 <= end {
                let (child_size, child_kind, child_header) = box_header(data, child, end)?;
                if &child_kind == b"traf" {
                    rewrite_traf(
                        data,
                        child + child_header,
                        child + child_size,
                        base,
                        &mut video_tfdt,
                    );
                }
                child += child_size;
            }
        }
        position += size;
    }
    video_tfdt
}

fn rewrite_traf(
    data: &mut [u8],
    start: usize,
    end: usize,
    bases: &mut BTreeMap<u32, u64>,
    video_tfdt: &mut Option<u64>,
) {
    let mut track_id = None;
    let mut position = start;
    while position + 8 <= end {
        let Some((size, kind, header)) = box_header(data, position, end) else {
            return;
        };
        if &kind == b"tfhd" {
            track_id = read_u32(data, position + header + 4);
            break;
        }
        position += size;
    }
    let Some(track_id) = track_id else { return };

    position = start;
    while position + 8 <= end {
        let Some((size, kind, header)) = box_header(data, position, end) else {
            return;
        };
        if &kind == b"tfdt" {
            let Some(&version) = data.get(position + header) else {
                return;
            };
            let value_offset = position + header + 4;
            let original = if version == 1 {
                read_u64(data, value_offset)
            } else {
                read_u32(data, value_offset).map(u64::from)
            };
            let Some(original) = original else { return };
            let anchor = *bases.entry(track_id).or_insert(original);
            let fixed = original.saturating_sub(anchor);
            if version == 1 {
                if let Some(target) = data.get_mut(value_offset..value_offset + 8) {
                    target.copy_from_slice(&fixed.to_be_bytes());
                }
            } else if let Some(target) = data.get_mut(value_offset..value_offset + 4) {
                target.copy_from_slice(&(fixed as u32).to_be_bytes());
            }
            if track_id == 1 && video_tfdt.is_none() {
                *video_tfdt = Some(fixed);
            }
            return;
        }
        position += size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fragment(track: u32, tfdt: u64) -> Vec<u8> {
        let mut tfhd = Vec::new();
        tfhd.extend_from_slice(&16u32.to_be_bytes());
        tfhd.extend_from_slice(b"tfhd");
        tfhd.extend_from_slice(&[0, 0, 0, 0]);
        tfhd.extend_from_slice(&track.to_be_bytes());
        let mut tfdt_box = Vec::new();
        tfdt_box.extend_from_slice(&20u32.to_be_bytes());
        tfdt_box.extend_from_slice(b"tfdt");
        tfdt_box.extend_from_slice(&[1, 0, 0, 0]);
        tfdt_box.extend_from_slice(&tfdt.to_be_bytes());
        let traf_size = 8 + tfhd.len() + tfdt_box.len();
        let mut traf = Vec::new();
        traf.extend_from_slice(&(traf_size as u32).to_be_bytes());
        traf.extend_from_slice(b"traf");
        traf.extend_from_slice(&tfhd);
        traf.extend_from_slice(&tfdt_box);
        let moof_size = 8 + traf.len();
        let mut moof = Vec::new();
        moof.extend_from_slice(&(moof_size as u32).to_be_bytes());
        moof.extend_from_slice(b"moof");
        moof.extend_from_slice(&traf);
        moof
    }

    #[test]
    fn rebases_tfdt_per_track() {
        let mut bases = BTreeMap::new();
        let mut first = make_fragment(1, 900_000);
        let mut second = make_fragment(1, 990_000);
        assert_eq!(rewrite_tfdt(&mut first, &mut bases), Some(0));
        assert_eq!(rewrite_tfdt(&mut second, &mut bases), Some(90_000));
    }

    #[test]
    fn new_init_closes_active_recording() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "camera-hub-init-reconnect-{}-{nonce}",
            std::process::id()
        ));
        let settings = Arc::new(
            HubSettingsStore::load(
                root.join("hub-settings.json"),
                crate::settings::HubSettings {
                    ai_enabled: false,
                    ai_interval_ms: 1000,
                    ai_threshold: 0.3,
                    ai_min_person_area_ratio: 0.02,
                    ai_min_snapshot_seconds: 10,
                    ai_snapshot_max_count: 500,
                    ai_snapshot_quality: 95,
                    segment_seconds: 600,
                    max_bytes: 1024 * 1024,
                    retain_days: 7,
                    record_enabled: true,
                },
            )
            .unwrap(),
        );
        let store = MediaStore::new(root.clone(), settings).unwrap();
        store.set_init("front", b"\0\0\0\x08ftyp").unwrap();
        store.write_fragment("front", b"\0\0\0\x08moof").unwrap();
        assert!(store.statuses()[0].recording);

        store.set_init("front", b"\0\0\0\x08ftyp").unwrap();
        assert!(!store.statuses()[0].recording);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lists_record_days_and_files_without_exposing_unexpected_entries() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "camera-hub-record-list-{}-{nonce}",
            std::process::id()
        ));
        let first = root.join("front/record/20260809");
        let second = root.join("front/record/20260810");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(first.join("20260809_235959.mp4"), [1u8; 3]).unwrap();
        fs::write(second.join("20260810_010203.mp4"), [2u8; 5]).unwrap();
        fs::write(second.join("20260810_040506.mp4"), [3u8; 7]).unwrap();
        fs::write(second.join("20260810_040506.mp4.idx"), b"index").unwrap();
        fs::write(second.join("../ignored.txt"), b"ignored").unwrap();
        let settings = Arc::new(
            HubSettingsStore::load(
                root.join("hub-settings.json"),
                crate::settings::HubSettings {
                    ai_enabled: true,
                    ai_interval_ms: 1000,
                    ai_threshold: 0.3,
                    ai_min_person_area_ratio: 0.02,
                    ai_min_snapshot_seconds: 10,
                    ai_snapshot_max_count: 500,
                    ai_snapshot_quality: 95,
                    segment_seconds: 600,
                    max_bytes: 1024 * 1024 * 1024,
                    retain_days: 7,
                    record_enabled: true,
                },
            )
            .unwrap(),
        );
        let store = MediaStore::new(root.clone(), settings).unwrap();

        let days = store.record_days("front").unwrap();
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].date, "20260810");
        assert_eq!(days[0].files, 2);
        assert_eq!(days[0].bytes, 12);

        let records = store.recordings("front", "20260810").unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].name, "20260810_040506.mp4");
        assert_eq!(records[0].time, "04:05:06");
        assert_eq!(records[0].size, 7);
        assert!(!records[0].active);
        assert!(store.recordings("front", "../20260810").is_err());

        fs::remove_dir_all(root).unwrap();
    }
}
