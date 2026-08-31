#[path = "../inference_lock.rs"]
mod inference_lock;
#[path = "../voice_config.rs"]
mod voice_config;

use anyhow::{Context, Result, bail};
use clap::Parser;
use inference_lock::InferenceLock;
use reqwest::Client;
use sherpa_onnx::{KeywordSpotter, KeywordSpotterConfig};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use voice_config::{VoiceCommand, VoiceConfig, VoiceEvent, VoiceTestRequest, VoiceWorkerStatus};

const MODEL_PROBE_KEYWORD: &str = "x iǎo y ǔ :1.50 #0.45 @小雨";

#[derive(Debug, Parser)]
#[command(version, about = "Local keyword-control worker for camera-hub")]
struct Args {
    #[arg(long, default_value = "/home/android/.config/camera-hub-voice.json")]
    config: PathBuf,

    #[arg(
        long,
        default_value = "/home/android/.config/camera-hub-voice-status.json"
    )]
    status: PathBuf,

    #[arg(
        long,
        default_value = "/home/android/.config/camera-hub-voice-command.json"
    )]
    command: PathBuf,

    #[arg(long, default_value = "/home/android/camera-data/voice/events.jsonl")]
    events: PathBuf,

    #[arg(
        long,
        default_value = "/home/android/camera-voice/models/sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01"
    )]
    model_dir: PathBuf,
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
    let mut cooldowns = HashMap::new();
    let mut spotter_revision = 0_u64;

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

        if let Some(test) = take_test_request(&args.command)? {
            handle_test(&client, &config, test, &args, &mut status).await;
            continue;
        }

        if !config.enabled {
            status.running = false;
            status.state = "disabled".to_owned();
            status.last_error.clear();
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
        if spotter_revision != config.revision {
            spotter = create_spotter(&args.model_dir, &keywords)?;
            spotter_revision = config.revision;
        }

        status.running = true;
        status.state = "listening".to_owned();
        status.last_error.clear();
        write_status(&args.status, &status)?;
        if let Err(error) = capture_once(
            &spotter,
            &inference_lock,
            &client,
            &config,
            &args,
            &mut status,
            &mut cooldowns,
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
    inference_lock: &InferenceLock,
    client: &Client,
    config: &VoiceConfig,
    args: &Args,
    status: &mut VoiceWorkerStatus,
    cooldowns: &mut HashMap<String, Instant>,
) -> Result<()> {
    let mut child = spawn_capture(config)?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("无法读取 arecord 音频输出"))?;
    let stream = spotter.create_stream();
    let mut buffer = vec![0u8; 8192];
    let mut last_status = Instant::now();

    loop {
        if let Some(test) = take_test_request(&args.command)? {
            stop_capture(&mut child).await;
            handle_test(client, config, test, args, status).await;
            return Ok(());
        }
        let latest = load_config(&args.config)?;
        if latest.revision != config.revision || latest.enabled != config.enabled {
            stop_capture(&mut child).await;
            return Ok(());
        }

        let count =
            match tokio::time::timeout(Duration::from_secs(1), stdout.read(&mut buffer)).await {
                Ok(result) => result.context("read arecord audio")?,
                Err(_) => {
                    if last_status.elapsed() >= Duration::from_secs(5) {
                        write_status(&args.status, status)?;
                        last_status = Instant::now();
                    }
                    continue;
                }
            };
        if count == 0 {
            let exit = child.wait().await.context("wait for arecord")?;
            bail!("arecord 已退出：{exit}");
        }

        let samples = pcm_i16_to_f32(&buffer[..count]);
        status.audio_rms = rms(&samples);
        stream.accept_waveform(config.capture_rate, &samples);
        while spotter.is_ready(&stream) {
            let _guard = inference_lock.lock()?;
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
            write_status(&args.status, status)?;
            execute_command(client, config, &command, true, true, "voice", args, status).await;
            cooldowns.insert(command.id.clone(), Instant::now());
            status.state = "listening".to_owned();
            write_status(&args.status, status)?;
            return Ok(());
        }

        if last_status.elapsed() >= Duration::from_secs(5) {
            write_status(&args.status, status)?;
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
        .stderr(Stdio::null())
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
    execute_command(
        client,
        config,
        command,
        request.call_url,
        request.speak_reply,
        "test",
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

#[allow(clippy::too_many_arguments)]
async fn execute_command(
    client: &Client,
    config: &VoiceConfig,
    command: &VoiceCommand,
    call_url: bool,
    speak_reply: bool,
    source: &str,
    args: &Args,
    status: &mut VoiceWorkerStatus,
) {
    let started = Instant::now();
    let (success, http_status, message) = if call_url {
        call_command_url(client, config, command).await
    } else {
        (true, 0, "仅测试回复".to_owned())
    };
    if speak_reply {
        let reply = if success {
            &command.reply
        } else {
            &config.failure_reply
        };
        if let Err(error) = speak(reply, &config.playback_device, config.playback_volume).await {
            status.last_error = format!("播放回复失败：{error:#}");
        }
    }
    let event = VoiceEvent {
        epoch: epoch_seconds(),
        command_id: command.id.clone(),
        phrase: command.phrase.clone(),
        source: source.to_owned(),
        success,
        http_status,
        elapsed_ms: started.elapsed().as_millis() as u64,
        message: message.clone(),
    };
    if let Err(error) = append_event(&args.events, &event) {
        status.last_error = format!("写入语音事件失败：{error:#}");
    } else if success {
        status.last_error.clear();
    } else {
        status.last_error = message;
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

async fn speak(text: &str, playback_device: &str, playback_volume: u8) -> Result<()> {
    let wav =
        std::env::temp_dir().join(format!("camera-hub-voice-reply-{}.wav", std::process::id()));
    let playback_volume = playback_volume.to_string();
    let status = Command::new("espeak-ng")
        .args(["-v", "cmn", "-s", "145", "-a", &playback_volume, "-w"])
        .arg(&wav)
        .arg(text)
        .status()
        .await
        .context("启动 espeak-ng")?;
    if !status.success() {
        bail!("espeak-ng 退出：{status}");
    }
    let status = Command::new("aplay")
        .args(["-q", "-D", playback_device])
        .arg(&wav)
        .status()
        .await
        .context("启动 aplay")?;
    let _ = fs::remove_file(&wav);
    if !status.success() {
        bail!("aplay 退出：{status}");
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
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let _ = fs::remove_file(path);
    Ok(Some(
        serde_json::from_slice(&data).context("解析语音测试请求")?,
    ))
}

fn pcm_i16_to_f32(data: &[u8]) -> Vec<f32> {
    data.chunks_exact(2)
        .map(|sample| i16::from_le_bytes([sample[0], sample[1]]) as f32 / 32_768.0)
        .collect()
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
    fs::rename(&temporary, path)?;
    Ok(())
}

fn append_event(path: &Path, event: &VoiceEvent) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
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
        let samples = pcm_i16_to_f32(&[0, 0, 0xff, 0x7f, 0, 0x80]);
        assert_eq!(samples.len(), 3);
        assert!(rms(&samples) > 0.8);
    }
}
