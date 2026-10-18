//! Small process/platform adapters; media and voice logic stay in their workers.
pub fn ffmpeg_command() -> std::process::Command {
    #[allow(unused_mut)]
    let mut command = std::process::Command::new(
        std::env::var_os("CAMERA_HUB_FFMPEG").unwrap_or_else(|| "ffmpeg".into()),
    );
    #[cfg(feature = "android-app")]
    {
        use std::os::unix::process::CommandExt;
        let parent = std::process::id() as libc::pid_t;
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    return Err(std::io::Error::other("Android worker exited"));
                }
                Ok(())
            });
        }
    }
    command
}

#[cfg(feature = "android-app")]
pub mod android {
    use anyhow::{Context, Result, bail};
    use serde_json::{Value, json};
    use std::io::BufReader;
    use std::os::unix::net::UnixStream;
    use std::path::Path;

    pub fn initialize(server: bool) -> Result<()> {
        // All descendants die with the App; the main server owns a process group
        // so the service can also stop a wedged worker/FFmpeg process.
        let parent = unsafe { libc::getppid() };
        if unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) } == -1 {
            return Err(std::io::Error::last_os_error()).context("set Android parent death signal");
        }
        if parent == 1 || unsafe { libc::getppid() } != parent {
            bail!("Android host exited during startup");
        }
        if server {
            if unsafe { libc::setpgid(0, 0) } == -1 {
                return Err(std::io::Error::last_os_error()).context("create engine process group");
            }
            if let Some(path) = std::env::var_os("CAMERA_HUB_PID_FILE") {
                std::fs::write(path, std::process::id().to_string())?;
            }
        }
        Ok(())
    }

    pub fn request(message: Value) -> Result<BufReader<UnixStream>> {
        camera_hub::android_bridge::exchange(message).map(|(_, reader)| reader)
    }

    pub fn run_capture(arguments: &[OsString]) -> Result<()> {
        let rate = arguments
            .get(2)
            .and_then(|v| v.to_str())
            .context("capture sample rate")?
            .parse::<u32>()?;
        let mut reader = request(json!({"op": "capture", "rate": rate}))?;
        std::io::copy(&mut reader, &mut std::io::stdout().lock())?;
        Ok(())
    }

    use std::ffi::OsString;

    pub async fn play(path: &Path) -> Result<()> {
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || {
            request(json!({"op": "play", "path": path})).map(|_| ())
        })
        .await?
    }

    pub async fn speak(text: &str, volume: u8) -> Result<()> {
        let text = text.to_owned();
        tokio::task::spawn_blocking(move || {
            request(json!({"op": "speak", "text": text, "volume": volume})).map(|_| ())
        })
        .await?
    }

    pub struct IrTransmitter;

    impl IrTransmitter {
        pub fn open(_device: std::path::PathBuf) -> Result<Self> {
            request(json!({"op": "ir_status"}))?;
            Ok(Self)
        }

        pub fn transmit(&self, action: crate::ir::IrAction) -> Result<crate::ir::IrTransmission> {
            let definition = action.definition();
            self.send(
                crate::ir::action_pattern(action)?,
                definition.id,
                definition.label.to_owned(),
            )
        }

        pub fn transmit_aircon(
            &self,
            command: &crate::ir::AirconCommand,
        ) -> Result<crate::ir::IrTransmission> {
            self.send(
                crate::ir::aircon_pattern(command)?,
                "aircon-state",
                command.label(),
            )
        }

        fn send(
            &self,
            pattern: crate::ir::PulsePattern,
            action: &str,
            label: String,
        ) -> Result<crate::ir::IrTransmission> {
            request(
                json!({"op": "ir", "carrier_hz": pattern.carrier_hz, "durations_us": pattern.durations_us}),
            )?;
            Ok(crate::ir::IrTransmission {
                action: action.to_owned(),
                label,
                carrier_hz: pattern.carrier_hz,
                pulse_count: pattern.durations_us.len(),
                duration_us: pattern.durations_us.iter().map(|v| u64::from(*v)).sum(),
                spi_bytes: 0,
            })
        }
    }
}
