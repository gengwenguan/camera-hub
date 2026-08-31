use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;

const LOCK_PATH: &str = "/tmp/camera-hub-inference.lock";

pub struct InferenceLock {
    file: File,
}

pub struct InferenceGuard<'a> {
    file: &'a File,
}

impl InferenceLock {
    pub fn open() -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(LOCK_PATH)
            .with_context(|| format!("open inference lock {LOCK_PATH}"))?;
        Ok(Self { file })
    }

    pub fn lock(&self) -> Result<InferenceGuard<'_>> {
        loop {
            if unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_EX) } == 0 {
                return Ok(InferenceGuard { file: &self.file });
            }
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                return Err(error).context("lock inference worker");
            }
        }
    }
}

impl Drop for InferenceGuard<'_> {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
