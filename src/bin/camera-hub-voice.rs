#[path = "../inference_lock.rs"]
mod inference_lock;
#[path = "../voice_config.rs"]
mod voice_config;
#[path = "../voice_tts.rs"]
mod voice_tts;

use anyhow::{Context, Result, bail};
use clap::Parser;
use inference_lock::InferenceLock;
use reqwest::Client;
use sherpa_onnx::{KeywordSpotter, KeywordSpotterConfig};
use std::collections::{BTreeSet, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use voice_config::{VoiceCommand, VoiceConfig, VoiceEvent, VoiceTestRequest, VoiceWorkerStatus};
use voice_tts::{DEFAULT_VOICE_PROFILE_ID, VoiceTtsClient};

const MODEL_PROBE_KEYWORD: &str = "x iǎo y ǔ :1.50 #0.45 @小雨";
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
    args: &'a Args,
}

#[derive(Debug, Parser)]
#[command(version, about = "Local keyword-control worker for camera-hub")]
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
        env = "CAMERA_HUB_TTS_URL",
        default_value = "http://127.0.0.1:39081"
    )]
    tts_url: String,

    #[arg(long, env = "CAMERA_HUB_TTS_TOKEN", default_value = "")]
    tts_token: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args = Args::parse();
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
    write_status(&args.status, &status)?;

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .build()
        .context("build HTTP client")?;
    let tts = VoiceTtsClient::new(&args.tts_url, &args.tts_token)?;
    let mut cooldowns = HashMap::new();
    let mut spotter_keywords = MODEL_PROBE_KEYWORD.to_owned();
    let mut prepared_voice_config_revision = 0_u64;
    let mut prewarm_retry_revision = None;
    let mut prewarm_retry_at = None;
    let mut tts_warning = String::new();

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

        let keywords = match config.keyword_buffer() {
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

        status.running = true;
        status.state = "listening".to_owned();
        status.last_error.clone_from(&tts_warning);
        write_status(&args.status, &status)?;
        let context = CaptureContext {
            inference_lock: &inference_lock,
            client: &client,
            tts: &tts,
            args: &args,
        };
        if let Err(error) = capture_once(
            &spotter,
            &context,
            &config,
            &mut status,
            &mut cooldowns,
            prewarm_retry_at.filter(|_| prewarm_retry_revision == Some(config.revision)),
        )
        .await
        {
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

async fn capture_once(
    spotter: &KeywordSpotter,
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
    let stream = spotter.create_stream();
    let mut buffer = vec![0u8; 8192];
    let mut pending_pcm_byte = None;
    let mut last_status = Instant::now();

    loop {
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
        stream.accept_waveform(config.capture_rate, &samples);
        while spotter.is_ready(&stream) {
            let _guard = context.inference_lock.lock()?;
            spotter.decode(&stream);
            drop(_guard);
            let Some(result) = spotter.get_result(&stream) else {
                continue;
            };
            if result.keyword.is_empty() {
                continue;
            }
            spotter.reset(&stream);
            let phrase = result.keyword.replace('_', "");
            let Some(command) = config
                .commands
                .iter()
                .find(|command| command.enabled && command.phrase == phrase)
                .cloned()
            else {
                continue;
            };
            if cooling_down(config, &command, cooldowns) {
                continue;
            }
            stop_capture(&mut child).await;
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

        if last_status.elapsed() >= STATUS_INTERVAL {
            write_status(&context.args.status, status)?;
            last_status = Instant::now();
        }
    }
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
                .saturating_sub(voice_config::VOICE_TEST_REQUEST_MAX_AGE_SECS + 1),
        };
        write_json(&path, &request).unwrap();

        assert!(take_test_request(&path).is_err());
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
}
