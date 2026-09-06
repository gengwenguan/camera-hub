#[path = "../inference_lock.rs"]
mod inference_lock;
#[path = "../voice_tts.rs"]
mod voice_tts;

use anyhow::{Context, Result, bail};
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use clap::Parser;
use inference_lock::InferenceLock;
use serde_json::json;
use sha2::{Digest, Sha256};
use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsModelConfig,
    OfflineTtsZipvoiceModelConfig, Wave,
};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path as FsPath, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::net::TcpListener;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use voice_tts::{
    MAX_GENERATION_MILLIS, MAX_SYNTHESIZED_WAV_BYTES, TtsEnrollRequest, TtsProfileResponse,
    TtsSynthesizeRequest, decode_reference_audio, valid_profile_id, validate_synthesis_text,
};

const PROFILE_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_CONCURRENT_GENERATIONS: usize = 1;
const CACHE_VERSION: &str = "zipvoice-distill-int8-steps4-v1";
const MAX_CACHE_FILES_PER_PROFILE: usize = 128;
const MIN_DATA_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Parser)]
#[command(version, about = "Local ZipVoice synthesis service for camera-hub")]
struct Args {
    #[arg(long, env = "CAMERA_HUB_TTS_BIND", default_value = "127.0.0.1:39081")]
    bind: SocketAddr,

    #[arg(long, env = "CAMERA_HUB_TTS_TOKEN", default_value = "")]
    token: String,

    #[arg(
        long,
        env = "CAMERA_HUB_TTS_MODEL_DIR",
        default_value = "/home/android/camera-voice/models/sherpa-onnx-zipvoice-distill-int8-zh-en-emilia"
    )]
    model_dir: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_TTS_VOCODER",
        default_value = "/home/android/camera-voice/models/vocos_24khz.onnx"
    )]
    vocoder: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_TTS_DATA_DIR",
        default_value = "/home/android/camera-data/voice/tts"
    )]
    data_dir: PathBuf,

    #[arg(long, env = "CAMERA_HUB_TTS_THREADS", default_value_t = 2)]
    threads: i32,

    #[arg(
        long,
        env = "CAMERA_HUB_TTS_MAX_DATA_BYTES",
        default_value_t = 512 * 1024 * 1024
    )]
    max_data_bytes: u64,
}

struct TtsState {
    tts: Arc<StdMutex<Option<OfflineTts>>>,
    model: TtsModel,
    inference_lock: Arc<InferenceLock>,
    token: String,
    data_dir: PathBuf,
    max_data_bytes: u64,
    capacity: Arc<Semaphore>,
}

#[derive(Clone)]
struct TtsModel {
    model_dir: PathBuf,
    vocoder: PathBuf,
    threads: i32,
}

struct SynthesisTask {
    tts: Arc<StdMutex<Option<OfflineTts>>>,
    model: TtsModel,
    inference_lock: Arc<InferenceLock>,
    data_dir: PathBuf,
    profile_id: String,
    text: String,
    max_generation: Duration,
    max_data_bytes: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("camera_hub_tts=info")),
        )
        .init();
    let args = Args::parse();
    if !is_loopback(args.bind.ip()) && args.token.len() < 16 {
        bail!("TTS 绑定非回环地址时必须配置至少 16 个字符的 CAMERA_HUB_TTS_TOKEN");
    }
    create_private_dir(&args.data_dir)?;
    create_private_dir(&args.data_dir.join("profiles"))?;
    let model = TtsModel {
        model_dir: args.model_dir,
        vocoder: args.vocoder,
        threads: args.threads.clamp(1, 4),
    };
    validate_model_assets(&model)?;
    let inference_lock = Arc::new(InferenceLock::open()?);
    let state = Arc::new(TtsState {
        tts: Arc::new(StdMutex::new(None)),
        model,
        inference_lock,
        token: args.token,
        data_dir: args.data_dir,
        max_data_bytes: args.max_data_bytes.max(MIN_DATA_BYTES),
        capacity: Arc::new(Semaphore::new(MAX_CONCURRENT_GENERATIONS)),
    });
    spawn_cleanup(state.clone());

    let app = Router::new()
        .route("/health", get(health))
        .route(
            "/v1/profiles/{profile_id}",
            put(enroll_profile).delete(delete_profile),
        )
        .route("/v1/synthesize", post(synthesize))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .with_state(state);
    let listener = TcpListener::bind(args.bind)
        .await
        .with_context(|| format!("bind camera-hub-tts on {}", args.bind))?;
    info!(bind = %args.bind, "camera-hub TTS started");
    axum::serve(listener, app)
        .await
        .context("serve camera-hub-tts")
}

fn create_tts(model: &TtsModel) -> Result<OfflineTts> {
    let path = |name: &str| Some(model.model_dir.join(name).display().to_string());
    let config = OfflineTtsConfig {
        model: OfflineTtsModelConfig {
            zipvoice: OfflineTtsZipvoiceModelConfig {
                tokens: path("tokens.txt"),
                encoder: path("encoder.int8.onnx"),
                decoder: path("decoder.int8.onnx"),
                vocoder: Some(model.vocoder.display().to_string()),
                data_dir: path("espeak-ng-data"),
                lexicon: path("lexicon.txt"),
                ..Default::default()
            },
            num_threads: model.threads,
            provider: Some("cpu".to_owned()),
            ..Default::default()
        },
        max_num_sentences: 1,
        ..Default::default()
    };
    OfflineTts::create(&config).ok_or_else(|| anyhow::anyhow!("无法加载 ZipVoice TTS 模型"))
}

fn validate_model_assets(model: &TtsModel) -> Result<()> {
    for path in [
        model.model_dir.join("tokens.txt"),
        model.model_dir.join("encoder.int8.onnx"),
        model.model_dir.join("decoder.int8.onnx"),
        model.model_dir.join("lexicon.txt"),
        model.vocoder.clone(),
    ] {
        if !path.is_file() || fs::metadata(&path)?.len() == 0 {
            bail!("TTS 模型资源不存在：{}", path.display());
        }
    }
    let espeak_data = model.model_dir.join("espeak-ng-data");
    if !espeak_data.is_dir() {
        bail!("TTS 模型资源不存在：{}", espeak_data.display());
    }
    Ok(())
}

async fn health(State(state): State<Arc<TtsState>>) -> Json<serde_json::Value> {
    let loaded = state
        .tts
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .is_some();
    Json(json!({
        "ok": true,
        "model": "zipvoice-distill-int8",
        "loaded": loaded,
        "busy": state.capacity.available_permits() == 0
    }))
}

async fn enroll_profile(
    State(state): State<Arc<TtsState>>,
    Path(profile_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<TtsEnrollRequest>,
) -> Result<Json<TtsProfileResponse>, TtsError> {
    authorize(&state, &headers)?;
    if !valid_profile_id(&profile_id) {
        return Err(TtsError::bad_request("声纹 profile ID 无效"));
    }
    let wav = decode_reference_audio(&request.audio_base64).map_err(TtsError::bad_request_error)?;
    let transcript =
        validate_reference_text(&request.transcript).map_err(TtsError::bad_request_error)?;
    let fingerprint = fingerprint(&wav, &transcript);
    let permit = acquire_capacity(&state)?;
    let stored_fingerprint = fingerprint.clone();
    let root = profile_root(&state.data_dir, &profile_id);
    let data_dir = state.data_dir.clone();
    let max_data_bytes = state.max_data_bytes;
    let preserve = root.clone();
    let saved = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        save_profile(&root, &wav, &transcript, &stored_fingerprint)?;
        prune_storage(&data_dir, max_data_bytes, Some(&preserve), None)
    })
    .await
    .map_err(|error| TtsError::internal("等待声纹保存任务", error))?;
    saved.map_err(|error| TtsError::internal("保存声纹 profile", error))?;
    info!(profile_id, "TTS voice profile enrolled");
    Ok(Json(TtsProfileResponse {
        profile_id,
        fingerprint,
    }))
}

async fn delete_profile(
    State(state): State<Arc<TtsState>>,
    Path(profile_id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, TtsError> {
    authorize(&state, &headers)?;
    if !valid_profile_id(&profile_id) {
        return Err(TtsError::bad_request("声纹 profile ID 无效"));
    }
    let permit = acquire_capacity(&state)?;
    let root = profile_root(&state.data_dir, &profile_id);
    let result = match tokio::fs::remove_dir_all(root).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(StatusCode::NO_CONTENT),
        Err(error) => Err(TtsError::internal("删除声纹 profile", error)),
    };
    drop(permit);
    result
}

async fn synthesize(
    State(state): State<Arc<TtsState>>,
    headers: HeaderMap,
    Json(request): Json<TtsSynthesizeRequest>,
) -> Result<Response, TtsError> {
    authorize(&state, &headers)?;
    if !valid_profile_id(&request.profile_id) {
        return Err(TtsError::bad_request("声纹 profile ID 无效"));
    }
    let text = validate_synthesis_text(&request.text).map_err(TtsError::bad_request_error)?;
    let permit = acquire_capacity(&state)?;
    let profile_id = request.profile_id;
    let logged_profile_id = profile_id.clone();
    let data_dir = state.data_dir.clone();
    let root = profile_root(&data_dir, &profile_id);
    if !root.join("fingerprint").is_file() {
        return Err(TtsError::not_found("声纹 profile 不存在，请重新录入"));
    }
    let max_generation = Duration::from_millis(
        request
            .max_generation_ms
            .unwrap_or(MAX_GENERATION_MILLIS)
            .clamp(1_000, MAX_GENERATION_MILLIS),
    );
    let task = SynthesisTask {
        tts: state.tts.clone(),
        model: state.model.clone(),
        inference_lock: state.inference_lock.clone(),
        data_dir,
        profile_id,
        text,
        max_generation,
        max_data_bytes: state.max_data_bytes,
    };
    let generated = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        synthesize_cached(&task)
    })
    .await
    .map_err(|error| TtsError::internal("等待 TTS 推理任务", error))?;
    let wav = generated.map_err(|error| TtsError::internal("执行 TTS 推理", error))?;
    info!(
        profile_id = logged_profile_id,
        wav_bytes = wav.len(),
        "TTS synthesis completed"
    );

    Ok((
        [
            (CONTENT_TYPE, HeaderValue::from_static("audio/wav")),
            (CACHE_CONTROL, HeaderValue::from_static("private, no-store")),
        ],
        Body::from(wav),
    )
        .into_response())
}

fn synthesize_cached(task: &SynthesisTask) -> Result<Vec<u8>> {
    let root = profile_root(&task.data_dir, &task.profile_id);
    let reference_path = root.join("reference.wav");
    let reference_bytes = fs::read(&reference_path).context("读取声纹参考 WAV")?;
    let transcript = fs::read_to_string(root.join("reference.txt"))
        .context("读取声纹参考文稿")?
        .trim()
        .to_owned();
    let profile_fingerprint = fs::read_to_string(root.join("fingerprint"))
        .context("读取声纹指纹")?
        .trim()
        .to_owned();
    if fingerprint(&reference_bytes, &transcript) != profile_fingerprint {
        bail!("声纹资料不完整，请重新录入");
    }
    let cache_identity = format!("{profile_fingerprint}:{CACHE_VERSION}");
    let cache_key = fingerprint(cache_identity, &task.text);
    let cache_dir = root.join("cache");
    let cache_path = cache_dir.join(format!("{cache_key}.wav"));
    if let Ok(data) = fs::read(&cache_path) {
        return Ok(data);
    }

    let reference = Wave::read(&reference_path.display().to_string())
        .ok_or_else(|| anyhow::anyhow!("无法读取声纹参考 WAV"))?;
    let generation = GenerationConfig {
        reference_audio: Some(reference.samples().to_vec()),
        reference_sample_rate: reference.sample_rate(),
        reference_text: Some(transcript),
        num_steps: 4,
        extra: Some(
            [(
                "min_char_in_sentence".to_owned(),
                serde_json::Value::from(10),
            )]
            .into_iter()
            .collect(),
        ),
        ..Default::default()
    };
    let _guard = task.inference_lock.lock()?;
    let mut tts = task.tts.lock().unwrap_or_else(|error| error.into_inner());
    if tts.is_none() {
        *tts = Some(create_tts(&task.model)?);
    }
    let deadline = Instant::now() + task.max_generation;
    let cancelled = Arc::new(AtomicBool::new(false));
    let callback_cancelled = cancelled.clone();
    let audio = tts
        .as_ref()
        .expect("TTS initialized")
        .generate_with_config(
            &task.text,
            &generation,
            Some(move |_: &[f32], _: f32| {
                let keep_running = Instant::now() < deadline;
                if !keep_running {
                    callback_cancelled.store(true, Ordering::Relaxed);
                }
                keep_running
            }),
        )
        .ok_or_else(|| anyhow::anyhow!("ZipVoice 未生成音频"))?;
    drop(_guard);
    if cancelled.load(Ordering::Relaxed) {
        bail!(
            "ZipVoice 生成超过 {} 秒，已取消",
            task.max_generation.as_secs()
        );
    }
    if audio.samples().is_empty() {
        bail!("ZipVoice 生成了空音频");
    }

    create_private_dir(&cache_dir)?;
    let temporary = cache_path.with_extension("wav.tmp");
    let _ = fs::remove_file(&temporary);
    if !audio.save(&temporary.display().to_string()) {
        bail!("保存 ZipVoice WAV 失败");
    }
    set_private_permissions(&temporary)?;
    if fs::metadata(&temporary)?.len() > MAX_SYNTHESIZED_WAV_BYTES {
        let _ = fs::remove_file(&temporary);
        bail!("ZipVoice 生成的 WAV 超过 8 MiB");
    }
    fs::rename(&temporary, &cache_path)?;
    prune_cache(&cache_dir, &cache_path);
    prune_storage(
        &task.data_dir,
        task.max_data_bytes,
        Some(&root),
        Some(&cache_path),
    )?;
    fs::read(cache_path).context("读取生成的 ZipVoice WAV")
}

fn prune_cache(cache_dir: &FsPath, current: &FsPath) {
    let Ok(entries) = fs::read_dir(cache_dir) else {
        return;
    };
    let mut files = entries
        .flatten()
        .filter_map(|entry| {
            if entry.path() == current {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            metadata.is_file().then(|| {
                (
                    metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    entry.path(),
                )
            })
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(modified, _)| *modified);
    let remove_count = files
        .len()
        .saturating_add(1)
        .saturating_sub(MAX_CACHE_FILES_PER_PROFILE);
    for (_, path) in files.into_iter().take(remove_count) {
        let _ = fs::remove_file(path);
    }
}

fn save_profile(root: &FsPath, wav: &[u8], transcript: &str, fingerprint: &str) -> Result<()> {
    let parent = root
        .parent()
        .ok_or_else(|| anyhow::anyhow!("声纹 profile 缺少父目录"))?;
    create_private_dir(parent)?;
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("声纹 profile 路径无效"))?;
    let staging = parent.join(format!(".{name}.new"));
    let backup = parent.join(format!(".{name}.old"));
    for path in [&staging, &backup] {
        match fs::remove_dir_all(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    create_private_dir(&staging)?;
    write_private(&staging.join("reference.wav"), wav)?;
    write_private(&staging.join("reference.txt"), transcript.as_bytes())?;
    write_private(&staging.join("fingerprint"), fingerprint.as_bytes())?;

    let had_previous = root.exists();
    if had_previous {
        fs::rename(root, &backup)?;
    }
    if let Err(error) = fs::rename(&staging, root) {
        if had_previous {
            let _ = fs::rename(&backup, root);
        }
        return Err(error.into());
    }
    if had_previous {
        let _ = fs::remove_dir_all(backup);
    }
    Ok(())
}

fn write_private(path: &FsPath, data: &[u8]) -> Result<()> {
    let temporary = path.with_extension("tmp");
    let _ = fs::remove_file(&temporary);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(data)?;
    file.sync_all()?;
    drop(file);
    set_private_permissions(&temporary)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn create_private_dir(path: &FsPath) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn set_private_permissions(path: &FsPath) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn validate_reference_text(text: &str) -> Result<String> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > 200 || text.chars().any(char::is_control) {
        bail!("参考文稿不能为空、不能换行且最多 200 个字符");
    }
    Ok(text.to_owned())
}

fn fingerprint(first: impl AsRef<[u8]>, second: impl AsRef<[u8]>) -> String {
    let mut digest = Sha256::new();
    digest.update(first.as_ref());
    digest.update([0]);
    digest.update(second.as_ref());
    hex::encode(digest.finalize())
}

fn profile_root(data_dir: &FsPath, profile_id: &str) -> PathBuf {
    data_dir.join("profiles").join(profile_id)
}

fn authorize(state: &TtsState, headers: &HeaderMap) -> Result<(), TtsError> {
    if state.token.is_empty() {
        return Ok(());
    }
    let provided = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if constant_time_eq(provided.as_bytes(), state.token.as_bytes()) {
        Ok(())
    } else {
        Err(TtsError {
            status: StatusCode::UNAUTHORIZED,
            message: "TTS authentication required".to_owned(),
        })
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let length = left.len().max(right.len());
    for index in 0..length {
        difference |= usize::from(left.get(index).copied().unwrap_or_default())
            ^ usize::from(right.get(index).copied().unwrap_or_default());
    }
    difference == 0
}

fn is_loopback(address: IpAddr) -> bool {
    address.is_loopback()
}

fn acquire_capacity(state: &Arc<TtsState>) -> Result<OwnedSemaphorePermit, TtsError> {
    state
        .capacity
        .clone()
        .try_acquire_owned()
        .map_err(|_| TtsError::busy())
}

fn spawn_cleanup(state: Arc<TtsState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60 * 60));
        loop {
            interval.tick().await;
            let Ok(permit) = state.capacity.clone().try_acquire_owned() else {
                continue;
            };
            let data_dir = state.data_dir.clone();
            let max_data_bytes = state.max_data_bytes;
            match tokio::task::spawn_blocking(move || {
                let _permit = permit;
                prune_storage(&data_dir, max_data_bytes, None, None)
            })
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => error!(%error, "TTS profile cleanup failed"),
                Err(error) => error!(%error, "join TTS profile cleanup task"),
            }
        }
    });
}

fn prune_storage(
    data_dir: &FsPath,
    max_data_bytes: u64,
    preserve_profile: Option<&FsPath>,
    preserve_file: Option<&FsPath>,
) -> Result<()> {
    let root = data_dir.join("profiles");
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with("public-") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::now());
        if modified.elapsed().is_ok_and(|age| age >= PROFILE_RETENTION) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }

    let profiles_root = data_dir.join("profiles");
    let mut files = Vec::new();
    collect_files(&profiles_root, &mut files)?;
    let mut total = files.iter().map(|file| file.bytes).sum::<u64>();
    files.retain(|file| {
        if preserve_file.is_some_and(|preserve| preserve == file.path) {
            return false;
        }
        file.path
            .parent()
            .is_some_and(|parent| parent.file_name().is_some_and(|name| name == "cache"))
    });
    files.sort_by_key(|file| file.modified);
    for file in files {
        if total <= max_data_bytes {
            break;
        }
        if fs::remove_file(&file.path).is_ok() {
            total = total.saturating_sub(file.bytes);
        }
    }

    if total > max_data_bytes {
        let mut profiles = fs::read_dir(&profiles_root)?
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if preserve_profile.is_some_and(|preserve| preserve == path) {
                    return None;
                }
                let name = entry.file_name();
                let name = name.to_str()?;
                if !name.starts_with("public-") {
                    return None;
                }
                let metadata = entry.metadata().ok()?;
                Some(StoredPath {
                    bytes: directory_size(&path).ok()?,
                    modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    path,
                })
            })
            .collect::<Vec<_>>();
        profiles.sort_by_key(|profile| profile.modified);
        for profile in profiles {
            if total <= max_data_bytes {
                break;
            }
            if fs::remove_dir_all(&profile.path).is_ok() {
                total = total.saturating_sub(profile.bytes);
            }
        }
    }
    if total > max_data_bytes {
        bail!("TTS 数据目录超过配置的容量上限");
    }
    Ok(())
}

struct StoredPath {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

fn collect_files(root: &FsPath, files: &mut Vec<StoredPath>) -> Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_files(&entry.path(), files)?;
        } else if metadata.is_file() {
            files.push(StoredPath {
                path: entry.path(),
                bytes: metadata.len(),
                modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            });
        }
    }
    Ok(())
}

fn directory_size(path: &FsPath) -> Result<u64> {
    let mut files = Vec::new();
    collect_files(path, &mut files)?;
    Ok(files.iter().map(|file| file.bytes).sum())
}

struct TtsError {
    status: StatusCode,
    message: String,
}

impl TtsError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn bad_request_error(error: anyhow::Error) -> Self {
        Self::bad_request(format!("{error:#}"))
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn busy() -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: "TTS 正在处理其他任务，请稍后重试".to_owned(),
        }
    }

    fn internal(context: &'static str, error: impl std::fmt::Display) -> Self {
        error!(%error, context, "TTS request failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "TTS 服务内部错误".to_owned(),
        }
    }
}

impl IntoResponse for TtsError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({"ok":false,"error":self.message}))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "camera-hub-tts-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn validates_reference_text_and_constant_time_token() {
        assert_eq!(validate_reference_text(" 你好 ").unwrap(), "你好");
        assert!(validate_reference_text("a\nb").is_err());
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"other"));
    }

    #[test]
    fn creates_stable_separated_fingerprints() {
        assert_eq!(fingerprint("ab", "c"), fingerprint("ab", "c"));
        assert_ne!(fingerprint("ab", "c"), fingerprint("a", "bc"));
    }

    #[test]
    fn prunes_profile_cache_without_removing_current_file() {
        let root = temporary_root("cache");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let current = root.join("current.wav");
        fs::write(&current, b"current").unwrap();
        for index in 0..MAX_CACHE_FILES_PER_PROFILE {
            fs::write(root.join(format!("{index:03}.wav")), b"old").unwrap();
        }

        prune_cache(&root, &current);

        assert!(current.is_file());
        assert_eq!(
            fs::read_dir(&root).unwrap().count(),
            MAX_CACHE_FILES_PER_PROFILE
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn atomically_replaces_complete_profile() {
        let data_dir = temporary_root("profile");
        let root = profile_root(&data_dir, "public-test");
        fs::create_dir_all(root.join("cache")).unwrap();
        fs::write(root.join("reference.wav"), b"old").unwrap();
        fs::write(root.join("reference.txt"), b"old").unwrap();
        fs::write(root.join("fingerprint"), b"old").unwrap();
        fs::write(root.join("cache/old.wav"), b"old").unwrap();

        save_profile(&root, b"new-wav", "new-text", "new-fingerprint").unwrap();

        assert_eq!(fs::read(root.join("reference.wav")).unwrap(), b"new-wav");
        assert_eq!(
            fs::read_to_string(root.join("reference.txt")).unwrap(),
            "new-text"
        );
        assert_eq!(
            fs::read_to_string(root.join("fingerprint")).unwrap(),
            "new-fingerprint"
        );
        assert!(!root.join("cache").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(root.join("reference.wav"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn enforces_global_storage_budget() {
        let data_dir = temporary_root("budget");
        for profile in ["public-a", "public-b"] {
            let cache = profile_root(&data_dir, profile).join("cache");
            fs::create_dir_all(&cache).unwrap();
            fs::write(cache.join("one.wav"), vec![1_u8; 2048]).unwrap();
        }

        prune_storage(&data_dir, 2048, None, None).unwrap();

        assert!(directory_size(&data_dir.join("profiles")).unwrap() <= 2048);
        let _ = fs::remove_dir_all(data_dir);
    }
}
