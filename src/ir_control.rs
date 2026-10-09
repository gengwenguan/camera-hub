use crate::config::Config;
use crate::ir::{AirconCommand, IrAction, IrActionDefinition, IrTransmission, action_catalog};
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
        Self::from_url(&config.ir_url, config.ir_device.clone())
    }

    pub(crate) fn from_url(url: &str, device: PathBuf) -> Result<Self> {
        let parsed = reqwest::Url::parse(url)?;
        let loopback = parsed.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        });
        if parsed.scheme() != "http"
            || !loopback
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.path() != "/"
        {
            bail!("红外 worker URL 必须使用无路径、无凭据的本机回环 HTTP");
        }
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .build()?,
            base_url: parsed.as_str().trim_end_matches('/').to_owned(),
            device,
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
        Self::read_transmission(response).await
    }

    pub async fn send_aircon(&self, command: &AirconCommand) -> Result<IrTransmission> {
        command.validate()?;
        let response = self
            .client
            .post(format!("{}/v1/aircon/state", self.base_url))
            .json(command)
            .send()
            .await
            .context("调用参数化红外控制")?;
        Self::read_transmission(response).await
    }

    async fn read_transmission(response: reqwest::Response) -> Result<IrTransmission> {
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
