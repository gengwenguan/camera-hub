use anyhow::{Context, Result};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

/// Write a private JSON document using a unique sibling and an atomic rename.
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let data = serde_json::to_vec_pretty(value)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent)?;
    }
    let name = path.file_name().context("JSON path has no file name")?;
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).context("generate temporary JSON name")?;
    let mut temporary_name = name.to_os_string();
    temporary_name.push(format!(".{}.tmp", hex::encode(nonce)));
    let temporary = path.with_file_name(temporary_name);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .with_context(|| format!("create temporary JSON {}", temporary.display()))?;
    let result = (|| {
        file.write_all(&data)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path).with_context(|| format!("replace JSON {}", path.display()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
