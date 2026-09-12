use anyhow::Result;
use camera_hub::ddns::{
    API_ENDPOINT, DdnsConfig, DdnsLegacyConfig, DdnsStatus, DdnsSyncResult, epoch_seconds,
    load_config, preview, reconcile_with_endpoint, save_config, save_status,
};
use chrono::Utc;
use clap::Parser;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(
    name = "camera-hub-ddns",
    about = "Synchronize one IPv6 /64 prefix to multiple DNSPod AAAA records"
)]
struct Args {
    #[arg(
        long,
        env = "CAMERA_HUB_DDNS_CONFIG_FILE",
        default_value = "/home/android/.config/camera-hub-ddns.json"
    )]
    config_file: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_DDNS_STATUS_FILE",
        default_value = "/home/android/.config/camera-hub-ddns-status.json"
    )]
    status_file: PathBuf,

    #[arg(
        long,
        env = "CAMERA_HUB_DDNS_STATE_FILE",
        default_value = "/home/android/.config/camera-hub-ddns.state"
    )]
    state_file: PathBuf,

    #[arg(long, env = "CAMERA_HUB_DDNS_ENABLED", default_value_t = false)]
    enabled: bool,

    #[arg(long, env = "CAMERA_HUB_DDNS_DOMAIN", default_value = "gwghome.site")]
    domain: String,

    #[arg(long, env = "CAMERA_HUB_DDNS_SECRET_ID", default_value = "")]
    secret_id: String,

    #[arg(long, env = "CAMERA_HUB_DDNS_SECRET_KEY", default_value = "")]
    secret_key: String,

    #[arg(long, env = "CAMERA_HUB_DDNS_INTERFACE", default_value = "wlan0")]
    interface: String,

    #[arg(
        long,
        env = "CAMERA_HUB_DDNS_RECORDS",
        default_value = "@=528f:4cff:feef:dd90,mi6=528f:4cff:feef:dd90,v831=a22c:36ff:febd:4feb,lecoo=8647:09ff:fe45:35a0,lecoo-wifi=72c9:12ff:fe1c:2f67,huawei=1a56:80ff:fe82:816a"
    )]
    records: String,

    #[arg(long, env = "CAMERA_HUB_DDNS_TTL", default_value_t = 600)]
    ttl: u64,

    #[arg(long, env = "CAMERA_HUB_DDNS_INTERVAL_SECONDS", default_value_t = 60)]
    interval_seconds: u64,

    #[arg(long, env = "CAMERA_HUB_DDNS_FORCE_SECONDS", default_value_t = 21_600)]
    force_seconds: u64,

    #[arg(long, default_value_t = false)]
    dry_run: bool,

    #[arg(long, default_value_t = false)]
    once: bool,

    #[arg(long, default_value_t = false)]
    write_config: bool,

    #[arg(long, hide = true, default_value = API_ENDPOINT)]
    endpoint: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args = Args::parse();

    if args.write_config {
        let config = legacy_config(&args)?;
        save_config(&args.config_file, &config)?;
        println!("{}", args.config_file.display());
        return Ok(());
    }

    if !args.config_file.is_file() {
        save_config(&args.config_file, &legacy_config(&args)?)?;
    }
    let config = load_config(&args.config_file)?;
    if args.dry_run {
        print_preview(&config)?;
        return Ok(());
    }
    if args.once {
        let result =
            reconcile_with_endpoint(&config, &args.state_file, true, &args.endpoint).await?;
        print_result(&result);
        return Ok(());
    }

    run_daemon(&args).await
}

async fn run_daemon(args: &Args) -> Result<()> {
    let mut status = DdnsStatus::default();
    let mut observed_revision = None;
    let mut force = false;
    let mut next_attempt = Instant::now();
    let mut retry_delay = Duration::from_secs(60);
    let mut last_publish = Instant::now()
        .checked_sub(Duration::from_secs(60))
        .unwrap_or_else(Instant::now);

    loop {
        let config = match load_config(&args.config_file) {
            Ok(config) => config,
            Err(error) => {
                let message = format!("{error:#}");
                let changed = status.state != "config_error" || status.last_error != message;
                status.state = "config_error".to_owned();
                status.detail = "DDNS 配置文件无效".to_owned();
                status.last_error = message.clone();
                status.next_attempt_epoch = epoch_seconds().saturating_add(2);
                if changed || last_publish.elapsed() >= Duration::from_secs(30) {
                    log_line(&format!("DDNS configuration failed: {message}"));
                    publish_status(&mut status, &args.status_file);
                    last_publish = Instant::now();
                }
                if wait_for_tick().await {
                    publish_stopped(&mut status, &args.status_file);
                    return Ok(());
                }
                continue;
            }
        };

        let config_changed = observed_revision != Some(config.revision);
        if config_changed {
            observed_revision = Some(config.revision);
            status.config_revision = config.revision;
            force = true;
            next_attempt = Instant::now();
            retry_delay = Duration::from_secs(config.interval_seconds);
        }

        if !config.enabled {
            let changed = status.state != "disabled" || config_changed;
            status.state = "disabled".to_owned();
            status.detail = "DDNS 已关闭".to_owned();
            status.next_attempt_epoch = 0;
            status.consecutive_failures = 0;
            status.last_error.clear();
            if changed || last_publish.elapsed() >= Duration::from_secs(30) {
                publish_status(&mut status, &args.status_file);
                last_publish = Instant::now();
            }
            if wait_for_tick().await {
                publish_stopped(&mut status, &args.status_file);
                return Ok(());
            }
            continue;
        }

        if let Some(error) = config.readiness_error() {
            let changed = status.state != "incomplete" || status.last_error != error;
            status.state = "incomplete".to_owned();
            status.detail = "DDNS 配置不完整".to_owned();
            status.last_error = error;
            status.next_attempt_epoch = 0;
            if changed || last_publish.elapsed() >= Duration::from_secs(30) {
                publish_status(&mut status, &args.status_file);
                last_publish = Instant::now();
            }
            if wait_for_tick().await {
                publish_stopped(&mut status, &args.status_file);
                return Ok(());
            }
            continue;
        }

        if Instant::now() >= next_attempt {
            status.state = "synchronizing".to_owned();
            status.detail = "正在对账 DNSPod AAAA 记录".to_owned();
            status.last_attempt_epoch = epoch_seconds();
            status.next_attempt_epoch = 0;
            publish_status(&mut status, &args.status_file);

            match reconcile_with_endpoint(&config, &args.state_file, force, &args.endpoint).await {
                Ok(result) => {
                    apply_success(&mut status, &result);
                    retry_delay = Duration::from_secs(config.interval_seconds);
                    next_attempt = Instant::now() + retry_delay;
                    status.next_attempt_epoch =
                        epoch_seconds().saturating_add(retry_delay.as_secs());
                    log_line(&status.detail);
                }
                Err(error) => {
                    let message = format!("{error:#}");
                    retry_delay = (retry_delay * 2).min(Duration::from_secs(900));
                    next_attempt = Instant::now() + retry_delay;
                    status.state = "retrying".to_owned();
                    status.detail = format!("同步失败，{} 秒后重试", retry_delay.as_secs());
                    status.next_attempt_epoch =
                        epoch_seconds().saturating_add(retry_delay.as_secs());
                    status.consecutive_failures = status.consecutive_failures.saturating_add(1);
                    status.last_error = message.clone();
                    log_line(&format!("DNSPod synchronization failed: {message}"));
                }
            }
            force = false;
            publish_status(&mut status, &args.status_file);
            last_publish = Instant::now();
        } else if last_publish.elapsed() >= Duration::from_secs(30) {
            publish_status(&mut status, &args.status_file);
            last_publish = Instant::now();
        }

        if wait_for_tick().await {
            publish_stopped(&mut status, &args.status_file);
            return Ok(());
        }
    }
}

fn apply_success(status: &mut DdnsStatus, result: &DdnsSyncResult) {
    status.state = "online".to_owned();
    status.detail = if result.verified {
        format!("DNSPod 对账完成，更新 {} 条记录", result.changed)
    } else {
        "公网前缀未变化，等待下次检查".to_owned()
    };
    status.stable_ipv6.clone_from(&result.stable_ipv6);
    status.prefix.clone_from(&result.prefix);
    status.last_success_epoch = result.last_verified_epoch;
    status.changed_count = result.changed as u64;
    status.consecutive_failures = 0;
    status.last_error.clear();
}

async fn wait_for_tick() -> bool {
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(1)) => false,
        _ = tokio::signal::ctrl_c() => true,
    }
}

fn publish_status(status: &mut DdnsStatus, path: &Path) {
    status.pid = std::process::id();
    status.updated_epoch = epoch_seconds();
    if let Err(error) = save_status(path, status) {
        log_line(&format!("write DDNS status failed: {error:#}"));
    }
}

fn publish_stopped(status: &mut DdnsStatus, path: &Path) {
    status.state = "stopped".to_owned();
    status.detail = "DDNS 进程已停止".to_owned();
    status.next_attempt_epoch = 0;
    publish_status(status, path);
}

fn legacy_config(args: &Args) -> Result<DdnsConfig> {
    DdnsConfig::from_legacy(DdnsLegacyConfig {
        enabled: args.enabled,
        domain: args.domain.clone(),
        secret_id: args.secret_id.clone(),
        secret_key: args.secret_key.clone(),
        interface: args.interface.clone(),
        records: args.records.clone(),
        ttl: args.ttl,
        interval_seconds: args.interval_seconds,
        force_seconds: args.force_seconds,
    })
}

fn print_preview(config: &DdnsConfig) -> Result<()> {
    let preview = preview(config)?;
    println!("stable_ipv6={}", preview.stable_ipv6);
    println!("prefix={}", preview.prefix);
    for record in preview.records {
        println!("{} AAAA {}", record.fqdn, record.address);
    }
    Ok(())
}

fn print_result(result: &DdnsSyncResult) {
    println!(
        "stable_ipv6={} prefix={} verified={} changed={}",
        result.stable_ipv6, result.prefix, result.verified, result.changed
    );
}

fn log_line(message: &str) {
    eprintln!("{} {message}", Utc::now().format("%Y-%m-%dT%H:%M:%SZ"));
}
