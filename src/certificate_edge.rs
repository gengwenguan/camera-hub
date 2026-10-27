use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

#[derive(Clone, Deserialize, Serialize)]
pub struct EdgeCertificateConfig {
    pub domain: String,
    #[serde(default = "default_device_id")]
    pub device_id: String,
}

fn default_device_id() -> String {
    "v831cam".to_owned()
}

pub fn validate(config: &EdgeCertificateConfig) -> Result<()> {
    if config.device_id.is_empty() || config.device_id.len() > 128 {
        bail!("V831 证书需要设备 ID");
    }
    Ok(())
}

struct Pending {
    hash: String,
    reply: oneshot::Sender<Result<bool>>,
}

struct Link {
    generation: u64,
    sender: mpsc::Sender<String>,
    supported: bool,
    loaded_hash: String,
    pending: Option<Pending>,
}

#[derive(Default)]
pub struct CertificateLinks {
    links: Mutex<HashMap<String, Link>>,
}

impl CertificateLinks {
    pub fn register(&self, device: &str, generation: u64) -> mpsc::Receiver<String> {
        let (sender, receiver) = mpsc::channel(1);
        self.links.lock().unwrap().insert(
            device.to_owned(),
            Link {
                generation,
                sender,
                supported: false,
                loaded_hash: String::new(),
                pending: None,
            },
        );
        receiver
    }

    pub fn disconnect(&self, device: &str, generation: u64) {
        let mut links = self.links.lock().unwrap();
        if links.get(device).is_some_and(|link| link.generation == generation) {
            links.remove(device);
        }
    }

    /// Returns true when a capability/version change needs a cached deployment check.
    pub fn control(&self, device: &str, generation: u64, value: &Value) -> bool {
        let mut links = self.links.lock().unwrap();
        let Some(link) = links.get_mut(device).filter(|link| link.generation == generation) else {
            return false;
        };
        match value["type"].as_str() {
            Some("hello") => {
                let supported = value["certificate"]["protocol"].as_u64() == Some(1);
                let hash = value["certificate"]["bundle_sha256"].as_str().unwrap_or_default();
                let changed = supported && (!link.supported || link.loaded_hash != hash);
                link.supported = supported;
                link.loaded_hash = hash.to_owned();
                changed
            }
            Some("certificate_ack") => {
                let hash = value["bundle_sha256"].as_str().unwrap_or_default();
                if !link.pending.as_ref().is_some_and(|pending| pending.hash == hash) {
                    return false;
                }
                let pending = link.pending.take().unwrap();
                let result = if value["ok"].as_bool() == Some(true) {
                    link.loaded_hash = hash.to_owned();
                    Ok(value["reloaded"].as_bool() == Some(true))
                } else {
                    Err(anyhow::anyhow!("V831 证书安装失败：{}",
                        value["error"].as_str().unwrap_or("设备拒绝安装")))
                };
                let _ = pending.reply.send(result);
                false
            }
            _ => false,
        }
    }

    pub async fn synchronize(
        &self,
        config: &EdgeCertificateConfig,
        chain: &str,
        key: &str,
    ) -> Result<bool> {
        if chain.len() + key.len() > 64 * 1024 {
            bail!("V831 证书包超过 64 KiB");
        }
        let mut digest = Sha256::new();
        digest.update(chain.as_bytes());
        digest.update([0]);
        digest.update(key.as_bytes());
        let hash = hex::encode(digest.finalize());
        let (sender, reply, generation) = {
            let mut links = self.links.lock().unwrap();
            let link = links.get_mut(&config.device_id).context("V831 设备连接离线")?;
            if !link.supported {
                bail!("V831 尚未声明证书热加载协议");
            }
            if link.loaded_hash == hash {
                return Ok(false);
            }
            let (sender, receiver) = oneshot::channel();
            link.pending = Some(Pending { hash: hash.clone(), reply: sender });
            (link.sender.clone(), receiver, link.generation)
        };
        let command = json!({
            "type": "certificate_update",
            "domain": config.domain,
            "bundle_sha256": hash,
            "chain": chain,
            "key": key,
        }).to_string();
        let result = tokio::time::timeout(Duration::from_secs(30), async {
            sender.send(command).await.context("V831 设备连接已断开")?;
            reply.await.context("V831 证书确认前连接断开或被替代")?
        }).await.context("等待 V831 证书安装确认超时");
        let mut links = self.links.lock().unwrap();
        if let Some(link) = links.get_mut(&config.device_id).filter(|link| link.generation == generation) {
            if link.pending.as_ref().is_some_and(|pending| pending.hash == hash) {
                link.pending = None;
            }
        }
        result?
    }
}
