use crate::component_manager::ComponentManager;
use crate::config::Config;
#[cfg(feature = "voice-workers")]
use anyhow::Context;
use anyhow::{Result, bail};
#[cfg(feature = "voice-workers")]
use futures_util::StreamExt;
use serde::Serialize;
#[cfg(feature = "voice-workers")]
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[cfg(all(feature = "voice-workers", unix))]
use std::ffi::CString;
use std::fs;
#[cfg(feature = "voice-workers")]
use std::io::{BufReader, Read};
#[cfg(feature = "voice-workers")]
use std::path::Component;
use std::path::Path;
#[cfg(feature = "voice-workers")]
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(feature = "voice-workers")]
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tracing::{info, warn};

#[cfg(feature = "voice-workers")]
const KWS_MODEL: &str = "sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01";
#[cfg(feature = "voice-workers")]
const KWS_ARCHIVE: &str = "sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01.tar.bz2";
#[cfg(feature = "voice-workers")]
const KWS_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/kws-models/sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01.tar.bz2";
#[cfg(feature = "voice-workers")]
const KWS_SHA256: &str = "b2f7c89690dc8ce4c6ed6afeab7cd800c36ad1421fb6b6302b4a4b194cf7f35f";
const KWS_DOWNLOAD_BYTES: u64 = 32_654_866;
const KWS_INSTALLED_BYTES: u64 = 37_361_991;

#[cfg(feature = "voice-workers")]
const ASR_MODEL: &str = "sherpa-onnx-streaming-zipformer-zh-14M-2023-02-23";
#[cfg(feature = "voice-workers")]
const ASR_ARCHIVE: &str = "sherpa-onnx-streaming-zipformer-zh-14M-2023-02-23.tar.bz2";
#[cfg(feature = "voice-workers")]
const ASR_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-streaming-zipformer-zh-14M-2023-02-23.tar.bz2";
#[cfg(feature = "voice-workers")]
const ASR_SHA256: &str = "2cbd71b640d9c37d3784f29367333a4577b0398b62e9deeed418170b081cba8b";
const ASR_DOWNLOAD_BYTES: u64 = 74_004_050;
const ASR_INSTALLED_BYTES: u64 = 81_340_658;

#[cfg(feature = "voice-workers")]
const TTS_MODEL: &str = "sherpa-onnx-zipvoice-distill-int8-zh-en-emilia";
#[cfg(feature = "voice-workers")]
const TTS_ARCHIVE: &str = "sherpa-onnx-zipvoice-distill-int8-zh-en-emilia.tar.bz2";
#[cfg(feature = "voice-workers")]
const TTS_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/sherpa-onnx-zipvoice-distill-int8-zh-en-emilia.tar.bz2";
#[cfg(feature = "voice-workers")]
const TTS_SHA256: &str = "77219c8b40f4ee8d73a7f902305ff6c1128ef9b54461c41b4ca6ed890b6c2803";
#[cfg(feature = "voice-workers")]
const VOCODER_FILE: &str = "vocos_24khz.onnx";
#[cfg(feature = "voice-workers")]
const VOCODER_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/vocoder-models/vocos_24khz.onnx";
#[cfg(feature = "voice-workers")]
const VOCODER_SHA256: &str = "bcb3b970e384161c4d634f0bb9e999ff1c471b34c9bc0b1049a5014065ed3cc0";
#[cfg(feature = "voice-workers")]
const TTS_ARCHIVE_BYTES: u64 = 109_162_785;
#[cfg(feature = "voice-workers")]
const VOCODER_BYTES: u64 = 54_157_409;
const TTS_DOWNLOAD_BYTES: u64 = 163_320_194;
const TTS_INSTALLED_BYTES: u64 = 205_309_868;
const INSTALL_HEADROOM_BYTES: u64 = 64 * 1024 * 1024;

const KWS_REQUIRED: &[RequiredPath] = &[
    RequiredPath::file("tokens.txt"),
    RequiredPath::file("encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx"),
    RequiredPath::file("decoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx"),
    RequiredPath::file("joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx"),
];

const ASR_REQUIRED: &[RequiredPath] = &[
    RequiredPath::file("tokens.txt"),
    RequiredPath::file("encoder-epoch-99-avg-1.int8.onnx"),
    RequiredPath::file("decoder-epoch-99-avg-1.int8.onnx"),
    RequiredPath::file("joiner-epoch-99-avg-1.int8.onnx"),
];

const TTS_REQUIRED: &[RequiredPath] = &[
    RequiredPath::file("tokens.txt"),
    RequiredPath::file("encoder.int8.onnx"),
    RequiredPath::file("decoder.int8.onnx"),
    RequiredPath::file("lexicon.txt"),
    RequiredPath::directory("espeak-ng-data"),
];

#[cfg(feature = "voice-workers")]
const KWS_ARTIFACTS: &[Artifact] = &[Artifact {
    file_name: KWS_ARCHIVE,
    url: KWS_URL,
    sha256: KWS_SHA256,
    bytes: KWS_DOWNLOAD_BYTES,
}];

#[cfg(feature = "voice-workers")]
const ASR_ARTIFACTS: &[Artifact] = &[Artifact {
    file_name: ASR_ARCHIVE,
    url: ASR_URL,
    sha256: ASR_SHA256,
    bytes: ASR_DOWNLOAD_BYTES,
}];

#[cfg(feature = "voice-workers")]
const TTS_ARTIFACTS: &[Artifact] = &[
    Artifact {
        file_name: TTS_ARCHIVE,
        url: TTS_URL,
        sha256: TTS_SHA256,
        bytes: TTS_ARCHIVE_BYTES,
    },
    Artifact {
        file_name: VOCODER_FILE,
        url: VOCODER_URL,
        sha256: VOCODER_SHA256,
        bytes: VOCODER_BYTES,
    },
];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum AssetId {
    VoiceKws,
    VoiceAsr,
    VoiceTts,
}

impl AssetId {
    const ALL: [Self; 3] = [Self::VoiceKws, Self::VoiceAsr, Self::VoiceTts];

    fn parse(value: &str) -> Result<Self> {
        match value {
            "voice-kws" => Ok(Self::VoiceKws),
            "voice-asr" => Ok(Self::VoiceAsr),
            "voice-tts" => Ok(Self::VoiceTts),
            _ => bail!("不支持的资源包：{value}"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::VoiceKws => "voice-kws",
            Self::VoiceAsr => "voice-asr",
            Self::VoiceTts => "voice-tts",
        }
    }

    fn definition(self) -> AssetDefinition {
        match self {
            Self::VoiceKws => AssetDefinition {
                label: "语音识别模型",
                version: "kws-zipformer-3.3m-2024-01-01",
                download_bytes: KWS_DOWNLOAD_BYTES,
                installed_bytes: KWS_INSTALLED_BYTES,
                #[cfg(feature = "voice-workers")]
                artifacts: KWS_ARTIFACTS,
            },
            Self::VoiceAsr => AssetDefinition {
                label: "自然语言识别模型",
                version: "streaming-zipformer-zh-14m-2023-02-23",
                download_bytes: ASR_DOWNLOAD_BYTES,
                installed_bytes: ASR_INSTALLED_BYTES,
                #[cfg(feature = "voice-workers")]
                artifacts: ASR_ARTIFACTS,
            },
            Self::VoiceTts => AssetDefinition {
                label: "TTS 声纹模型",
                version: "zipvoice-distill-int8-v1",
                download_bytes: TTS_DOWNLOAD_BYTES,
                installed_bytes: TTS_INSTALLED_BYTES,
                #[cfg(feature = "voice-workers")]
                artifacts: TTS_ARTIFACTS,
            },
        }
    }
}

#[derive(Clone, Copy)]
struct AssetDefinition {
    label: &'static str,
    version: &'static str,
    download_bytes: u64,
    installed_bytes: u64,
    #[cfg(feature = "voice-workers")]
    artifacts: &'static [Artifact],
}

#[cfg(feature = "voice-workers")]
#[derive(Clone, Copy)]
struct Artifact {
    file_name: &'static str,
    url: &'static str,
    sha256: &'static str,
    bytes: u64,
}

#[derive(Clone, Copy)]
enum RequiredKind {
    File,
    Directory,
}

#[derive(Clone, Copy)]
struct RequiredPath {
    path: &'static str,
    kind: RequiredKind,
}

impl RequiredPath {
    const fn file(path: &'static str) -> Self {
        Self {
            path,
            kind: RequiredKind::File,
        }
    }

    const fn directory(path: &'static str) -> Self {
        Self {
            path,
            kind: RequiredKind::Directory,
        }
    }
}

#[derive(Clone)]
struct RuntimeStatus {
    state: String,
    detail: String,
    downloaded_bytes: u64,
    total_bytes: u64,
    last_error: String,
    updated_epoch: u64,
}

impl RuntimeStatus {
    fn new(id: AssetId, installed: bool) -> Self {
        let definition = id.definition();
        Self {
            state: if installed { "installed" } else { "missing" }.to_owned(),
            detail: if installed {
                "模型资源已安装".to_owned()
            } else {
                "模型资源尚未安装".to_owned()
            },
            downloaded_bytes: 0,
            total_bytes: definition.download_bytes,
            last_error: String::new(),
            updated_epoch: epoch_seconds(),
        }
    }
}

struct ManagerState {
    active: Option<AssetId>,
    runtime: BTreeMap<AssetId, RuntimeStatus>,
}

pub struct AssetManager {
    config: Config,
    #[cfg(feature = "voice-workers")]
    cache_dir: PathBuf,
    #[cfg(feature = "voice-workers")]
    components: Arc<ComponentManager>,
    #[cfg(feature = "voice-workers")]
    client: reqwest::Client,
    state: Mutex<ManagerState>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AssetStatus {
    pub id: &'static str,
    pub label: &'static str,
    pub version: &'static str,
    pub supported: bool,
    pub installed: bool,
    pub installable: bool,
    pub active: bool,
    pub state: String,
    pub detail: String,
    pub downloaded_bytes: u64,
    pub download_bytes: u64,
    pub installed_bytes: u64,
    pub required_available_bytes: u64,
    pub progress_percent: f64,
    pub last_error: String,
    pub updated_epoch: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct AssetsOverview {
    pub busy: bool,
    pub voice_kws: AssetStatus,
    pub voice_asr: AssetStatus,
    pub voice_tts: AssetStatus,
}

impl AssetManager {
    pub fn new(config: &Config, components: Arc<ComponentManager>) -> Result<Arc<Self>> {
        let runtime = AssetId::ALL
            .into_iter()
            .map(|id| (id, RuntimeStatus::new(id, asset_installed(config, id))))
            .collect();
        #[cfg(feature = "voice-workers")]
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(20))
            .user_agent(concat!("camera-hub-assets/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("创建模型下载客户端")?;
        #[cfg(not(feature = "voice-workers"))]
        let _ = components;
        Ok(Arc::new(Self {
            config: config.clone(),
            #[cfg(feature = "voice-workers")]
            cache_dir: config.asset_cache_dir.clone(),
            #[cfg(feature = "voice-workers")]
            components,
            #[cfg(feature = "voice-workers")]
            client,
            state: Mutex::new(ManagerState {
                active: None,
                runtime,
            }),
        }))
    }

    pub async fn overview(&self) -> AssetsOverview {
        let state = self.state.lock().await;
        AssetsOverview {
            busy: state.active.is_some(),
            voice_kws: self.status_locked(&state, AssetId::VoiceKws),
            voice_asr: self.status_locked(&state, AssetId::VoiceAsr),
            voice_tts: self.status_locked(&state, AssetId::VoiceTts),
        }
    }

    pub async fn install(self: &Arc<Self>, name: &str) -> Result<AssetStatus> {
        let id = AssetId::parse(name)?;
        if !cfg!(feature = "voice-workers") {
            bail!("当前 camera-hub 未包含 voice-workers 功能");
        }
        let mut state = self.state.lock().await;
        if let Some(active) = state.active {
            bail!("资源包 {} 正在安装，请等待完成", active.as_str());
        }
        state.active = Some(id);
        let definition = id.definition();
        state.runtime.insert(
            id,
            RuntimeStatus {
                state: "queued".to_owned(),
                detail: "等待开始下载".to_owned(),
                downloaded_bytes: 0,
                total_bytes: definition.download_bytes,
                last_error: String::new(),
                updated_epoch: epoch_seconds(),
            },
        );
        let status = self.status_locked(&state, id);
        drop(state);

        let manager = self.clone();
        tokio::spawn(async move {
            manager.run_install(id).await;
        });
        Ok(status)
    }

    fn status_locked(&self, state: &ManagerState, id: AssetId) -> AssetStatus {
        let definition = id.definition();
        let runtime = &state.runtime[&id];
        let installed = asset_installed(&self.config, id);
        let active = state.active == Some(id);
        let display_state = if active || runtime.state == "failed" {
            runtime.state.clone()
        } else if installed {
            "installed".to_owned()
        } else {
            "missing".to_owned()
        };
        let detail = if active || runtime.state == "failed" {
            runtime.detail.clone()
        } else if installed {
            "模型资源已安装".to_owned()
        } else {
            "模型资源尚未安装".to_owned()
        };
        let progress_percent = if runtime.total_bytes == 0 {
            0.0
        } else {
            (runtime.downloaded_bytes as f64 * 100.0 / runtime.total_bytes as f64).clamp(0.0, 100.0)
        };
        AssetStatus {
            id: id.as_str(),
            label: definition.label,
            version: definition.version,
            supported: cfg!(feature = "voice-workers"),
            installed,
            installable: cfg!(feature = "voice-workers") && state.active.is_none(),
            active,
            state: display_state,
            detail,
            downloaded_bytes: runtime.downloaded_bytes,
            download_bytes: definition.download_bytes,
            installed_bytes: definition.installed_bytes,
            required_available_bytes: definition
                .download_bytes
                .saturating_add(definition.installed_bytes)
                .saturating_add(INSTALL_HEADROOM_BYTES),
            progress_percent,
            last_error: runtime.last_error.clone(),
            updated_epoch: runtime.updated_epoch,
        }
    }

    async fn run_install(&self, id: AssetId) {
        let result = self.install_inner(id).await;
        let mut state = self.state.lock().await;
        state.active = None;
        let runtime = state.runtime.get_mut(&id).expect("asset runtime");
        runtime.updated_epoch = epoch_seconds();
        match result {
            Ok(detail) => {
                runtime.state = "installed".to_owned();
                runtime.detail = detail;
                runtime.downloaded_bytes = runtime.total_bytes;
                runtime.last_error.clear();
                info!(asset = id.as_str(), "voice asset installed");
            }
            Err(error) => {
                runtime.state = "failed".to_owned();
                runtime.detail = "模型安装失败".to_owned();
                runtime.last_error = format!("{error:#}");
                warn!(asset = id.as_str(), error = %runtime.last_error, "voice asset installation failed");
            }
        }
    }

    #[cfg(feature = "voice-workers")]
    async fn install_inner(&self, id: AssetId) -> Result<String> {
        let definition = id.definition();
        tokio::fs::create_dir_all(&self.cache_dir)
            .await
            .with_context(|| format!("创建模型缓存目录 {}", self.cache_dir.display()))?;

        let mut cached = Vec::with_capacity(definition.artifacts.len());
        let mut cached_bytes = 0_u64;
        let mut missing_bytes = 0_u64;
        for artifact in definition.artifacts {
            let path = self.cache_dir.join(artifact.file_name);
            let valid = verify_cached(path.clone(), *artifact).await?;
            if valid {
                cached_bytes = cached_bytes.saturating_add(artifact.bytes);
            } else {
                let _ = tokio::fs::remove_file(&path).await;
                missing_bytes = missing_bytes.saturating_add(artifact.bytes);
            }
            cached.push(path);
        }

        let install_parent = install_parent(&self.config, id)?;
        tokio::fs::create_dir_all(&install_parent).await?;
        ensure_free_space(
            &self.cache_dir,
            missing_bytes
                .saturating_add(definition.installed_bytes)
                .saturating_add(INSTALL_HEADROOM_BYTES),
        )?;
        ensure_free_space(
            &install_parent,
            definition
                .installed_bytes
                .saturating_add(INSTALL_HEADROOM_BYTES),
        )?;

        self.update_runtime(id, "downloading", "正在下载模型资源", cached_bytes, "")
            .await;
        let mut completed = cached_bytes;
        for (artifact, path) in definition.artifacts.iter().zip(cached.iter()) {
            if tokio::fs::try_exists(path).await.unwrap_or(false) {
                continue;
            }
            self.download_artifact(id, *artifact, path, completed)
                .await?;
            completed = completed.saturating_add(artifact.bytes);
        }

        self.update_runtime(
            id,
            "verifying",
            "校验完成，正在解压模型",
            definition.download_bytes,
            "",
        )
        .await;
        let config = self.config.clone();
        let cached_for_prepare = cached.clone();
        let prepared =
            tokio::task::spawn_blocking(move || prepare_install(id, &config, &cached_for_prepare))
                .await
                .context("模型解压任务异常退出")??;

        self.update_runtime(
            id,
            "installing",
            "正在原子替换模型文件",
            definition.download_bytes,
            "",
        )
        .await;
        let resume = self.components.quiesce_for_asset(id.as_str()).await?;
        let commit_result = match tokio::task::spawn_blocking(move || prepared.commit()).await {
            Ok(result) => result,
            Err(error) => Err(anyhow::anyhow!("模型安装任务异常退出：{error}")),
        };

        self.update_runtime(
            id,
            "reinitializing",
            "模型已安装，正在重新初始化服务",
            definition.download_bytes,
            "",
        )
        .await;
        let resume_result = self
            .components
            .resume_after_asset(id.as_str(), resume)
            .await;
        commit_result?;

        match resume_result {
            Ok(()) => Ok("模型已安装，服务已重新初始化".to_owned()),
            Err(error) => Ok(format!("模型已安装；服务等待依赖就绪：{error}")),
        }
    }

    #[cfg(not(feature = "voice-workers"))]
    async fn install_inner(&self, _id: AssetId) -> Result<String> {
        bail!("当前 camera-hub 未包含 voice-workers 功能")
    }

    #[cfg(feature = "voice-workers")]
    async fn download_artifact(
        &self,
        id: AssetId,
        artifact: Artifact,
        target: &Path,
        completed_bytes: u64,
    ) -> Result<()> {
        let temporary = target.with_extension("part");
        let _ = tokio::fs::remove_file(&temporary).await;
        let result = async {
            let response = self
                .client
                .get(artifact.url)
                .send()
                .await
                .with_context(|| format!("下载 {}", artifact.url))?
                .error_for_status()
                .with_context(|| format!("下载 {}", artifact.url))?;
            if let Some(length) = response.content_length()
                && length != artifact.bytes
            {
                bail!(
                    "下载大小不匹配：{}，期望 {}，实际 {}",
                    artifact.file_name,
                    artifact.bytes,
                    length
                );
            }
            let mut file = tokio::fs::File::create(&temporary)
                .await
                .with_context(|| format!("创建临时下载文件 {}", temporary.display()))?;
            let mut stream = response.bytes_stream();
            let mut digest = Sha256::new();
            let mut downloaded = 0_u64;
            while let Some(chunk) =
                tokio::time::timeout(std::time::Duration::from_secs(60), stream.next())
                    .await
                    .context("模型下载 60 秒内没有收到数据")?
            {
                let chunk = chunk.with_context(|| format!("下载 {}", artifact.file_name))?;
                downloaded = downloaded.saturating_add(chunk.len() as u64);
                if downloaded > artifact.bytes {
                    bail!("下载文件超过预期大小：{}", artifact.file_name);
                }
                digest.update(&chunk);
                file.write_all(&chunk).await?;
                self.update_runtime(
                    id,
                    "downloading",
                    &format!("正在下载 {}", artifact.file_name),
                    completed_bytes.saturating_add(downloaded),
                    "",
                )
                .await;
            }
            file.flush().await?;
            if downloaded != artifact.bytes {
                bail!(
                    "下载不完整：{}，期望 {}，实际 {}",
                    artifact.file_name,
                    artifact.bytes,
                    downloaded
                );
            }
            let actual = hex::encode(digest.finalize());
            if actual != artifact.sha256 {
                bail!("SHA-256 校验失败：{}", artifact.file_name);
            }
            tokio::fs::rename(&temporary, target)
                .await
                .with_context(|| format!("提交下载文件 {}", target.display()))?;
            Ok(())
        }
        .await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&temporary).await;
        }
        result
    }

    #[cfg(feature = "voice-workers")]
    async fn update_runtime(
        &self,
        id: AssetId,
        state_name: &str,
        detail: &str,
        downloaded_bytes: u64,
        last_error: &str,
    ) {
        let mut state = self.state.lock().await;
        let runtime = state.runtime.get_mut(&id).expect("asset runtime");
        runtime.state = state_name.to_owned();
        runtime.detail = detail.to_owned();
        runtime.downloaded_bytes = downloaded_bytes.min(runtime.total_bytes);
        runtime.last_error = last_error.to_owned();
        runtime.updated_epoch = epoch_seconds();
    }
}

#[cfg(feature = "voice-workers")]
async fn verify_cached(path: PathBuf, artifact: Artifact) -> Result<bool> {
    tokio::task::spawn_blocking(move || verify_artifact(&path, artifact))
        .await
        .context("模型缓存校验任务异常退出")?
}

#[cfg(feature = "voice-workers")]
fn verify_artifact(path: &Path, artifact: Artifact) -> Result<bool> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if metadata.len() != artifact.bytes {
        return Ok(false);
    }
    let mut file = BufReader::new(fs::File::open(path)?);
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(hex::encode(digest.finalize()) == artifact.sha256)
}

fn asset_installed(config: &Config, id: AssetId) -> bool {
    if !cfg!(feature = "voice-workers") {
        return false;
    }
    match id {
        AssetId::VoiceKws => required_paths_exist(&config.voice_model_dir, KWS_REQUIRED),
        AssetId::VoiceAsr => required_paths_exist(&config.asr_model_dir, ASR_REQUIRED),
        AssetId::VoiceTts => {
            required_paths_exist(&config.tts_model_dir, TTS_REQUIRED)
                && config.tts_vocoder.is_file()
                && fs::metadata(&config.tts_vocoder).is_ok_and(|metadata| metadata.len() > 0)
        }
    }
}

fn required_paths_exist(root: &Path, required: &[RequiredPath]) -> bool {
    required.iter().all(|required| {
        let path = root.join(required.path);
        match required.kind {
            RequiredKind::File => {
                path.is_file() && fs::metadata(path).is_ok_and(|metadata| metadata.len() > 0)
            }
            RequiredKind::Directory => path.is_dir(),
        }
    })
}

#[cfg(feature = "voice-workers")]
fn install_parent(config: &Config, id: AssetId) -> Result<PathBuf> {
    let target = match id {
        AssetId::VoiceKws => &config.voice_model_dir,
        AssetId::VoiceAsr => &config.asr_model_dir,
        AssetId::VoiceTts => &config.tts_model_dir,
    };
    target
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow::anyhow!("模型目录缺少父目录：{}", target.display()))
}

#[cfg(feature = "voice-workers")]
fn ensure_free_space(path: &Path, required: u64) -> Result<()> {
    let available = available_space(path)?;
    if available < required {
        bail!(
            "磁盘空间不足：{} 可用，至少需要 {}",
            format_bytes(available),
            format_bytes(required)
        );
    }
    Ok(())
}

#[cfg(all(feature = "voice-workers", unix))]
fn available_space(path: &Path) -> Result<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = CString::new(path.as_os_str().as_bytes()).context("模型目录路径包含 NUL")?;
    let mut value = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(path.as_ptr(), value.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error()).context("读取模型目录磁盘空间");
    }
    let value = unsafe { value.assume_init() };
    let block_size = if value.f_frsize > 0 {
        value.f_frsize
    } else {
        value.f_bsize
    };
    let available_blocks = u64::from(value.f_bavail);
    Ok(available_blocks.saturating_mul(block_size))
}

#[cfg(all(feature = "voice-workers", not(unix)))]
fn available_space(_path: &Path) -> Result<u64> {
    Ok(u64::MAX)
}

#[cfg(feature = "voice-workers")]
fn format_bytes(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    }
}

#[cfg(feature = "voice-workers")]
struct PreparedInstall {
    replacements: Vec<Replacement>,
}

#[cfg(feature = "voice-workers")]
impl PreparedInstall {
    fn commit(self) -> Result<()> {
        commit_replacements(&self.replacements)
    }
}

#[cfg(feature = "voice-workers")]
struct Replacement {
    staged: PathBuf,
    target: PathBuf,
    backup: PathBuf,
}

#[cfg(feature = "voice-workers")]
impl Drop for Replacement {
    fn drop(&mut self) {
        let _ = remove_path(&self.staged);
    }
}

#[cfg(feature = "voice-workers")]
fn prepare_install(id: AssetId, config: &Config, cached: &[PathBuf]) -> Result<PreparedInstall> {
    let nonce = install_nonce()?;
    let replacements = match id {
        AssetId::VoiceKws => vec![prepare_archive_replacement(
            &cached[0],
            &config.voice_model_dir,
            KWS_MODEL,
            KWS_REQUIRED,
            KWS_INSTALLED_BYTES.saturating_add(INSTALL_HEADROOM_BYTES),
            &nonce,
        )?],
        AssetId::VoiceAsr => vec![prepare_archive_replacement(
            &cached[0],
            &config.asr_model_dir,
            ASR_MODEL,
            ASR_REQUIRED,
            ASR_INSTALLED_BYTES.saturating_add(INSTALL_HEADROOM_BYTES),
            &nonce,
        )?],
        AssetId::VoiceTts => vec![
            prepare_archive_replacement(
                &cached[0],
                &config.tts_model_dir,
                TTS_MODEL,
                TTS_REQUIRED,
                TTS_INSTALLED_BYTES.saturating_add(INSTALL_HEADROOM_BYTES),
                &nonce,
            )?,
            prepare_file_replacement(&cached[1], &config.tts_vocoder, &nonce)?,
        ],
    };
    Ok(PreparedInstall { replacements })
}

#[cfg(feature = "voice-workers")]
fn prepare_archive_replacement(
    archive_path: &Path,
    target: &Path,
    archive_root: &str,
    required: &[RequiredPath],
    extracted_limit: u64,
    nonce: &str,
) -> Result<Replacement> {
    use bzip2::read::BzDecoder;
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("模型目录缺少父目录：{}", target.display()))?;
    fs::create_dir_all(parent)?;
    let unpack_root = parent.join(format!(".camera-hub-unpack-{nonce}"));
    let staged = parent.join(format!(".camera-hub-stage-{nonce}"));
    let backup = backup_path(target)?;
    remove_path(&unpack_root)?;
    remove_path(&staged)?;
    recover_backup(target, &backup)?;
    fs::create_dir_all(&unpack_root)?;

    let unpack_result = (|| {
        let file = fs::File::open(archive_path)?;
        let decoder = BzDecoder::new(BufReader::new(file));
        unpack_archive(decoder, &unpack_root, extracted_limit)?;
        let extracted = unpack_root.join(archive_root);
        if !required_paths_exist(&extracted, required) {
            bail!("模型归档缺少必需文件：{}", archive_path.display());
        }
        fs::rename(&extracted, &staged)?;
        Ok(())
    })();
    let _ = fs::remove_dir_all(&unpack_root);
    if let Err(error) = unpack_result {
        let _ = remove_path(&staged);
        return Err(error);
    }
    Ok(Replacement {
        staged,
        target: target.to_path_buf(),
        backup,
    })
}

#[cfg(feature = "voice-workers")]
fn prepare_file_replacement(source: &Path, target: &Path, nonce: &str) -> Result<Replacement> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("模型文件缺少父目录：{}", target.display()))?;
    fs::create_dir_all(parent)?;
    let staged = parent.join(format!(".camera-hub-stage-{nonce}-file"));
    let backup = backup_path(target)?;
    remove_path(&staged)?;
    recover_backup(target, &backup)?;
    fs::copy(source, &staged)?;
    Ok(Replacement {
        staged,
        target: target.to_path_buf(),
        backup,
    })
}

#[cfg(feature = "voice-workers")]
fn unpack_archive(reader: impl Read, destination: &Path, extracted_limit: u64) -> Result<()> {
    let mut archive = tar::Archive::new(reader);
    let mut extracted = 0_u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        {
            bail!("模型归档包含不安全路径：{}", path.display());
        }
        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            bail!("模型归档包含不支持的条目：{}", path.display());
        }
        extracted = extracted.saturating_add(entry.header().size()?);
        if extracted > extracted_limit {
            bail!("模型归档解压后超过安全限制");
        }
        if !entry.unpack_in(destination)? {
            bail!("模型归档条目越过目标目录：{}", path.display());
        }
    }
    Ok(())
}

#[cfg(feature = "voice-workers")]
fn commit_replacements(replacements: &[Replacement]) -> Result<()> {
    let mut backed_up = Vec::<&Replacement>::new();
    for replacement in replacements {
        if replacement.target.exists() {
            if let Err(error) = fs::rename(&replacement.target, &replacement.backup) {
                restore_backups(&backed_up);
                return Err(error)
                    .with_context(|| format!("备份现有模型 {}", replacement.target.display()));
            }
            backed_up.push(replacement);
        }
    }

    let mut committed = Vec::<&Replacement>::new();
    for replacement in replacements {
        if let Err(error) = fs::rename(&replacement.staged, &replacement.target) {
            for committed in committed.iter().rev() {
                let _ = remove_path(&committed.target);
            }
            restore_backups(&backed_up);
            return Err(error)
                .with_context(|| format!("安装模型 {}", replacement.target.display()));
        }
        committed.push(replacement);
    }
    for replacement in backed_up {
        let _ = remove_path(&replacement.backup);
    }
    Ok(())
}

#[cfg(feature = "voice-workers")]
fn backup_path(target: &Path) -> Result<PathBuf> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("模型路径缺少父目录：{}", target.display()))?;
    let name = target
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("模型路径缺少文件名：{}", target.display()))?
        .to_string_lossy();
    Ok(parent.join(format!(".{name}.camera-hub-backup")))
}

#[cfg(feature = "voice-workers")]
fn recover_backup(target: &Path, backup: &Path) -> Result<()> {
    if !backup.exists() {
        return Ok(());
    }
    if target.exists() {
        remove_path(backup)
    } else {
        fs::rename(backup, target).with_context(|| format!("恢复上次安装备份 {}", target.display()))
    }
}

#[cfg(feature = "voice-workers")]
fn restore_backups(backed_up: &[&Replacement]) {
    for replacement in backed_up.iter().rev() {
        let _ = remove_path(&replacement.target);
        let _ = fs::rename(&replacement.backup, &replacement.target);
    }
}

#[cfg(feature = "voice-workers")]
fn remove_path(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path)?,
        Ok(_) => fs::remove_file(path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

#[cfg(feature = "voice-workers")]
fn install_nonce() -> Result<String> {
    let mut random = [0_u8; 8];
    getrandom::fill(&mut random)?;
    Ok(format!("{}-{}", std::process::id(), hex::encode(random)))
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(all(test, feature = "voice-workers"))]
mod tests {
    use super::*;
    use bzip2::Compression;
    use bzip2::write::BzEncoder;
    use std::io::Cursor;

    #[test]
    fn parses_only_known_voice_assets() {
        assert_eq!(AssetId::parse("voice-kws").unwrap(), AssetId::VoiceKws);
        assert_eq!(AssetId::parse("voice-tts").unwrap(), AssetId::VoiceTts);
        assert!(AssetId::parse("ai-gallery").is_err());
    }

    #[test]
    fn unpacks_regular_model_files() {
        let encoder = BzEncoder::new(Vec::new(), Compression::best());
        let mut builder = tar::Builder::new(encoder);
        let data = b"tokens";
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "model/tokens.txt", Cursor::new(data))
            .unwrap();
        let encoder = builder.into_inner().unwrap();
        let archive = encoder.finish().unwrap();

        let root = temporary_root("unpack");
        fs::create_dir_all(&root).unwrap();
        let decoder = bzip2::read::BzDecoder::new(Cursor::new(archive));
        unpack_archive(decoder, &root, 1024).unwrap();
        assert_eq!(fs::read(root.join("model/tokens.txt")).unwrap(), data);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_archive_parent_traversal() {
        let encoder = BzEncoder::new(Vec::new(), Compression::best());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(1);
        header.set_mode(0o644);
        header.set_cksum();
        let result = builder.append_data(&mut header, "../escape", Cursor::new(b"x"));
        assert!(result.is_err());
    }

    #[test]
    fn verifies_cached_artifact_size_and_digest() {
        let root = temporary_root("digest");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("artifact");
        fs::write(&path, b"abc").unwrap();
        let artifact = Artifact {
            file_name: "artifact",
            url: "https://example.invalid/artifact",
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            bytes: 3,
        };
        assert!(verify_artifact(&path, artifact).unwrap());
        fs::write(&path, b"changed").unwrap();
        assert!(!verify_artifact(&path, artifact).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn commits_all_replacements_and_removes_backups() {
        let root = temporary_root("commit");
        fs::create_dir_all(&root).unwrap();
        let first_target = root.join("first");
        let second_target = root.join("second");
        let first_staged = root.join("first-stage");
        let second_staged = root.join("second-stage");
        fs::write(&first_target, b"old-first").unwrap();
        fs::write(&second_target, b"old-second").unwrap();
        fs::write(&first_staged, b"new-first").unwrap();
        fs::write(&second_staged, b"new-second").unwrap();
        let replacements = vec![
            Replacement {
                staged: first_staged,
                backup: backup_path(&first_target).unwrap(),
                target: first_target.clone(),
            },
            Replacement {
                staged: second_staged,
                backup: backup_path(&second_target).unwrap(),
                target: second_target.clone(),
            },
        ];

        commit_replacements(&replacements).unwrap();
        assert_eq!(fs::read(&first_target).unwrap(), b"new-first");
        assert_eq!(fs::read(&second_target).unwrap(), b"new-second");
        assert!(!backup_path(&first_target).unwrap().exists());
        assert!(!backup_path(&second_target).unwrap().exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rolls_back_all_targets_when_commit_fails() {
        let root = temporary_root("rollback");
        fs::create_dir_all(&root).unwrap();
        let first_target = root.join("first");
        let second_target = root.join("second");
        let first_staged = root.join("first-stage");
        let missing_staged = root.join("missing-stage");
        fs::write(&first_target, b"old-first").unwrap();
        fs::write(&second_target, b"old-second").unwrap();
        fs::write(&first_staged, b"new-first").unwrap();
        let replacements = vec![
            Replacement {
                staged: first_staged,
                backup: backup_path(&first_target).unwrap(),
                target: first_target.clone(),
            },
            Replacement {
                staged: missing_staged,
                backup: backup_path(&second_target).unwrap(),
                target: second_target.clone(),
            },
        ];

        assert!(commit_replacements(&replacements).is_err());
        assert_eq!(fs::read(first_target).unwrap(), b"old-first");
        assert_eq!(fs::read(second_target).unwrap(), b"old-second");
        fs::remove_dir_all(root).unwrap();
    }

    fn temporary_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "camera-hub-assets-{label}-{}-{}",
            std::process::id(),
            epoch_seconds()
        ))
    }
}
