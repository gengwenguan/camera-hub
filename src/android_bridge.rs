//! Private Unix socket transport shared by the Android hardware and network adapters.
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

pub fn exchange(message: Value) -> Result<(Value, BufReader<UnixStream>)> {
    let path = std::env::var_os("CAMERA_HUB_ANDROID_SOCKET")
        .context("Android hardware bridge is not configured")?;
    let mut socket = UnixStream::connect(path).context("connect Android hardware bridge")?;
    socket.set_read_timeout(Some(Duration::from_secs(60)))?;
    socket.set_write_timeout(Some(Duration::from_secs(10)))?;
    serde_json::to_writer(&mut socket, &message)?;
    socket.write_all(b"\n")?;
    let mut reader = BufReader::new(socket);
    let mut response = String::new();
    reader.read_line(&mut response)?;
    let response: Value = serde_json::from_str(&response).context("Android bridge response")?;
    if response["ok"] != true {
        bail!(
            "Android: {}",
            response["error"]
                .as_str()
                .unwrap_or("hardware operation failed")
        );
    }
    Ok((response, reader))
}
