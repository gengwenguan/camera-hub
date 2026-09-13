use crate::config::Config;
use crate::ir::{IrAction, IrActionDefinition, IrTransmission, action_catalog};
use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone)]
pub struct IrControl {
    client: Client,
    base_url: String,
    device: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
pub struct IrOverview {
    pub available: bool,
    pub state: &'static str,
    pub detail: String,
    pub device: PathBuf,
    pub actions: Vec<IrActionDefinition>,
}

#[derive(Debug, Deserialize)]
struct WorkerResponse {
    ok: bool,
    #[serde(default)]
    error: String,
    transmission: Option<IrTransmission>,
}

impl IrControl {
    pub fn new(config: &Config) -> Result<Self> {
        let parsed = reqwest::Url::parse(&config.ir_url)?;
        let loopback = parsed.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        });
        if parsed.scheme() != "http" || !loopback {
            bail!("红外 worker URL 必须使用本机回环 HTTP");
        }
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(5))
                .build()?,
            base_url: config.ir_url.trim_end_matches('/').to_owned(),
            device: config.ir_device.clone(),
        })
    }

    pub async fn overview(&self) -> IrOverview {
        let available = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success());
        IrOverview {
            available,
            state: if available { "ready" } else { "offline" },
            detail: if available {
                "红外发射服务已就绪".to_owned()
            } else if !self.device.exists() {
                format!("红外设备不存在：{}", self.device.display())
            } else {
                "等待红外 worker 启动".to_owned()
            },
            device: self.device.clone(),
            actions: action_catalog(),
        }
    }

    pub async fn send(&self, action: &str) -> Result<IrTransmission> {
        let action = IrAction::parse(action)?;
        let response = self
            .client
            .post(format!("{}/v1/actions/{}", self.base_url, action.id()))
            .send()
            .await
            .context("调用红外 worker")?;
        let status = response.status();
        let body = response
            .json::<WorkerResponse>()
            .await
            .context("解析红外 worker 响应")?;
        if !status.is_success() || !body.ok {
            bail!(
                "{}",
                if body.error.is_empty() {
                    format!("红外 worker 返回 HTTP {status}")
                } else {
                    body.error
                }
            );
        }
        body.transmission
            .ok_or_else(|| anyhow::anyhow!("红外 worker 未返回发送结果"))
    }
}
