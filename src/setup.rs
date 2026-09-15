use anyhow::{Context, Result, bail};
use camera_hub::ddns::DdnsConfigStore;
use clap::{Parser, ValueEnum};
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

const OBSOLETE_KEYS: &[&str] = &[
    "CAMERA_HUB_SPEECH_TRANSCRIBE",
    "CAMERA_HUB_SPEECH_SUMMARIZE",
    "CAMERA_HUB_AI_SNAPSHOT_RETAIN_DAYS",
    "CAMERA_HUB_VOICE_STUDIO_ENABLED",
    "CAMERA_HUB_VOICE_STUDIO_TOKEN",
];

#[derive(Clone, Copy, Debug, ValueEnum)]
enum SetupProfile {
    Mi6,
}

#[derive(Debug, Parser)]
#[command(name = "camera-hub setup", about = "Initialize camera-hub user state")]
struct Args {
    #[arg(value_enum)]
    profile: SetupProfile,

    #[arg(long, default_value = "/home/android")]
    home: PathBuf,

    #[arg(long, default_value = "mi6.gwghome.site")]
    public_domain: String,

    #[arg(long, default_value = "wlan0")]
    public_interface: String,

    #[arg(long, default_value = "/dev/peel_ir")]
    ir_device: PathBuf,
}

pub fn run(arguments: Vec<OsString>) -> Result<()> {
    let args = Args::parse_from(arguments);
    match args.profile {
        SetupProfile::Mi6 => setup_mi6(&args)?,
    }
    println!("{}", args.home.join(".config/camera-hub.env").display());
    Ok(())
}

fn setup_mi6(args: &Args) -> Result<()> {
    if !args.home.is_absolute() {
        bail!("setup home must be an absolute path");
    }
    let path = |suffix: &str| args.home.join(suffix).to_string_lossy().into_owned();
    for suffix in [
        ".config",
        ".config/camera-hub-acme-webroot/.well-known/acme-challenge",
        ".ssh",
        "camera-data",
        "camera-data/voice",
        "camera-data/voice/tts",
        "camera-voice",
        "camera-voice/models",
    ] {
        fs::create_dir_all(args.home.join(suffix))
            .with_context(|| format!("create setup directory {suffix}"))?;
    }
    for suffix in [".config", ".ssh", "camera-data/voice/tts"] {
        secure_directory(&args.home.join(suffix))?;
    }

    let env_path = args.home.join(".config/camera-hub.env");
    let tts_token = match existing_value(&env_path, "CAMERA_HUB_TTS_TOKEN") {
        Some(value) => value,
        None => random_token()?,
    };
    let ai_runtime = path("camera-ai/runtime/lib/libonnxruntime.so");
    let ai_model = path("camera-ai/models/yolox_nano.onnx");
    let defaults = vec![
        ("CAMERA_HUB_WEB_USERNAME", "admin".to_owned()),
        ("CAMERA_HUB_WEB_PASSWORD", "12345".to_owned()),
        ("CAMERA_HUB_BIND", "[::]:80".to_owned()),
        ("CAMERA_HUB_TLS_BIND", "[::]:443".to_owned()),
        ("CAMERA_HUB_TLS_CERT", path(".config/camera-hub-cert.pem")),
        ("CAMERA_HUB_TLS_KEY", path(".config/camera-hub-key.pem")),
        ("CAMERA_HUB_MOQ_ENABLED", "true".to_owned()),
        ("CAMERA_HUB_MOQ_BIND", "[::]:443".to_owned()),
        (
            "CAMERA_HUB_ACME_WEBROOT",
            path(".config/camera-hub-acme-webroot"),
        ),
        ("CAMERA_HUB_PUBLIC_INTERFACE", args.public_interface.clone()),
        ("CAMERA_HUB_PUBLIC_DOMAIN", args.public_domain.clone()),
        ("CAMERA_HUB_EDGE_ACME_ENABLED", "false".to_owned()),
        ("CAMERA_HUB_EDGE_DEVICE_ID", "v831cam".to_owned()),
        ("CAMERA_HUB_EDGE_DOMAIN", "v831.gwghome.site".to_owned()),
        ("CAMERA_HUB_EDGE_SSH_USER", "root".to_owned()),
        (
            "CAMERA_HUB_EDGE_SSH_KEY",
            path(".ssh/camera-hub-edge-acme-rsa"),
        ),
        ("CAMERA_HUB_EDGE_RUNTIME_DIR", "/root/maix_dist".to_owned()),
        ("CAMERA_HUB_DATA_DIR", path("camera-data")),
        ("CAMERA_HUB_SETTINGS_FILE", path(".config/camera-hub.json")),
        (
            "CAMERA_HUB_QQ_CONFIG_FILE",
            path(".config/camera-hub-qq.json"),
        ),
        (
            "CAMERA_HUB_DDNS_CONFIG_FILE",
            path(".config/camera-hub-ddns.json"),
        ),
        (
            "CAMERA_HUB_DDNS_STATUS_FILE",
            path(".config/camera-hub-ddns-status.json"),
        ),
        (
            "CAMERA_HUB_DDNS_STATE_FILE",
            path(".config/camera-hub-ddns.state"),
        ),
        ("CAMERA_HUB_COMPONENT_MANAGER_ENABLED", "true".to_owned()),
        (
            "CAMERA_HUB_COMPONENTS_FILE",
            path(".config/camera-hub-components.json"),
        ),
        (
            "CAMERA_HUB_LOG_DIR",
            args.home.to_string_lossy().into_owned(),
        ),
        ("CAMERA_HUB_IR_BIND", "127.0.0.1:39182".to_owned()),
        ("CAMERA_HUB_IR_URL", "http://127.0.0.1:39182".to_owned()),
        (
            "CAMERA_HUB_IR_DEVICE",
            args.ir_device.to_string_lossy().into_owned(),
        ),
        ("CAMERA_HUB_ASSET_CACHE_DIR", path("camera-voice")),
        (
            "CAMERA_HUB_VOICE_CONFIG_FILE",
            path(".config/camera-hub-voice.json"),
        ),
        (
            "CAMERA_HUB_VOICE_STATUS_FILE",
            path(".config/camera-hub-voice-status.json"),
        ),
        (
            "CAMERA_HUB_VOICE_EVENTS_FILE",
            path("camera-data/voice/events.jsonl"),
        ),
        (
            "CAMERA_HUB_VOICE_COMMAND_FILE",
            path(".config/camera-hub-voice-command.json"),
        ),
        (
            "CAMERA_HUB_VOICE_TRANSCRIBE_FILE",
            path(".config/camera-hub-voice-transcribe.json"),
        ),
        (
            "CAMERA_HUB_VOICE_LIB_DIR",
            "/usr/local/lib/camera-hub-voice".to_owned(),
        ),
        (
            "CAMERA_HUB_VOICE_MODEL_DIR",
            path("camera-voice/models/sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01"),
        ),
        (
            "CAMERA_HUB_ASR_MODEL_DIR",
            path("camera-voice/models/sherpa-onnx-streaming-zipformer-zh-14M-2023-02-23"),
        ),
        ("CAMERA_HUB_TTS_BIND", "127.0.0.1:39081".to_owned()),
        ("CAMERA_HUB_TTS_URL", "http://127.0.0.1:39081".to_owned()),
        ("CAMERA_HUB_TTS_TOKEN", tts_token),
        (
            "CAMERA_HUB_TTS_MODEL_DIR",
            path("camera-voice/models/sherpa-onnx-zipvoice-distill-int8-zh-en-emilia"),
        ),
        (
            "CAMERA_HUB_TTS_VOCODER",
            path("camera-voice/models/vocos_24khz.onnx"),
        ),
        ("CAMERA_HUB_TTS_DATA_DIR", path("camera-data/voice/tts")),
        ("CAMERA_HUB_TTS_THREADS", "2".to_owned()),
        ("CAMERA_HUB_TTS_MAX_DATA_BYTES", "536870912".to_owned()),
        ("CAMERA_HUB_SEGMENT_SECONDS", "600".to_owned()),
        ("CAMERA_HUB_MAX_BYTES", "8589934592".to_owned()),
        ("CAMERA_HUB_RETAIN_DAYS", "7".to_owned()),
        (
            "CAMERA_HUB_AI_ENABLED",
            (Path::new(&ai_runtime).is_file() && Path::new(&ai_model).is_file()).to_string(),
        ),
        ("CAMERA_HUB_AI_RUNTIME", ai_runtime),
        ("CAMERA_HUB_AI_MODEL", ai_model),
        ("CAMERA_HUB_AI_INTERVAL_MS", "1000".to_owned()),
        ("CAMERA_HUB_AI_THRESHOLD", "0.30".to_owned()),
        ("CAMERA_HUB_AI_MIN_PERSON_AREA_RATIO", "0.02".to_owned()),
        ("CAMERA_HUB_AI_MIN_SNAPSHOT_SECONDS", "10".to_owned()),
        ("CAMERA_HUB_AI_SNAPSHOT_MAX_COUNT", "500".to_owned()),
        ("CAMERA_HUB_AI_SNAPSHOT_QUALITY", "95".to_owned()),
    ];
    merge_env_file(&env_path, &defaults)?;
    DdnsConfigStore::load(args.home.join(".config/camera-hub-ddns.json"))?;
    Ok(())
}

fn merge_env_file(path: &Path, defaults: &[(&str, String)]) -> Result<()> {
    let existing = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let obsolete = OBSOLETE_KEYS.iter().copied().collect::<HashSet<_>>();
    let mut lines = existing
        .lines()
        .filter(|line| env_key(line).is_none_or(|key| !obsolete.contains(key)))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut keys = lines
        .iter()
        .filter_map(|line| env_key(line))
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    for (key, value) in defaults {
        if keys.insert((*key).to_owned()) {
            lines.push(format!("{key}={}", shell_quote(value)));
        }
    }
    let mut output = lines.join("\n");
    output.push('\n');
    write_secure(path, output.as_bytes())
}

fn env_key(line: &str) -> Option<&str> {
    let line = line.trim_start();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (key, _) = line.split_once('=')?;
    (!key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'))
    .then_some(key)
}

fn existing_value(path: &Path, key: &str) -> Option<String> {
    let data = fs::read_to_string(path).ok()?;
    data.lines().find_map(|line| {
        let (candidate, value) = line.trim_start().split_once('=')?;
        (candidate == key).then(|| unquote(value.trim()))
    })
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn unquote(value: &str) -> String {
    value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
        .unwrap_or(value)
        .replace("'\\''", "'")
}

fn random_token() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).context("generate TTS token")?;
    Ok(hex::encode(bytes))
}

fn write_secure(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("env.tmp");
    fs::write(&temporary, data)?;
    secure_file(&temporary)?;
    fs::rename(&temporary, path)?;
    secure_file(path)
}

fn secure_file(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn secure_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_home() -> PathBuf {
        std::env::temp_dir().join(format!(
            "camera-hub-setup-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn creates_and_updates_mi6_environment_without_overwriting_values() {
        let home = temporary_home();
        let config_dir = home.join(".config");
        fs::create_dir_all(&config_dir).unwrap();
        let env_path = config_dir.join("camera-hub.env");
        fs::write(
            &env_path,
            "CAMERA_HUB_WEB_PASSWORD='custom'\n\
             CAMERA_HUB_VOICE_STUDIO_TOKEN='obsolete'\n",
        )
        .unwrap();
        let args = Args {
            profile: SetupProfile::Mi6,
            home: home.clone(),
            public_domain: "camera.example".to_owned(),
            public_interface: "wlan0".to_owned(),
            ir_device: PathBuf::from("/dev/example-ir"),
        };

        setup_mi6(&args).unwrap();
        let first = fs::read_to_string(&env_path).unwrap();
        setup_mi6(&args).unwrap();
        let second = fs::read_to_string(&env_path).unwrap();

        assert_eq!(first, second);
        assert!(first.contains("CAMERA_HUB_WEB_PASSWORD='custom'"));
        assert!(first.contains("CAMERA_HUB_PUBLIC_DOMAIN='camera.example'"));
        assert!(first.contains("CAMERA_HUB_IR_DEVICE='/dev/example-ir'"));
        assert!(!first.contains("CAMERA_HUB_VOICE_STUDIO_TOKEN"));
        let token = existing_value(&env_path, "CAMERA_HUB_TTS_TOKEN").unwrap();
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(config_dir.join("camera-hub-ddns.json").is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&env_path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn quotes_shell_values() {
        assert_eq!(shell_quote("camera hub"), "'camera hub'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(unquote("'it'\\''s'"), "it's");
    }
}
