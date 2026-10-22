use anyhow::{Context, Result, bail};
use russh::keys::{
    Algorithm, HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate, load_secret_key,
};
use russh::{ChannelMsg, Disconnect, Preferred, client};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Deserialize, Serialize)]
pub struct EdgeCertificateConfig {
    pub domain: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub user: String,
    pub key_path: PathBuf,
    pub host_key_sha256: String,
    pub runtime_dir: String,
}

fn default_port() -> u16 {
    22
}

struct PinnedHost(String);

impl client::Handler for PinnedHost {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        Ok(key.public_key().fingerprint(HashAlg::Sha256).to_string() == self.0)
    }
}

pub fn validate(config: &EdgeCertificateConfig) -> Result<()> {
    if config.host.is_empty()
        || config.user.is_empty()
        || !config.host_key_sha256.starts_with("SHA256:")
        || !config.runtime_dir.starts_with('/')
        || config.runtime_dir.contains('\0')
        || config.port == 0
    {
        bail!("V831 SSH 配置不完整（需要主机、用户、固定主机指纹和绝对运行目录）");
    }
    Ok(())
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

async fn exec(
    session: &client::Handle<PinnedHost>,
    command: &str,
    input: &[u8],
) -> Result<Vec<u8>> {
    let mut channel = session.channel_open_session().await?;
    channel.exec(true, command).await?;
    if !input.is_empty() {
        channel.data(input).await?;
    }
    channel.eof().await?;
    let mut output = Vec::new();
    let mut code = None;
    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { data } => {
                if output.len() + data.len() > 1024 * 1024 {
                    bail!("V831 SSH 响应过大");
                }
                output.extend_from_slice(&data);
            }
            ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status),
            _ => {}
        }
    }
    if code != Some(0) {
        bail!("V831 SSH 命令失败，退出码 {code:?}");
    }
    Ok(output)
}

/// Both staged files are sent over the encrypted channel, never in exec arguments.
/// A failed reload is retried even if the files already contain the new certificate.
pub async fn synchronize(config: &EdgeCertificateConfig, chain: &str, key: &str) -> Result<bool> {
    tokio::time::timeout(Duration::from_secs(90), async {
        validate(config)?;
        let mut preferred = Preferred::default();
        let mut kex = preferred.kex.to_vec();
        kex.extend([russh::kex::DH_G14_SHA1, russh::kex::DH_G1_SHA1]);
        preferred.kex = Cow::Owned(kex);
        let mut algorithms = preferred.key.to_vec();
        algorithms.push(Algorithm::Rsa { hash: None });
        preferred.key = Cow::Owned(algorithms);
        let mut mac = preferred.mac.to_vec();
        mac.push(russh::mac::HMAC_SHA1);
        preferred.mac = Cow::Owned(mac);
        let mut session = client::connect(
            Arc::new(client::Config {
                preferred,
                inactivity_timeout: Some(Duration::from_secs(30)),
                ..Default::default()
            }),
            (config.host.as_str(), config.port),
            PinnedHost(config.host_key_sha256.clone()),
        )
        .await
        .context("连接 V831 SSH（固定主机指纹）")?;
        let hash = session.best_supported_rsa_hash().await?.flatten();
        let authentication = session
            .authenticate_publickey(
                &config.user,
                PrivateKeyWithHashAlg::new(
                    Arc::new(load_secret_key(&config.key_path, None)?),
                    hash,
                ),
            )
            .await?;
        if !authentication.success() {
            bail!("V831 SSH 公钥认证失败");
        }
        let directory = quote(&format!("{}/state/tls", config.runtime_dir));
        let old = exec(
            &session,
            &format!("cat {directory}/fullchain.pem 2>/dev/null || true"),
            &[],
        )
        .await?;
        let remote_key_hash = exec(
            &session,
            &format!("sha256sum {directory}/private.key 2>/dev/null || true"),
            &[],
        )
        .await?;
        use sha2::{Digest, Sha256};
        let key_hash = hex::encode(Sha256::digest(key.as_bytes()));
        let pending = exec(
            &session,
            &format!("test ! -e {directory}/reload-pending"),
            &[],
        )
        .await
        .is_err();
        let changed = old != chain.as_bytes()
            || !String::from_utf8_lossy(&remote_key_hash).starts_with(&key_hash);
        if changed {
            for (name, data) in [("fullchain.pem", chain), ("private.key", key)] {
                exec(
                    &session,
                    &format!(
                        "set -e; umask 077; mkdir -p {directory}; cat > {directory}/{name}.new"
                    ),
                    data.as_bytes(),
                )
                .await?;
            }
            exec(
                &session,
                &format!(
                    "set -e; touch {directory}/reload-pending; \
                     mv {directory}/private.key.new {directory}/private.key; \
                     mv {directory}/fullchain.pem.new {directory}/fullchain.pem"
                ),
                &[],
            )
            .await?;
        }
        if changed || pending {
            let runtime = quote(&config.runtime_dir);
            exec(
                &session,
                &format!(
                    "set -e; cd {runtime}; variant=$(cat active_variant); \
                     case \"$variant\" in rust|cpp) ./switch_camera.sh \"$variant\" ;; \
                     *) exit 1 ;; esac; rm -f {directory}/reload-pending"
                ),
                &[],
            )
            .await?;
        }
        session
            .disconnect(Disconnect::ByApplication, "", "English")
            .await?;
        Ok(changed || pending)
    })
    .await
    .context("V831 证书同步超时")?
}
