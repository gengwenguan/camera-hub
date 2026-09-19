use crate::inference_lock::InferenceLock;
use crate::ir_control::IrControl;
use crate::voice_config::{
    VoiceCommand, VoiceConfig, VoiceEvent, VoiceTestRequest, VoiceTranscribeRequest,
    VoiceWorkerStatus,
};
use crate::voice_nlu::{self, ExecutionState, ParseState};
use crate::voice_tts::{DEFAULT_VOICE_PROFILE_ID, VoiceTtsClient};
use anyhow::{Context, Result, bail};
use clap::Parser;
use reqwest::Client;
use sherpa_onnx::{
    KeywordSpotter, KeywordSpotterConfig, OnlineRecognizer, OnlineRecognizerConfig, OnlineStream,
};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

const MODEL_PROBE_KEYWORD: &str = "x iǎo y ǔ :1.50 #0.05 @小雨";
const WAKE_PHRASE: &str = "小雨";
const WAKE_TRANSCRIBE_WINDOW: Duration = Duration::from_secs(6);
const WAKE_PREROLL_MILLIS: usize = 1_200;
const STATUS_INTERVAL: Duration = Duration::from_secs(5);
const EVENT_LOG_MAX_BYTES: u64 = 4 * 1024 * 1024;
const SPEAK_TTS_TIMEOUT: Duration = Duration::from_secs(10);
const PREWARM_ITEM_TIMEOUT: Duration = Duration::from_secs(60);
const PREWARM_BATCH_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const PREWARM_RETRY_INTERVAL: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy)]
struct ExecutionMode {
    call_url: bool,
    speak_reply: bool,
    voice_clone: bool,
    source: &'static str,
}

struct CaptureContext<'a> {
    inference_lock: &'a InferenceLock,
    client: &'a Client,
    tts: &'a VoiceTtsClient,
    ir: &'a IrControl,
    args: &'a Args,
}

struct AudioPreRoll {
    samples: VecDeque<f32>,
    capacity: usize,
}

impl AudioPreRoll {
    fn new(sample_rate: i32) -> Self {
        let capacity = usize::try_from(sample_rate)
            .unwrap_or_default()
            .saturating_mul(WAKE_PREROLL_MILLIS)
            / 1_000;
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn push(&mut self, samples: &[f32]) {
        if self.capacity == 0 {
            return;
        }
        if samples.len() >= self.capacity {
            self.samples.clear();
            self.samples
                .extend(samples[samples.len() - self.capacity..].iter().copied());
            return;
        }
        let overflow = self
            .samples
            .len()
            .saturating_add(samples.len())
            .saturating_sub(self.capacity);
        if overflow > 0 {
            self.samples.drain(..overflow);
        }
        self.samples.extend(samples.iter().copied());
    }

    fn accept_into(&self, stream: &OnlineStream, sample_rate: i32) {
        let (first, second) = self.samples.as_slices();
        if !first.is_empty() {
            stream.accept_waveform(sample_rate, first);
        }
        if !second.is_empty() {
            stream.accept_waveform(sample_rate, second);
        }
    }
}

struct ActiveTranscription {
    stream: OnlineStream,
    started: Instant,
}

#[derive(Debug, Parser)]
#[command(
    name = "camera-hub worker voice",
    version,
    about = "Local keyword-control worker for camera-hub"
)]
struct Args {
    #[arg(
        long,
        env = "CAMERA_HUB_VOICE_CONFIG_FILE",
        default_value = "/home/android/.config/camera-hub-voice.json"
    )]
    config: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_VOICE_STATUS_FILE",
        default_value = "/home/android/.config/camera-hub-voice-status.json"
    )]
    status: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_VOICE_COMMAND_FILE",
        default_value = "/home/android/.config/camera-hub-voice-command.json"
    )]
    command: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_VOICE_EVENTS_FILE",
        default_value = "/home/android/camera-data/voice/events.jsonl"
    )]
    events: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_VOICE_MODEL_DIR",
        default_value = "/home/android/camera-voice/models/sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01"
    )]
    model_dir: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_ASR_MODEL_DIR",
        default_value = "/home/android/camera-voice/models/sherpa-onnx-streaming-zipformer-zh-14M-2023-02-23"
    )]
    asr_model_dir: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_VOICE_TRANSCRIBE_FILE",
        default_value = "/home/android/.config/camera-hub-voice-transcribe.json"
    )]
    transcribe: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_TTS_URL",
        default_value = "http://127.0.0.1:39081"
    )]
    tts_url: String,

    #[arg(long, env = "CAMERA_HUB_TTS_TOKEN", default_value = "")]
    tts_token: String,

    #[arg(
        long,
        env = "CAMERA_HUB_IR_URL",
        default_value = "http://127.0.0.1:39182"
    )]
    ir_url: String,
}

pub async fn run(args: Vec<OsString>) -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args = Args::parse_from(args);
    let mut status = VoiceWorkerStatus {
        state: "starting".to_owned(),
        model: args.model_dir.display().to_string(),
        updated_epoch: epoch_seconds(),
        ..VoiceWorkerStatus::default()
    };
    write_status(&args.status, &status)?;

    let mut spotter = match create_spotter(&args.model_dir, MODEL_PROBE_KEYWORD) {
        Ok(spotter) => spotter,
        Err(error) => {
            status.state = "failed".to_owned();
            status.last_error = format!("{error:#}");
            write_status(&args.status, &status)?;
            return Err(error);
        }
    };
    let inference_lock = InferenceLock::open()?;
    status.available = true;
    status.state = "disabled".to_owned();
    status.last_error.clear();
    let asr_installed = args.asr_model_dir.join("tokens.txt").is_file();
    status.asr_available = asr_installed;
    status.asr_state = if asr_installed {
        "idle".to_owned()
    } else {
        "missing".to_owned()
    };
    write_status(&args.status, &status)?;

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .build()
        .context("build HTTP client")?;
    let tts = VoiceTtsClient::new(&args.tts_url, &args.tts_token)?;
    let ir = IrControl::from_url(&args.ir_url, PathBuf::from("/dev/peel_ir"))?;
    let mut cooldowns = HashMap::new();
    let mut spotter_keywords = MODEL_PROBE_KEYWORD.to_owned();
    let mut prepared_voice_config_revision = 0_u64;
    let mut prewarm_retry_revision = None;
    let mut prewarm_retry_at = None;
    let mut tts_warning = String::new();
    let mut recognizer: Option<OnlineRecognizer> = None;

    loop {
        let config = match load_config(&args.config) {
            Ok(config) => config,
            Err(error) => {
                status.running = false;
                status.state = "config-error".to_owned();
                status.last_error = format!("{error:#}");
                write_status(&args.status, &status)?;
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        status.capture_device = config.capture_device.clone();
        status.playback_device = config.playback_device.clone();
        status.config_revision = config.revision;

        match take_transcribe_request(&args.transcribe) {
            Ok(Some(request)) => {
                if let Some(recognizer) = ensure_recognizer(
                    &mut recognizer,
                    &args.asr_model_dir,
                    &mut status,
                    &args.status,
                ) {
                    run_transcription(
                        recognizer,
                        &inference_lock,
                        &config,
                        &request,
                        &mut status,
                        &args.status,
                    )
                    .await;
                }
                continue;
            }
            Ok(None) => {}
            Err(error) => {
                status.asr_state = "error".to_owned();
                status.asr_error = format!("{error:#}");
                write_status(&args.status, &status)?;
            }
        }

        match take_test_request(&args.command) {
            Ok(Some(test)) => {
                handle_test(&client, &tts, &config, test, &args, &mut status).await;
                continue;
            }
            Ok(None) => {}
            Err(error) => {
                status.running = false;
                status.state = "test-error".to_owned();
                status.last_error = format!("{error:#}");
                write_status(&args.status, &status)?;
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        }

        if config.voice_profile_revision == 0 {
            prepared_voice_config_revision = 0;
            prewarm_retry_revision = None;
            prewarm_retry_at = None;
            tts_warning.clear();
            status.tts_state = "espeak".to_owned();
        } else if prepared_voice_config_revision != config.revision
            && (prewarm_retry_revision != Some(config.revision)
                || prewarm_retry_at.is_none_or(|retry_at| Instant::now() >= retry_at))
        {
            status.running = false;
            status.state = "preparing-voice".to_owned();
            status.tts_state = "preparing".to_owned();
            status.last_error.clear();
            write_status(&args.status, &status)?;
            match prewarm_with_heartbeat(&tts, &config, &args, &mut status).await {
                Ok(()) => {
                    prepared_voice_config_revision = config.revision;
                    prewarm_retry_revision = None;
                    prewarm_retry_at = None;
                    tts_warning.clear();
                    status.tts_state = "voice-clone-ready".to_owned();
                }
                Err(error) => {
                    prewarm_retry_revision = Some(config.revision);
                    prewarm_retry_at = Some(Instant::now() + PREWARM_RETRY_INTERVAL);
                    tts_warning = format!("声纹回复预生成失败，将回退系统声音：{error:#}");
                    status.tts_state = "fallback".to_owned();
                }
            }
        }

        if !config.enabled {
            status.running = false;
            status.state = "disabled".to_owned();
            status.last_error.clone_from(&tts_warning);
            write_status(&args.status, &status)?;
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }

        let keywords = match listener_keyword_buffer(&config, asr_installed) {
            Ok(keywords) => keywords,
            Err(error) => {
                status.running = false;
                status.state = "config-error".to_owned();
                status.last_error = error.to_string();
                write_status(&args.status, &status)?;
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        if spotter_keywords != keywords {
            match create_spotter(&args.model_dir, &keywords) {
                Ok(next) => {
                    spotter = next;
                    spotter_keywords = keywords;
                }
                Err(error) => {
                    status.running = false;
                    status.state = "model-error".to_owned();
                    status.last_error = format!("{error:#}");
                    write_status(&args.status, &status)?;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            }
        }

        if asr_installed && recognizer.is_none() {
            let _ = ensure_recognizer(
                &mut recognizer,
                &args.asr_model_dir,
                &mut status,
                &args.status,
            );
        }

        status.running = true;
        status.state = "listening".to_owned();
        status.last_error.clone_from(&tts_warning);
        write_status(&args.status, &status)?;
        let context = CaptureContext {
            inference_lock: &inference_lock,
            client: &client,
            tts: &tts,
            ir: &ir,
            args: &args,
        };
        let capture_result = capture_once(
            &spotter,
            recognizer.as_ref(),
            &context,
            &config,
            &mut status,
            &mut cooldowns,
            prewarm_retry_at.filter(|_| prewarm_retry_revision == Some(config.revision)),
        )
        .await;
        restore_asr_idle_state(&mut status);
        if let Err(error) = capture_result {
            status.running = false;
            status.state = "audio-error".to_owned();
            status.last_error = format!("{error:#}");
            write_status(&args.status, &status)?;
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}

fn create_spotter(model_dir: &Path, keywords: &str) -> Result<KeywordSpotter> {
    let mut config = KeywordSpotterConfig::default();
    config.model_config.transducer.encoder = Some(
        model_dir
            .join("encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx")
            .display()
            .to_string(),
    );
    config.model_config.transducer.decoder = Some(
        model_dir
            .join("decoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx")
            .display()
            .to_string(),
    );
    config.model_config.transducer.joiner = Some(
        model_dir
            .join("joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx")
            .display()
            .to_string(),
    );
    config.model_config.tokens = Some(model_dir.join("tokens.txt").display().to_string());
    config.model_config.provider = Some("cpu".to_owned());
    config.model_config.num_threads = 1;
    config.keywords_buf = Some(keywords.to_owned());
    KeywordSpotter::create(&config)
        .ok_or_else(|| anyhow::anyhow!("无法加载 sherpa-onnx 关键词模型"))
}

fn listener_keyword_buffer(config: &VoiceConfig, wake_available: bool) -> Result<String> {
    if wake_available && config.enabled_commands().next().is_none() {
        Ok(MODEL_PROBE_KEYWORD.to_owned())
    } else {
        config.keyword_buffer()
    }
}

fn ensure_recognizer<'a>(
    recognizer: &'a mut Option<OnlineRecognizer>,
    model_dir: &Path,
    status: &mut VoiceWorkerStatus,
    status_path: &Path,
) -> Option<&'a OnlineRecognizer> {
    if recognizer.is_none() {
        status.asr_state = "loading".to_owned();
        status.asr_error.clear();
        let _ = write_status(status_path, status);
        match create_recognizer(model_dir) {
            Ok(created) => {
                status.asr_available = true;
                status.asr_state = "idle".to_owned();
                status.asr_error.clear();
                *recognizer = Some(created);
                let _ = write_status(status_path, status);
            }
            Err(error) => {
                status.asr_available = false;
                status.asr_state = "error".to_owned();
                status.asr_error = format!("{error:#}");
                let _ = write_status(status_path, status);
                return None;
            }
        }
    }
    recognizer.as_ref()
}

fn create_recognizer(model_dir: &Path) -> Result<OnlineRecognizer> {
    let mut config = OnlineRecognizerConfig::default();
    config.model_config.transducer.encoder = Some(
        model_dir
            .join("encoder-epoch-99-avg-1.int8.onnx")
            .display()
            .to_string(),
    );
    config.model_config.transducer.decoder = Some(
        model_dir
            .join("decoder-epoch-99-avg-1.int8.onnx")
            .display()
            .to_string(),
    );
    config.model_config.transducer.joiner = Some(
        model_dir
            .join("joiner-epoch-99-avg-1.int8.onnx")
            .display()
            .to_string(),
    );
    config.model_config.tokens = Some(model_dir.join("tokens.txt").display().to_string());
    config.model_config.provider = Some("cpu".to_owned());
    config.model_config.num_threads = 1;
    config.enable_endpoint = true;
    config.rule1_min_trailing_silence = 2.4;
    config.rule2_min_trailing_silence = 1.2;
    config.rule3_min_utterance_length = 20.0;
    OnlineRecognizer::create(&config)
        .ok_or_else(|| anyhow::anyhow!("无法加载 sherpa-onnx 流式识别模型"))
}

fn decode_transcription(
    recognizer: &OnlineRecognizer,
    inference_lock: &InferenceLock,
    stream: &OnlineStream,
) -> Result<bool> {
    let _guard = inference_lock.lock()?;
    while recognizer.is_ready(stream) {
        recognizer.decode(stream);
    }
    if !recognizer.is_endpoint(stream) {
        return Ok(false);
    }
    let has_text = recognizer
        .get_result(stream)
        .is_some_and(|result| !result.text.trim().is_empty());
    if !has_text {
        recognizer.reset(stream);
    }
    Ok(has_text)
}

fn finish_transcription(
    recognizer: &OnlineRecognizer,
    inference_lock: &InferenceLock,
    stream: &OnlineStream,
) -> Result<String> {
    stream.input_finished();
    let _guard = inference_lock.lock()?;
    while recognizer.is_ready(stream) {
        recognizer.decode(stream);
    }
    Ok(recognizer
        .get_result(stream)
        .map(|result| result.text)
        .unwrap_or_default()
        .trim()
        .to_owned())
}

fn store_transcription_result(status: &mut VoiceWorkerStatus, result: Result<String>) {
    match result {
        Ok(text) => {
            status.asr_state = "ready".to_owned();
            status.asr_transcript = text;
            status.asr_transcript_epoch = epoch_seconds();
            status.asr_error.clear();
        }
        Err(error) => {
            status.asr_state = "error".to_owned();
            status.asr_error = format!("{error:#}");
        }
    }
}

fn restore_asr_idle_state(status: &mut VoiceWorkerStatus) {
    if status.asr_state == "listening" {
        status.asr_state = if status.asr_transcript_epoch == 0 {
            "idle".to_owned()
        } else {
            "ready".to_owned()
        };
    }
    if status.nlu_state == ExecutionState::Listening {
        status.nlu_state = ExecutionState::Ignored;
        status.nlu_message = "本次转写已中断，未执行自然语言指令".to_owned();
        status.nlu_intent = None;
        status.nlu_epoch = epoch_seconds();
    }
}

fn transcribe_request_pending(path: &Path) -> bool {
    match fs::read(path) {
        Ok(data) => serde_json::from_slice::<VoiceTranscribeRequest>(&data)
            .map(|request| request.is_fresh(epoch_seconds()))
            .unwrap_or(false),
        Err(_) => false,
    }
}

fn take_transcribe_request(path: &Path) -> Result<Option<VoiceTranscribeRequest>> {
    let claimed = path.with_extension(format!(
        "json.processing-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    match fs::rename(path, &claimed) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let result = (|| {
        let data = fs::read(&claimed)?;
        let request =
            serde_json::from_slice::<VoiceTranscribeRequest>(&data).context("解析语音转写请求")?;
        if !request.is_fresh(epoch_seconds()) {
            bail!("语音转写请求已过期");
        }
        Ok(Some(request))
    })();
    let _ = fs::remove_file(claimed);
    result
}

/// Web 按需转写：命中请求后打开一次限时录音窗口，用流式识别器转写整句并写回状态。
async fn run_transcription(
    recognizer: &OnlineRecognizer,
    inference_lock: &InferenceLock,
    config: &VoiceConfig,
    request: &VoiceTranscribeRequest,
    status: &mut VoiceWorkerStatus,
    status_path: &Path,
) {
    status.asr_state = "listening".to_owned();
    status.asr_error.clear();
    begin_nlu(status);
    let _ = write_status(status_path, status);
    let result = transcribe_once(
        recognizer,
        inference_lock,
        config,
        request,
        status,
        status_path,
    )
    .await;
    store_transcription_result(status, result);
    preview_transcription(status);
    let _ = write_status(status_path, status);
}

async fn transcribe_once(
    recognizer: &OnlineRecognizer,
    inference_lock: &InferenceLock,
    config: &VoiceConfig,
    request: &VoiceTranscribeRequest,
    status: &mut VoiceWorkerStatus,
    status_path: &Path,
) -> Result<String> {
    let window = request.clamped_window();
    let mut child = spawn_capture(config)?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("无法读取 arecord 音频输出"))?;
    let stream = recognizer.create_stream();
    let mut buffer = vec![0u8; 8192];
    let mut pending_pcm_byte = None;
    let started = Instant::now();
    let mut last_status = Instant::now();

    let outcome = loop {
        if started.elapsed() >= window {
            break Ok(());
        }
        let count = match tokio::time::timeout(Duration::from_millis(500), stdout.read(&mut buffer))
            .await
        {
            Ok(result) => result.context("read arecord audio")?,
            Err(_) => continue,
        };
        if count == 0 {
            let exit = child.wait().await.context("wait for arecord")?;
            break Err(anyhow::anyhow!("arecord 已退出：{exit}"));
        }
        let samples = pcm_i16_to_f32(&buffer[..count], &mut pending_pcm_byte);
        status.audio_rms = rms(&samples);
        stream.accept_waveform(config.capture_rate, &samples);
        if decode_transcription(recognizer, inference_lock, &stream)? {
            break Ok(());
        }
        if last_status.elapsed() >= STATUS_INTERVAL {
            let _ = write_status(status_path, status);
            last_status = Instant::now();
        }
    };

    let text = finish_transcription(recognizer, inference_lock, &stream);
    stop_capture(&mut child).await;
    outcome?;
    text
}

async fn capture_once(
    spotter: &KeywordSpotter,
    recognizer: Option<&OnlineRecognizer>,
    context: &CaptureContext<'_>,
    config: &VoiceConfig,
    status: &mut VoiceWorkerStatus,
    cooldowns: &mut HashMap<String, Instant>,
    wake_at: Option<Instant>,
) -> Result<()> {
    let mut child = spawn_capture(config)?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("无法读取 arecord 音频输出"))?;
    // Without fixed commands, the model's default keyword is also 小雨. Do not
    // create a second stream that could consume it as an unmapped command.
    let command_stream = config
        .enabled_commands()
        .next()
        .map(|_| spotter.create_stream());
    let wake_stream = recognizer.map(|_| spotter.create_stream_with_keywords(MODEL_PROBE_KEYWORD));
    let mut active_transcription: Option<ActiveTranscription> = None;
    let mut pre_roll = AudioPreRoll::new(config.capture_rate);
    let mut buffer = vec![0u8; 8192];
    let mut pending_pcm_byte = None;
    let mut last_status = Instant::now();

    loop {
        if active_transcription
            .as_ref()
            .is_some_and(|active| active.started.elapsed() >= WAKE_TRANSCRIBE_WINDOW)
        {
            stop_capture(&mut child).await;
            let active = active_transcription.take().expect("checked above");
            let result = finish_transcription(
                recognizer.expect("active transcription requires recognizer"),
                context.inference_lock,
                &active.stream,
            );
            complete_wake_transcription(context, config, status, cooldowns, result).await;
            status.state = "listening".to_owned();
            write_status(&context.args.status, status)?;
            return Ok(());
        }
        if wake_at.is_some_and(|deadline| Instant::now() >= deadline) {
            stop_capture(&mut child).await;
            return Ok(());
        }
        let latest = load_config(&context.args.config)?;
        if latest != *config {
            stop_capture(&mut child).await;
            return Ok(());
        }
        match take_test_request(&context.args.command) {
            Ok(Some(test)) => {
                stop_capture(&mut child).await;
                handle_test(
                    context.client,
                    context.tts,
                    config,
                    test,
                    context.args,
                    status,
                )
                .await;
                return Ok(());
            }
            Ok(None) => {}
            Err(error) => {
                status.last_error = format!("{error:#}");
                write_status(&context.args.status, status)?;
            }
        }

        if transcribe_request_pending(&context.args.transcribe) {
            stop_capture(&mut child).await;
            return Ok(());
        }

        let count =
            match tokio::time::timeout(Duration::from_secs(1), stdout.read(&mut buffer)).await {
                Ok(result) => result.context("read arecord audio")?,
                Err(_) => {
                    if last_status.elapsed() >= STATUS_INTERVAL {
                        write_status(&context.args.status, status)?;
                        last_status = Instant::now();
                    }
                    continue;
                }
            };
        if count == 0 {
            let exit = child.wait().await.context("wait for arecord")?;
            bail!("arecord 已退出：{exit}");
        }

        let samples = pcm_i16_to_f32(&buffer[..count], &mut pending_pcm_byte);
        status.audio_rms = rms(&samples);
        pre_roll.push(&samples);
        if let Some(stream) = &command_stream {
            stream.accept_waveform(config.capture_rate, &samples);
        }
        if let Some(active) = &active_transcription {
            active.stream.accept_waveform(config.capture_rate, &samples);
        } else if let Some(stream) = &wake_stream {
            stream.accept_waveform(config.capture_rate, &samples);
        }

        let mut wake_detected = false;
        loop {
            let command_ready = command_stream
                .as_ref()
                .is_some_and(|stream| spotter.is_ready(stream));
            let wake_ready = active_transcription.is_none()
                && wake_stream
                    .as_ref()
                    .is_some_and(|stream| spotter.is_ready(stream));
            if !command_ready && !wake_ready {
                break;
            }
            {
                let _guard = context.inference_lock.lock()?;
                if command_ready {
                    spotter.decode(command_stream.as_ref().expect("checked above"));
                }
                if wake_ready {
                    spotter.decode(wake_stream.as_ref().expect("checked above"));
                }
            }

            if command_ready
                && let Some(stream) = &command_stream
                && let Some(result) = spotter.get_result(stream)
                && !result.keyword.is_empty()
            {
                spotter.reset(stream);
                let phrase = result.keyword.replace('_', "");
                if let Some(command) = command_for_phrase(config, &phrase)
                    && !defer_aircon_keyword(
                        config,
                        &command,
                        recognizer.is_some(),
                        &context.args.ir_url,
                    )
                    && !cooling_down(config, &command, cooldowns)
                {
                    stop_capture(&mut child).await;
                    restore_asr_idle_state(status);
                    status.detected_count = status.detected_count.saturating_add(1);
                    status.last_keyword = command.phrase.clone();
                    status.state = "executing".to_owned();
                    write_status(&context.args.status, status)?;
                    execute_with_heartbeat(
                        context.client,
                        context.tts,
                        config,
                        &command,
                        ExecutionMode {
                            call_url: true,
                            speak_reply: true,
                            voice_clone: config.voice_profile_revision > 0,
                            source: "voice",
                        },
                        context.args,
                        status,
                    )
                    .await;
                    cooldowns.insert(command.id.clone(), Instant::now());
                    status.state = "listening".to_owned();
                    write_status(&context.args.status, status)?;
                    return Ok(());
                }
            }

            if wake_ready {
                let wake_stream = wake_stream.as_ref().expect("checked above");
                if spotter
                    .get_result(wake_stream)
                    .is_some_and(|result| result.keyword.replace('_', "") == WAKE_PHRASE)
                {
                    spotter.reset(wake_stream);
                    wake_detected = true;
                    break;
                }
            }
        }

        if wake_detected {
            let recognizer = recognizer.expect("wake stream requires recognizer");
            let stream = recognizer.create_stream();
            pre_roll.accept_into(&stream, config.capture_rate);
            active_transcription = Some(ActiveTranscription {
                stream,
                started: Instant::now(),
            });
            status.last_keyword = WAKE_PHRASE.to_owned();
            status.state = "transcribing".to_owned();
            status.asr_state = "listening".to_owned();
            status.asr_error.clear();
            begin_nlu(status);
            write_status(&context.args.status, status)?;
        }

        if let Some(active) = &active_transcription
            && decode_transcription(
                recognizer.expect("active transcription requires recognizer"),
                context.inference_lock,
                &active.stream,
            )?
        {
            stop_capture(&mut child).await;
            let active = active_transcription.take().expect("checked above");
            let result = finish_transcription(
                recognizer.expect("active transcription requires recognizer"),
                context.inference_lock,
                &active.stream,
            );
            complete_wake_transcription(context, config, status, cooldowns, result).await;
            status.state = "listening".to_owned();
            write_status(&context.args.status, status)?;
            return Ok(());
        }

        if last_status.elapsed() >= STATUS_INTERVAL {
            write_status(&context.args.status, status)?;
            last_status = Instant::now();
        }
    }
}

fn begin_nlu(status: &mut VoiceWorkerStatus) {
    status.nlu_state = ExecutionState::Listening;
    status.nlu_intent = None;
    status.nlu_message = "正在听完整指令".to_owned();
    status.nlu_epoch = epoch_seconds();
}

/// Used by manual transcription too: this function has no execution capability.
fn preview_transcription(status: &mut VoiceWorkerStatus) {
    status.nlu_epoch = epoch_seconds();
    status.nlu_intent = None;
    if status.asr_state == "error" {
        status.nlu_state = ExecutionState::Failed;
        status.nlu_message = "转写失败，未执行任何操作".to_owned();
        return;
    }
    let parsed = voice_nlu::parse(&status.asr_transcript);
    status.nlu_state = match parsed.state {
        ParseState::Ready => ExecutionState::Preview,
        ParseState::Rejected => ExecutionState::Rejected,
        ParseState::Ignored => ExecutionState::Ignored,
    };
    status.nlu_intent = parsed.intent;
    status.nlu_message = parsed.message;
}

fn defer_aircon_keyword(
    config: &VoiceConfig,
    command: &VoiceCommand,
    asr_available: bool,
    ir_url: &str,
) -> bool {
    if !config.nlu_enabled
        || !asr_available
        || command.method != "POST"
        || !command.body.is_empty()
        || !matches!(
            command.id.as_str(),
            "ac-on" | "ac-off" | "ac-cool" | "ac-dry"
        )
        || voice_nlu::parse(&command.phrase).intent.is_none()
    {
        return false;
    }
    let expected = format!("{}/v1/actions/{}", ir_url.trim_end_matches('/'), command.id);
    matches!(
        (reqwest::Url::parse(&command.url), reqwest::Url::parse(&expected)),
        (Ok(actual), Ok(expected)) if actual == expected
    )
}

async fn complete_wake_transcription(
    context: &CaptureContext<'_>,
    config: &VoiceConfig,
    status: &mut VoiceWorkerStatus,
    cooldowns: &mut HashMap<String, Instant>,
    result: Result<String>,
) {
    store_transcription_result(status, result);
    preview_transcription(status);
    if !config.nlu_enabled {
        if status.nlu_intent.is_some() {
            status.nlu_state = ExecutionState::Disabled;
            status.nlu_message = "自然语言控制未启用，仅展示解析结果".to_owned();
        }
        return;
    }
    let started = Instant::now();
    let mut success = false;
    if let Some(command) = status.nlu_intent.clone() {
        if cooldowns.values().any(|last| {
            last.elapsed() < Duration::from_millis(config.global_cooldown_ms.max(1_000))
        }) {
            status.nlu_state = ExecutionState::Cooldown;
            status.nlu_message = "操作过于频繁，本次未发送红外".to_owned();
        } else {
            status.nlu_state = ExecutionState::Executing;
            status.nlu_message = format!("正在发送：{}", command.label());
            status.state = "executing".to_owned();
            let _ = write_status(&context.args.status, status);
            // Reserve a cooldown on failures too: an interrupted response does
            // not prove that no infrared signal was emitted. Never retry here.
            cooldowns.insert("nlu-aircon".to_owned(), Instant::now());
            match context.ir.send_aircon(&command).await {
                Ok(transmission) => {
                    success = true;
                    status.detected_count = status.detected_count.saturating_add(1);
                    status.nlu_state = ExecutionState::Sent;
                    status.nlu_message = format!(
                        "已发送红外：{}（{} 个脉冲；空调状态无回读）",
                        command.label(),
                        transmission.pulse_count
                    );
                }
                Err(error) => {
                    status.nlu_state = ExecutionState::Failed;
                    status.nlu_message = format!("红外发送失败：{error:#}");
                }
            }
            cooldowns.insert("nlu-aircon".to_owned(), Instant::now());
        }
    }
    status.nlu_epoch = epoch_seconds();
    let _ = write_status(&context.args.status, status);
    // Speak after capture stops and inference guards have been released. The
    // action outcome is final before TTS starts; speech failure never resends IR.
    let reply = match status.nlu_state {
        ExecutionState::Sent => Some(format!(
            "已发送，{}",
            status.nlu_intent.as_ref().unwrap().label()
        )),
        ExecutionState::Rejected => Some("没有听懂，请说，例如，把空调调到二十度".to_owned()),
        ExecutionState::Failed => Some("操作失败，请查看页面上的原因".to_owned()),
        _ => None,
    };
    if let Some(reply) = reply {
        let speech = tokio::time::timeout(
            Duration::from_secs(20),
            speak(
                context.tts,
                config,
                &reply,
                config.voice_profile_revision > 0,
            ),
        );
        tokio::pin!(speech);
        let mut heartbeat = tokio::time::interval(STATUS_INTERVAL);
        let warning = loop {
            tokio::select! {
                result = &mut speech => break match result {
                    Ok(Ok(warning)) => warning,
                    Ok(Err(error)) => Some(format!("回复播放失败：{error:#}")),
                    Err(_) => Some("回复播放超时".to_owned()),
                },
                _ = heartbeat.tick() => { let _ = write_status(&context.args.status, status); }
            }
        };
        if let Some(warning) = warning {
            status.last_error = warning.clone();
            status.nlu_message.push_str(&format!("；{warning}"));
        }
    }
    if status.nlu_state != ExecutionState::Ignored {
        let event = VoiceEvent {
            epoch: status.nlu_epoch,
            command_id: "nlu-aircon".to_owned(),
            phrase: status.asr_transcript.clone(),
            source: "nlu".to_owned(),
            success,
            http_status: if success { 200 } else { 0 },
            elapsed_ms: started.elapsed().as_millis() as u64,
            message: status.nlu_message.clone(),
        };
        if let Err(error) = append_event(&context.args.events, &event) {
            status.last_error = format!("写入语音事件失败：{error:#}");
        }
    }
}

fn command_for_phrase(config: &VoiceConfig, phrase: &str) -> Option<VoiceCommand> {
    config
        .commands
        .iter()
        .find(|command| {
            command.enabled && command.phrases.iter().any(|candidate| candidate == phrase)
        })
        .cloned()
        .map(|mut command| {
            command.phrase = phrase.to_owned();
            command
        })
}

fn spawn_capture(config: &VoiceConfig) -> Result<Child> {
    let capture_rate = config.capture_rate.to_string();
    Command::new("arecord")
        .args([
            "-q",
            "-D",
            &config.capture_device,
            "-t",
            "raw",
            "-f",
            "S16_LE",
            "-r",
            &capture_rate,
            "-c",
            "1",
            "--period-size=1024",
            "--buffer-size=4096",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .context("启动 arecord 失败")
}

async fn stop_capture(child: &mut Child) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}

fn cooling_down(
    config: &VoiceConfig,
    command: &VoiceCommand,
    cooldowns: &HashMap<String, Instant>,
) -> bool {
    let command_cooldown = Duration::from_millis(command.cooldown_ms);
    let global_cooldown = Duration::from_millis(config.global_cooldown_ms);
    cooldowns.iter().any(|(id, triggered)| {
        triggered.elapsed()
            < if id == &command.id {
                command_cooldown.max(global_cooldown)
            } else {
                global_cooldown
            }
    })
}

async fn handle_test(
    client: &Client,
    tts: &VoiceTtsClient,
    config: &VoiceConfig,
    request: VoiceTestRequest,
    args: &Args,
    status: &mut VoiceWorkerStatus,
) {
    let Some(command) = config
        .commands
        .iter()
        .find(|command| command.id == request.command_id)
    else {
        status.last_error = "测试命令不存在".to_owned();
        let _ = write_status(&args.status, status);
        return;
    };
    status.state = "testing".to_owned();
    let _ = write_status(&args.status, status);
    execute_with_heartbeat(
        client,
        tts,
        config,
        command,
        ExecutionMode {
            call_url: request.call_url,
            speak_reply: request.speak_reply,
            voice_clone: config.voice_profile_revision > 0,
            source: "test",
        },
        args,
        status,
    )
    .await;
    status.state = if config.enabled {
        "listening".to_owned()
    } else {
        "disabled".to_owned()
    };
    let _ = write_status(&args.status, status);
}

async fn execute_with_heartbeat(
    client: &Client,
    tts: &VoiceTtsClient,
    config: &VoiceConfig,
    command: &VoiceCommand,
    mode: ExecutionMode,
    args: &Args,
    status: &mut VoiceWorkerStatus,
) {
    let execution = execute_command(client, tts, config, command, mode, args);
    tokio::pin!(execution);
    let mut heartbeat = tokio::time::interval(STATUS_INTERVAL);
    heartbeat.tick().await;
    let last_error = loop {
        tokio::select! {
            last_error = &mut execution => break last_error,
            _ = heartbeat.tick() => {
                let _ = write_status(&args.status, status);
            }
        }
    };
    status.last_error = last_error.unwrap_or_default();
}

async fn execute_command(
    client: &Client,
    tts: &VoiceTtsClient,
    config: &VoiceConfig,
    command: &VoiceCommand,
    mode: ExecutionMode,
    args: &Args,
) -> Option<String> {
    let started = Instant::now();
    let (action_success, http_status, action_message) = if mode.call_url {
        call_command_url(client, config, command).await
    } else {
        (true, 0, "仅测试回复".to_owned())
    };
    let mut success = action_success;
    let mut message = action_message;
    let mut warning = None;
    if mode.speak_reply {
        let reply = if action_success {
            &command.reply
        } else {
            &config.failure_reply
        };
        match speak(tts, config, reply, mode.voice_clone).await {
            Ok(Some(fallback)) => {
                message = format!("{message}；{fallback}");
                warning = Some(fallback);
            }
            Ok(None) => {}
            Err(error) => {
                success = false;
                let speech_error = format!("播放回复失败：{error:#}");
                message = if action_success {
                    speech_error
                } else {
                    format!("{message}；{speech_error}")
                };
            }
        }
    }
    let event = VoiceEvent {
        epoch: epoch_seconds(),
        command_id: command.id.clone(),
        phrase: command.phrase.clone(),
        source: mode.source.to_owned(),
        success,
        http_status,
        elapsed_ms: started.elapsed().as_millis() as u64,
        message: message.clone(),
    };
    if let Err(error) = append_event(&args.events, &event) {
        let event_error = format!("写入语音事件失败：{error:#}");
        Some(if success {
            event_error
        } else {
            format!("{message}；{event_error}")
        })
    } else if success {
        warning
    } else {
        Some(message)
    }
}

async fn call_command_url(
    client: &Client,
    config: &VoiceConfig,
    command: &VoiceCommand,
) -> (bool, u16, String) {
    if command.url.is_empty() {
        return (false, 0, "命令 URL 未配置".to_owned());
    }
    let mut request = if command.method == "POST" {
        client.post(&command.url)
    } else {
        client.get(&command.url)
    };
    if command.method == "POST" && !command.body.is_empty() {
        request = request
            .header("Content-Type", "application/json")
            .body(command.body.clone());
    }
    match tokio::time::timeout(
        Duration::from_millis(config.request_timeout_ms),
        request.send(),
    )
    .await
    {
        Ok(Ok(response)) => {
            let code = response.status().as_u16();
            (response.status().is_success(), code, format!("HTTP {code}"))
        }
        Ok(Err(error)) => (false, 0, format!("请求失败：{error}")),
        Err(_) => (false, 0, "请求超时".to_owned()),
    }
}

async fn speak(
    tts: &VoiceTtsClient,
    config: &VoiceConfig,
    text: &str,
    voice_clone: bool,
) -> Result<Option<String>> {
    let wav =
        std::env::temp_dir().join(format!("camera-hub-voice-reply-{}.wav", std::process::id()));
    if voice_clone {
        match tokio::time::timeout(
            SPEAK_TTS_TIMEOUT + Duration::from_secs(1),
            tts.synthesize_with_timeout(DEFAULT_VOICE_PROFILE_ID, text, SPEAK_TTS_TIMEOUT),
        )
        .await
        {
            Ok(Ok(data)) => {
                fs::write(&wav, data)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&wav, fs::Permissions::from_mode(0o600))?;
                }
                let result = play_wav(&wav, &config.playback_device).await;
                let _ = fs::remove_file(&wav);
                result?;
                return Ok(None);
            }
            result => {
                let error = match result {
                    Ok(Err(error)) => format!("{error:#}"),
                    Err(_) => "请求超过 11 秒".to_owned(),
                    Ok(Ok(_)) => unreachable!(),
                };
                speak_espeak(text, &config.playback_device, config.playback_volume, &wav).await?;
                return Ok(Some(format!("声纹服务不可用，已回退系统声音：{error}")));
            }
        }
    }
    speak_espeak(text, &config.playback_device, config.playback_volume, &wav).await?;
    Ok(None)
}

async fn speak_espeak(
    text: &str,
    playback_device: &str,
    playback_volume: u8,
    wav: &Path,
) -> Result<()> {
    let playback_volume = playback_volume.to_string();
    let result = async {
        let status = Command::new("espeak-ng")
            .args(["-v", "cmn", "-s", "145", "-a", &playback_volume, "-w"])
            .arg(wav)
            .arg(text)
            .kill_on_drop(true)
            .status()
            .await
            .context("启动 espeak-ng")?;
        if !status.success() {
            bail!("espeak-ng 退出：{status}");
        }
        play_wav(wav, playback_device).await
    }
    .await;
    let _ = fs::remove_file(wav);
    result
}

async fn play_wav(wav: &Path, playback_device: &str) -> Result<()> {
    let status = Command::new("aplay")
        .args(["-q", "-D", playback_device])
        .arg(wav)
        .kill_on_drop(true)
        .status()
        .await
        .context("启动 aplay")?;
    if !status.success() {
        bail!("aplay 退出：{status}");
    }
    Ok(())
}

async fn prewarm_with_heartbeat(
    tts: &VoiceTtsClient,
    config: &VoiceConfig,
    args: &Args,
    status: &mut VoiceWorkerStatus,
) -> Result<()> {
    let prewarm = prewarm_replies(tts, config);
    tokio::pin!(prewarm);
    let deadline = tokio::time::sleep(PREWARM_BATCH_TIMEOUT);
    tokio::pin!(deadline);
    let mut heartbeat = tokio::time::interval(STATUS_INTERVAL);
    heartbeat.tick().await;
    loop {
        tokio::select! {
            result = &mut prewarm => return result,
            _ = &mut deadline => bail!("回复预生成超过 5 分钟"),
            _ = heartbeat.tick() => {
                let _ = write_status(&args.status, status);
            }
        }
    }
}

async fn prewarm_replies(tts: &VoiceTtsClient, config: &VoiceConfig) -> Result<()> {
    let mut replies = config
        .commands
        .iter()
        .map(|command| command.reply.clone())
        .collect::<BTreeSet<_>>();
    replies.insert(config.failure_reply.clone());
    for reply in replies {
        tokio::time::timeout(
            PREWARM_ITEM_TIMEOUT + Duration::from_secs(2),
            tts.synthesize_with_timeout(DEFAULT_VOICE_PROFILE_ID, &reply, PREWARM_ITEM_TIMEOUT),
        )
        .await
        .context("单条回复预生成超时")??;
    }
    Ok(())
}

fn load_config(path: &Path) -> Result<VoiceConfig> {
    let data = fs::read(path).with_context(|| format!("读取 {}", path.display()))?;
    serde_json::from_slice::<VoiceConfig>(&data)
        .with_context(|| format!("解析 {}", path.display()))?
        .normalize()
}

fn take_test_request(path: &Path) -> Result<Option<VoiceTestRequest>> {
    let claimed = path.with_extension(format!(
        "json.processing-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    match fs::rename(path, &claimed) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let result = (|| {
        let data = fs::read(&claimed)?;
        let request =
            serde_json::from_slice::<VoiceTestRequest>(&data).context("解析语音测试请求")?;
        if !request.is_fresh(epoch_seconds()) {
            bail!("语音测试请求已过期");
        }
        Ok(Some(request))
    })();
    let _ = fs::remove_file(claimed);
    result
}

fn pcm_i16_to_f32(data: &[u8], pending: &mut Option<u8>) -> Vec<f32> {
    let mut samples = Vec::with_capacity((data.len() + usize::from(pending.is_some())) / 2);
    let mut offset = 0;
    if let Some(low) = pending.take() {
        let Some(high) = data.first() else {
            *pending = Some(low);
            return samples;
        };
        samples.push(i16::from_le_bytes([low, *high]) as f32 / 32_768.0);
        offset = 1;
    }
    let complete = &data[offset..];
    samples.extend(
        complete
            .chunks_exact(2)
            .map(|sample| i16::from_le_bytes([sample[0], sample[1]]) as f32 / 32_768.0),
    );
    if complete.len() & 1 != 0 {
        *pending = complete.last().copied();
    }
    samples
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum = samples
        .iter()
        .map(|sample| f64::from(*sample) * f64::from(*sample))
        .sum::<f64>();
    (sum / samples.len() as f64).sqrt() as f32
}

fn write_status(path: &Path, status: &VoiceWorkerStatus) -> Result<()> {
    let mut status = status.clone();
    status.updated_epoch = epoch_seconds();
    write_json(path, &status)
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&temporary, path)?;
    Ok(())
}

fn append_event(path: &Path, event: &VoiceEvent) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::metadata(path).is_ok_and(|metadata| metadata.len() >= EVENT_LOG_MAX_BYTES) {
        let rotated = path.with_extension("jsonl.1");
        match fs::remove_file(&rotated) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        fs::rename(path, rotated)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    serde_json::to_writer(&mut file, event)?;
    file.write_all(b"\n")?;
    Ok(())
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_parse_preview_never_marks_an_action_executed() {
        let mut status = VoiceWorkerStatus::default();
        begin_nlu(&mut status);
        store_transcription_result(&mut status, Ok("空调20度".to_owned()));
        preview_transcription(&mut status);
        assert_eq!(status.nlu_state, ExecutionState::Preview);
        assert_eq!(
            status.nlu_intent,
            Some(crate::ir::AirconCommand::Set {
                temperature_c: 20,
                mode: crate::ir::AcMode::Cool,
                eco: false,
            })
        );
        assert_eq!(status.detected_count, 0);
        store_transcription_result(&mut status, Err(anyhow::anyhow!("decode failed")));
        preview_transcription(&mut status);
        assert_eq!(status.nlu_state, ExecutionState::Failed);
        assert!(status.nlu_intent.is_none());
        store_transcription_result(&mut status, Ok("不要开空调".to_owned()));
        preview_transcription(&mut status);
        assert_eq!(status.nlu_state, ExecutionState::Rejected);
        assert!(status.nlu_intent.is_none());
    }

    #[test]
    fn interrupted_wake_clears_pending_nlu_state() {
        let mut status = VoiceWorkerStatus::default();
        begin_nlu(&mut status);
        status.asr_state = "listening".to_owned();
        restore_asr_idle_state(&mut status);
        assert_eq!(status.asr_state, "idle");
        assert_eq!(status.nlu_state, ExecutionState::Ignored);
        assert!(status.nlu_intent.is_none());
    }

    #[test]
    fn defers_only_builtin_aircon_prefixes_with_usable_asr() {
        let mut config = VoiceConfig {
            nlu_enabled: true,
            ..VoiceConfig::default()
        };
        let mut command = VoiceCommand {
            id: "ac-on".to_owned(),
            phrase: "小雨打开空调".to_owned(),
            method: "POST".to_owned(),
            url: "http://127.0.0.1:39182/v1/actions/ac-on".to_owned(),
            ..VoiceCommand::default()
        };
        let base = "http://127.0.0.1:39182";
        assert!(defer_aircon_keyword(&config, &command, true, base));
        assert!(!defer_aircon_keyword(&config, &command, false, base));
        config.nlu_enabled = false;
        assert!(!defer_aircon_keyword(&config, &command, true, base));
        config.nlu_enabled = true;
        assert!(!defer_aircon_keyword(
            &config,
            &command,
            true,
            "http://127.0.0.1:40000"
        ));
        command.id = "custom-ac-on".to_owned();
        assert!(!defer_aircon_keyword(&config, &command, true, base));
        command.id = "ac-on".to_owned();
        command.phrase = "小雨我要睡觉".to_owned();
        assert!(!defer_aircon_keyword(&config, &command, true, base));
        command.phrase = "小雨打开空调".to_owned();
        command.url.push_str("?custom=true");
        assert!(!defer_aircon_keyword(&config, &command, true, base));
    }

    #[test]
    fn converts_pcm_and_computes_rms() {
        let mut pending = None;
        let samples = pcm_i16_to_f32(&[0, 0, 0xff, 0x7f, 0], &mut pending);
        assert_eq!(pending, Some(0));
        let tail = pcm_i16_to_f32(&[0x80], &mut pending);
        let samples = [samples, tail].concat();
        assert_eq!(samples.len(), 3);
        assert!(rms(&samples) > 0.8);
        assert_eq!(pending, None);
    }

    #[test]
    fn keeps_only_the_latest_wake_preroll_samples() {
        let mut pre_roll = AudioPreRoll {
            samples: VecDeque::new(),
            capacity: 4,
        };
        pre_roll.push(&[1.0, 2.0, 3.0]);
        pre_roll.push(&[4.0, 5.0, 6.0]);

        assert_eq!(
            pre_roll.samples.iter().copied().collect::<Vec<_>>(),
            vec![3.0, 4.0, 5.0, 6.0]
        );

        pre_roll.push(&[7.0, 8.0, 9.0, 10.0, 11.0]);
        assert_eq!(
            pre_roll.samples.iter().copied().collect::<Vec<_>>(),
            vec![8.0, 9.0, 10.0, 11.0]
        );
    }

    #[test]
    fn restores_asr_state_after_an_interrupted_wake_window() {
        let mut status = VoiceWorkerStatus {
            asr_state: "listening".to_owned(),
            ..VoiceWorkerStatus::default()
        };
        restore_asr_idle_state(&mut status);
        assert_eq!(status.asr_state, "idle");

        status.asr_state = "listening".to_owned();
        status.asr_transcript_epoch = 1;
        restore_asr_idle_state(&mut status);
        assert_eq!(status.asr_state, "ready");
    }

    #[test]
    fn keeps_wake_listener_available_without_fixed_commands() {
        let config = VoiceConfig::default();
        assert_eq!(
            listener_keyword_buffer(&config, true).unwrap(),
            MODEL_PROBE_KEYWORD
        );
        assert!(listener_keyword_buffer(&config, false).is_err());
    }

    #[test]
    fn rejects_and_removes_stale_test_request() {
        let path = std::env::temp_dir().join(format!(
            "camera-hub-voice-test-{}-{}.json",
            std::process::id(),
            epoch_seconds()
        ));
        let request = VoiceTestRequest {
            command_id: "light-on".to_owned(),
            call_url: true,
            speak_reply: false,
            created_epoch: epoch_seconds()
                .saturating_sub(crate::voice_config::VOICE_TEST_REQUEST_MAX_AGE_SECS + 1),
        };
        write_json(&path, &request).unwrap();

        assert!(take_test_request(&path).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn detects_only_fresh_transcribe_requests() {
        let path = std::env::temp_dir().join(format!(
            "camera-hub-voice-transcribe-{}-{}.json",
            std::process::id(),
            epoch_seconds()
        ));
        assert!(!transcribe_request_pending(&path));

        let fresh = VoiceTranscribeRequest {
            window_ms: 6_000,
            created_epoch: epoch_seconds(),
        };
        write_json(&path, &fresh).unwrap();
        assert!(transcribe_request_pending(&path));

        let stale = VoiceTranscribeRequest {
            window_ms: 6_000,
            created_epoch: epoch_seconds()
                .saturating_sub(crate::voice_config::VOICE_TEST_REQUEST_MAX_AGE_SECS + 1),
        };
        write_json(&path, &stale).unwrap();
        assert!(!transcribe_request_pending(&path));

        let claimed = take_transcribe_request(&path);
        assert!(claimed.is_err());
        assert!(!path.exists());
    }

    #[test]
    fn applies_global_and_per_command_cooldowns() {
        let config = VoiceConfig::default();
        let command = config.commands[0].clone();
        let other = config.commands[1].clone();
        let mut cooldowns = HashMap::new();

        cooldowns.insert(other.id, Instant::now());
        assert!(cooling_down(&config, &command, &cooldowns));

        cooldowns.clear();
        cooldowns.insert(
            command.id.clone(),
            Instant::now() - Duration::from_millis(command.cooldown_ms + 1),
        );
        assert!(!cooling_down(&config, &command, &cooldowns));
    }

    #[test]
    fn resolves_alias_to_the_same_command_and_keeps_matched_phrase() {
        let mut config = VoiceConfig::default();
        config.commands[0].enabled = true;
        config.commands[0].phrases.push("小雨打开灯".to_owned());

        let command = command_for_phrase(&config, "小雨打开灯").unwrap();

        assert_eq!(command.id, "light-on");
        assert_eq!(command.phrase, "小雨打开灯");
    }
}
