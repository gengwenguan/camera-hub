use crate::config::Config;
use crate::ir::{AcMode, AirconCommand, IrTransmission};
use crate::ir_control::IrControl;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::sleep;
use tracing::{info, warn};

const AIRCON_CONFIG_VERSION: u32 = 2;
const MAX_DELAY_MINUTES: u32 = 30 * 24 * 60;
const MAX_AUTO_OFF_MINUTES: u32 = 7 * 24 * 60;
const MAX_SCHEDULES: usize = 128;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AirconCapability {
    PowerOn,
    PowerOff,
    Cool,
    Dry,
    Heat,
    Auto,
    TemperatureUp,
    TemperatureDown,
    EcoOn,
    EcoOff,
}

const REQUIRED_CAPABILITIES: [AirconCapability; 10] = [
    AirconCapability::PowerOn,
    AirconCapability::PowerOff,
    AirconCapability::Cool,
    AirconCapability::Dry,
    AirconCapability::Heat,
    AirconCapability::Auto,
    AirconCapability::TemperatureUp,
    AirconCapability::TemperatureDown,
    AirconCapability::EcoOn,
    AirconCapability::EcoOff,
];

fn verification_complete(capabilities: &BTreeSet<AirconCapability>) -> bool {
    REQUIRED_CAPABILITIES
        .iter()
        .all(|capability| capabilities.contains(capability))
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirconVerificationUpdate {
    pub capability: AirconCapability,
    pub verified: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirconIntegrationUpdate {
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AirconScheduleStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AirconState {
    pub power: bool,
    pub temperature_c: u8,
    pub mode: AcMode,
    pub eco: bool,
}

impl Default for AirconState {
    fn default() -> Self {
        Self {
            power: false,
            temperature_c: 26,
            mode: AcMode::Cool,
            eco: true,
        }
    }
}

impl AirconState {
    fn validate(&self) -> std::result::Result<(), AirconError> {
        if !(17..=30).contains(&self.temperature_c) {
            return Err(AirconError::Invalid(
                "空调温度仅支持 17–30°C 的整数".to_owned(),
            ));
        }
        if self.eco && self.mode != AcMode::Cool {
            return Err(AirconError::Invalid(
                "ECO 仅支持与制冷模式一起设置".to_owned(),
            ));
        }
        Ok(())
    }

    fn to_ir_command(&self) -> AirconCommand {
        if self.power {
            AirconCommand::Set {
                temperature_c: self.temperature_c,
                mode: self.mode,
                eco: self.eco,
            }
        } else {
            AirconCommand::Off {}
        }
    }

    fn message(&self) -> String {
        if !self.power {
            return "已关闭空调".to_owned();
        }
        format!(
            "已设置{} {}°C，ECO {}",
            self.mode.label(),
            self.temperature_c,
            if self.eco { "开启" } else { "关闭" }
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum AirconOperation {
    Set {
        temperature_c: u8,
        mode: AcMode,
        eco: bool,
    },
    Off {},
    Power {
        enabled: bool,
    },
    SetTemperature {
        temperature_c: u8,
    },
    AdjustTemperature {
        delta_c: i8,
    },
    SetMode {
        mode: AcMode,
    },
    SetEco {
        enabled: bool,
    },
    CancelSchedule {
        schedule_id: String,
    },
    CancelAll {},
}

impl AirconOperation {
    fn apply(&self, current: &AirconState) -> std::result::Result<AirconState, AirconError> {
        let mut next = current.clone();
        match self {
            Self::Set {
                temperature_c,
                mode,
                eco,
            } => {
                next.power = true;
                next.temperature_c = *temperature_c;
                next.mode = *mode;
                next.eco = *eco;
            }
            Self::Off {} => next.power = false,
            Self::Power { enabled } => next.power = *enabled,
            Self::SetTemperature { temperature_c } => {
                next.power = true;
                next.temperature_c = *temperature_c;
            }
            Self::AdjustTemperature { delta_c } => {
                if !(-10..=10).contains(delta_c) || *delta_c == 0 {
                    return Err(AirconError::Invalid(
                        "温度调整幅度必须是 -10 到 10 之间的非零整数".to_owned(),
                    ));
                }
                next.power = true;
                next.temperature_c =
                    u8::try_from(i16::from(next.temperature_c) + i16::from(*delta_c))
                        .map_err(|_| AirconError::Invalid("温度调整结果超出范围".to_owned()))?;
            }
            Self::SetMode { mode } => {
                next.power = true;
                next.mode = *mode;
                if *mode != AcMode::Cool {
                    next.eco = false;
                }
            }
            Self::SetEco { enabled } => {
                next.power = true;
                next.eco = *enabled;
            }
            Self::CancelSchedule { .. } | Self::CancelAll {} => {
                return Err(AirconError::Invalid("取消定时不是红外状态操作".to_owned()));
            }
        }
        next.validate()?;
        Ok(next)
    }

    fn is_cancellation(&self) -> bool {
        matches!(self, Self::CancelSchedule { .. } | Self::CancelAll {})
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AirconCommandRequest {
    pub command: AirconOperation,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub delay_minutes: Option<u32>,
    #[serde(default)]
    pub execute_at_epoch: Option<u64>,
    #[serde(default)]
    pub auto_off_minutes: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirconIntegrationQuery {
    pub token: String,
    pub operation: String,
    #[serde(default)]
    pub temperature_c: Option<u8>,
    #[serde(default)]
    pub mode: Option<AcMode>,
    #[serde(default)]
    pub eco: Option<bool>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub delta_c: Option<i8>,
    #[serde(default)]
    pub schedule_id: String,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub delay_minutes: Option<u32>,
    #[serde(default)]
    pub execute_at_epoch: Option<u64>,
    #[serde(default)]
    pub auto_off_minutes: Option<u32>,
}

impl AirconIntegrationQuery {
    pub fn into_request(self) -> std::result::Result<AirconCommandRequest, AirconError> {
        let command = match self.operation.as_str() {
            "set" => AirconOperation::Set {
                temperature_c: required(self.temperature_c, "temperature_c")?,
                mode: required(self.mode, "mode")?,
                eco: required(self.eco, "eco")?,
            },
            "off" => AirconOperation::Off {},
            "power" => AirconOperation::Power {
                enabled: required(self.enabled, "enabled")?,
            },
            "set_temperature" => AirconOperation::SetTemperature {
                temperature_c: required(self.temperature_c, "temperature_c")?,
            },
            "adjust_temperature" => AirconOperation::AdjustTemperature {
                delta_c: required(self.delta_c, "delta_c")?,
            },
            "set_mode" => AirconOperation::SetMode {
                mode: required(self.mode, "mode")?,
            },
            "set_eco" => AirconOperation::SetEco {
                enabled: required(self.enabled, "enabled")?,
            },
            "cancel_schedule" => {
                if self.schedule_id.trim().is_empty() {
                    return Err(AirconError::Invalid("缺少 schedule_id".to_owned()));
                }
                AirconOperation::CancelSchedule {
                    schedule_id: self.schedule_id,
                }
            }
            "cancel_all" => AirconOperation::CancelAll {},
            _ => return Err(AirconError::Invalid("不支持的空调操作".to_owned())),
        };
        Ok(AirconCommandRequest {
            command,
            request_id: self.request_id,
            delay_minutes: self.delay_minutes,
            execute_at_epoch: self.execute_at_epoch,
            auto_off_minutes: self.auto_off_minutes,
        })
    }
}

fn required<T>(value: Option<T>, field: &str) -> std::result::Result<T, AirconError> {
    value.ok_or_else(|| AirconError::Invalid(format!("缺少 {field}")))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AirconSchedule {
    pub id: String,
    pub command: AirconOperation,
    pub request_id: String,
    pub execute_at_epoch: u64,
    pub auto_off_minutes: Option<u32>,
    pub status: AirconScheduleStatus,
    pub created_epoch: u64,
    pub finished_epoch: u64,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AirconOverview {
    pub state: AirconState,
    pub state_known: bool,
    pub physical_confirmed: bool,
    pub schedules: Vec<AirconSchedule>,
    pub integration_token: String,
    pub integration_endpoint: &'static str,
    pub integration_enabled: bool,
    pub verification_complete: bool,
    pub verified_capabilities: Vec<AirconCapability>,
    pub required_capabilities: Vec<AirconCapability>,
    pub temperature_min: u8,
    pub temperature_max: u8,
    pub modes: [&'static str; 4],
    pub max_delay_minutes: u32,
    pub max_auto_off_minutes: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AirconResultStatus {
    Executed,
    Scheduled,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
pub struct AirconCommandResult {
    pub ok: bool,
    pub status: AirconResultStatus,
    pub message: String,
    pub state: AirconState,
    pub state_known: bool,
    pub physical_confirmed: bool,
    pub transmission: Option<IrTransmission>,
    pub schedule: Option<AirconSchedule>,
}

#[derive(Debug)]
pub enum AirconError {
    Invalid(String),
    NotFound(String),
    Delivery(String),
    Internal(String),
}

impl std::fmt::Display for AirconError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message)
            | Self::NotFound(message)
            | Self::Delivery(message)
            | Self::Internal(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for AirconError {}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
struct AirconDocument {
    version: u32,
    revision: u64,
    integration_token: String,
    integration_enabled: bool,
    verified_capabilities: BTreeSet<AirconCapability>,
    state: AirconState,
    state_known: bool,
    next_schedule_id: u64,
    schedules: Vec<AirconSchedule>,
}

impl Default for AirconDocument {
    fn default() -> Self {
        Self {
            version: AIRCON_CONFIG_VERSION,
            revision: 1,
            integration_token: String::new(),
            integration_enabled: false,
            verified_capabilities: BTreeSet::new(),
            state: AirconState::default(),
            state_known: false,
            next_schedule_id: 1,
            schedules: Vec::new(),
        }
    }
}

impl AirconDocument {
    fn normalize(mut self) -> Result<Self> {
        self.version = AIRCON_CONFIG_VERSION;
        if self.integration_token.is_empty() {
            self.integration_token = generate_token()?;
        }
        if self.integration_token.len() > 128
            || self.integration_token.chars().any(char::is_control)
        {
            anyhow::bail!("空调集成 Token 格式无效");
        }
        self.verified_capabilities
            .retain(|capability| REQUIRED_CAPABILITIES.contains(capability));
        if !verification_complete(&self.verified_capabilities) {
            self.integration_enabled = false;
        }
        if self.state.validate().is_err() {
            self.state = AirconState::default();
            self.state_known = false;
        }
        for schedule in &mut self.schedules {
            if schedule.status == AirconScheduleStatus::Running {
                schedule.status = AirconScheduleStatus::Failed;
                schedule.finished_epoch = epoch_seconds();
                schedule.message = "服务重启时执行结果未知，未自动重试".to_owned();
            }
        }
        self.schedules
            .sort_by_key(|schedule| schedule.execute_at_epoch);
        self.prune_schedules();
        Ok(self)
    }

    fn prune_schedules(&mut self) {
        while self.schedules.len() > MAX_SCHEDULES {
            if let Some(index) = self.schedules.iter().position(|schedule| {
                !matches!(
                    schedule.status,
                    AirconScheduleStatus::Pending | AirconScheduleStatus::Running
                )
            }) {
                self.schedules.remove(index);
            } else {
                break;
            }
        }
    }
}

struct AirconStore {
    path: PathBuf,
    current: AirconDocument,
}

impl AirconStore {
    fn load(path: PathBuf) -> Result<Self> {
        let current = match fs::read(&path) {
            Ok(data) => serde_json::from_slice::<AirconDocument>(&data)
                .with_context(|| format!("parse aircon state {}", path.display()))?
                .normalize()?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                AirconDocument::default().normalize()?
            }
            Err(error) => return Err(error.into()),
        };
        let store = Self { path, current };
        store.save()?;
        Ok(store)
    }

    fn save(&self) -> Result<()> {
        crate::atomic_file::write_json(&self.path, &self.current)
    }

    fn commit(&mut self, next: AirconDocument) -> Result<()> {
        crate::atomic_file::write_json(&self.path, &next)?;
        self.current = next;
        Ok(())
    }

    fn add_schedule(
        &mut self,
        command: AirconOperation,
        request_id: String,
        execute_at_epoch: u64,
        auto_off_minutes: Option<u32>,
    ) -> std::result::Result<AirconSchedule, AirconError> {
        if !request_id.is_empty()
            && let Some(existing) = self
                .current
                .schedules
                .iter()
                .find(|schedule| schedule.request_id == request_id)
        {
            if existing.command != command || existing.auto_off_minutes != auto_off_minutes {
                return Err(AirconError::Invalid(
                    "request_id 已用于不同的定时请求".to_owned(),
                ));
            }
            return Ok(existing.clone());
        }
        let mut next = self.current.clone();
        let schedule = insert_schedule(
            &mut next,
            command,
            request_id,
            execute_at_epoch,
            auto_off_minutes,
        )?;
        self.commit(next).map_err(internal_error)?;
        Ok(schedule)
    }
}

fn insert_schedule(
    document: &mut AirconDocument,
    command: AirconOperation,
    request_id: String,
    execute_at_epoch: u64,
    auto_off_minutes: Option<u32>,
) -> std::result::Result<AirconSchedule, AirconError> {
    let active_count: usize = document
        .schedules
        .iter()
        .filter(|schedule| {
            matches!(
                schedule.status,
                AirconScheduleStatus::Pending | AirconScheduleStatus::Running
            )
        })
        // Running parents transfer their child reservation while holding the
        // execution lock; the concrete off task then owns that capacity.
        .map(|schedule| {
            1 + usize::from(
                schedule.status == AirconScheduleStatus::Pending
                    && schedule.auto_off_minutes.is_some(),
            )
        })
        .sum();
    let required = 1 + usize::from(auto_off_minutes.is_some());
    if active_count + required > MAX_SCHEDULES {
        return Err(AirconError::Invalid(format!(
            "最多允许 {MAX_SCHEDULES} 个待执行空调任务（含自动关机），请先取消部分任务"
        )));
    }
    let schedule = AirconSchedule {
        id: format!("aircon-{:016x}", document.next_schedule_id),
        command,
        request_id,
        execute_at_epoch,
        auto_off_minutes,
        status: AirconScheduleStatus::Pending,
        created_epoch: epoch_seconds(),
        finished_epoch: 0,
        message: "等待执行".to_owned(),
    };
    document.next_schedule_id = document.next_schedule_id.saturating_add(1);
    document.schedules.push(schedule.clone());
    document.schedules.sort_by_key(|item| item.execute_at_epoch);
    document.revision = document.revision.saturating_add(1);
    document.prune_schedules();
    Ok(schedule)
}

pub struct AirconService {
    store: Mutex<AirconStore>,
    ir: Arc<IrControl>,
    execution: AsyncMutex<()>,
}

impl AirconService {
    pub fn start(config: &Config, ir: Arc<IrControl>) -> Result<Arc<Self>> {
        let service = Arc::new(Self {
            store: Mutex::new(AirconStore::load(config.aircon_config_file.clone())?),
            ir,
            execution: AsyncMutex::new(()),
        });
        let worker = service.clone();
        tokio::spawn(async move {
            loop {
                if let Err(error) = worker.run_due().await {
                    warn!(%error, "run scheduled aircon command failed");
                }
                sleep(Duration::from_secs(1)).await;
            }
        });
        Ok(service)
    }

    pub fn overview(&self) -> AirconOverview {
        let store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        let verification_complete = verification_complete(&store.current.verified_capabilities);
        AirconOverview {
            state: store.current.state.clone(),
            state_known: store.current.state_known,
            physical_confirmed: false,
            schedules: store.current.schedules.iter().rev().cloned().collect(),
            integration_token: if store.current.integration_enabled {
                store.current.integration_token.clone()
            } else {
                String::new()
            },
            integration_endpoint: "/api/v1/integrations/aircon/command",
            integration_enabled: store.current.integration_enabled,
            verification_complete,
            verified_capabilities: store
                .current
                .verified_capabilities
                .iter()
                .copied()
                .collect(),
            required_capabilities: REQUIRED_CAPABILITIES.to_vec(),
            temperature_min: 17,
            temperature_max: 30,
            modes: ["cool", "dry", "heat", "auto"],
            max_delay_minutes: MAX_DELAY_MINUTES,
            max_auto_off_minutes: MAX_AUTO_OFF_MINUTES,
        }
    }

    pub fn verify_token(&self, token: &str) -> bool {
        let store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        store.current.integration_enabled
            && !store.current.integration_token.is_empty()
            && constant_time_eq(store.current.integration_token.as_bytes(), token.as_bytes())
    }

    pub fn integration_enabled(&self) -> bool {
        self.store
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .current
            .integration_enabled
    }

    pub fn update_verification(
        &self,
        update: AirconVerificationUpdate,
    ) -> std::result::Result<AirconOverview, AirconError> {
        let mut store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        let mut next = store.current.clone();
        if update.verified {
            next.verified_capabilities.insert(update.capability);
        } else {
            next.verified_capabilities.remove(&update.capability);
            next.integration_enabled = false;
        }
        next.revision = next.revision.saturating_add(1);
        store.commit(next).map_err(internal_error)?;
        drop(store);
        Ok(self.overview())
    }

    pub fn update_integration(
        &self,
        update: AirconIntegrationUpdate,
    ) -> std::result::Result<AirconOverview, AirconError> {
        let mut store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        let mut next = store.current.clone();
        if update.enabled {
            if !verification_complete(&next.verified_capabilities) {
                return Err(AirconError::Invalid(
                    "请先完成全部空调遥控功能实测".to_owned(),
                ));
            }
            if !next.integration_enabled {
                next.integration_token = generate_token().map_err(internal_error)?;
            }
        }
        next.integration_enabled = update.enabled;
        next.revision = next.revision.saturating_add(1);
        store.commit(next).map_err(internal_error)?;
        drop(store);
        Ok(self.overview())
    }

    pub fn rotate_token(&self) -> std::result::Result<String, AirconError> {
        let token = generate_token().map_err(internal_error)?;
        let mut store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        if !store.current.integration_enabled {
            return Err(AirconError::Invalid("外部空调接口尚未启用".to_owned()));
        }
        let mut next = store.current.clone();
        next.integration_token.clone_from(&token);
        next.revision = next.revision.saturating_add(1);
        store.commit(next).map_err(internal_error)?;
        Ok(token)
    }

    pub async fn submit(
        &self,
        mut request: AirconCommandRequest,
    ) -> std::result::Result<AirconCommandResult, AirconError> {
        let _execution = self.execution.lock().await;
        normalize_request_id(&mut request.request_id)?;
        if request.command.is_cancellation() {
            if request.delay_minutes.is_some()
                || request.execute_at_epoch.is_some()
                || request.auto_off_minutes.is_some()
            {
                return Err(AirconError::Invalid(
                    "取消定时操作不能附带执行时间或自动关机".to_owned(),
                ));
            }
            return self.cancel(request.command);
        }

        let now = epoch_seconds();
        validate_auto_off(request.auto_off_minutes)?;
        if let Some(execute_at_epoch) = schedule_epoch(&request, now)? {
            let state = {
                self.store
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .current
                    .state
                    .clone()
            };
            let preview = request.command.apply(&state)?;
            if request.auto_off_minutes.is_some() && !preview.power {
                return Err(AirconError::Invalid(
                    "关闭空调操作不能设置自动关机".to_owned(),
                ));
            }
            let schedule = self
                .store
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .add_schedule(
                    request.command,
                    request.request_id,
                    execute_at_epoch,
                    request.auto_off_minutes,
                )?;
            let overview = self.overview();
            return Ok(AirconCommandResult {
                ok: true,
                status: AirconResultStatus::Scheduled,
                message: "空调定时任务已创建".to_owned(),
                state: overview.state,
                state_known: overview.state_known,
                physical_confirmed: false,
                transmission: None,
                schedule: Some(schedule),
            });
        }
        self.execute(request.command, request.auto_off_minutes)
            .await
    }

    fn cancel(
        &self,
        command: AirconOperation,
    ) -> std::result::Result<AirconCommandResult, AirconError> {
        let mut store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        let mut next = store.current.clone();
        let message = match command {
            AirconOperation::CancelSchedule { schedule_id } => {
                let schedule = next
                    .schedules
                    .iter_mut()
                    .find(|schedule| schedule.id == schedule_id)
                    .ok_or_else(|| AirconError::NotFound("定时任务不存在".to_owned()))?;
                if schedule.status != AirconScheduleStatus::Pending {
                    return Err(AirconError::Invalid("该定时任务已不能取消".to_owned()));
                }
                schedule.status = AirconScheduleStatus::Cancelled;
                schedule.finished_epoch = epoch_seconds();
                schedule.message = "已取消".to_owned();
                "已取消空调定时任务".to_owned()
            }
            AirconOperation::CancelAll {} => {
                let mut count = 0_u64;
                for schedule in &mut next.schedules {
                    if schedule.status == AirconScheduleStatus::Pending {
                        schedule.status = AirconScheduleStatus::Cancelled;
                        schedule.finished_epoch = epoch_seconds();
                        schedule.message = "已取消".to_owned();
                        count = count.saturating_add(1);
                    }
                }
                format!("已取消 {count} 个空调定时任务")
            }
            _ => return Err(AirconError::Invalid("不是取消定时操作".to_owned())),
        };
        next.revision = next.revision.saturating_add(1);
        store.commit(next).map_err(internal_error)?;
        Ok(AirconCommandResult {
            ok: true,
            status: AirconResultStatus::Cancelled,
            message,
            state: store.current.state.clone(),
            state_known: store.current.state_known,
            physical_confirmed: false,
            transmission: None,
            schedule: None,
        })
    }

    async fn execute(
        &self,
        operation: AirconOperation,
        auto_off_minutes: Option<u32>,
    ) -> std::result::Result<AirconCommandResult, AirconError> {
        let (current, was_known) = {
            let store = self.store.lock().unwrap_or_else(|error| error.into_inner());
            (store.current.state.clone(), store.current.state_known)
        };
        let next = operation.apply(&current)?;
        if auto_off_minutes.is_some() && !next.power {
            return Err(AirconError::Invalid(
                "关闭空调操作不能设置自动关机".to_owned(),
            ));
        }
        // Persist the off task before dispatch. A scheduled parent already owns
        // this slot, so transfer its reservation to the concrete off task.
        let auto_off = if let Some(minutes) = auto_off_minutes {
            let mut store = self.store.lock().unwrap_or_else(|error| error.into_inner());
            let mut document = store.current.clone();
            let schedule = insert_schedule(
                &mut document,
                AirconOperation::Off {},
                String::new(),
                epoch_seconds().saturating_add(u64::from(minutes) * 60),
                None,
            )?;
            store.commit(document).map_err(internal_error)?;
            Some(schedule)
        } else {
            None
        };
        let transmission = match self.ir.send_aircon(&next.to_ir_command()).await {
            Ok(transmission) => transmission,
            Err(error) => {
                let mut store = self.store.lock().unwrap_or_else(|error| error.into_inner());
                // A worker timeout does not prove that nothing was transmitted.
                store.current.state_known = false;
                // Retain the persisted off task: an uncertain send may have
                // switched the appliance on before the response was lost.
                store.current.revision = store.current.revision.saturating_add(1);
                let persistence = store.save().err();
                let mut message = match persistence {
                    Some(persistence) => format!("{error}；失败状态保存失败：{persistence}"),
                    None => error.to_string(),
                };
                if auto_off.is_some() {
                    message.push_str("；发送结果未确认，已保留自动关机任务");
                }
                return Err(AirconError::Delivery(message));
            }
        };

        let mut store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        store.current.state = next.clone();
        store.current.state_known = true;
        store.current.revision = store.current.revision.saturating_add(1);
        // The transmission already happened: keep the shadow state truthful
        // even if the disk write fails, and report the partial outcome.
        let persistence_error = store.save().err();
        let mut message = if was_known {
            next.message()
        } else {
            format!("{}（此前状态未知，已按默认状态补全）", next.message())
        };
        if let Some(error) = persistence_error {
            warn!(%error, "aircon transmitted but state persistence failed");
            message.push_str(&format!("；红外已发送，但状态保存失败：{error}"));
        }
        Ok(AirconCommandResult {
            ok: true,
            status: AirconResultStatus::Executed,
            message,
            state: next,
            state_known: true,
            physical_confirmed: false,
            transmission: Some(transmission),
            schedule: auto_off,
        })
    }

    async fn run_due(&self) -> Result<()> {
        let _execution = self.execution.lock().await;
        let due = {
            let mut store = self.store.lock().unwrap_or_else(|error| error.into_inner());
            let Some(index) = store.current.schedules.iter().position(|schedule| {
                schedule.status == AirconScheduleStatus::Pending
                    && schedule.execute_at_epoch <= epoch_seconds()
            }) else {
                return Ok(());
            };
            let mut next = store.current.clone();
            next.schedules[index].status = AirconScheduleStatus::Running;
            next.schedules[index].message = "正在执行".to_owned();
            next.revision = next.revision.saturating_add(1);
            let due = next.schedules[index].clone();
            store.commit(next)?;
            due
        };

        let result = self
            .execute(due.command.clone(), due.auto_off_minutes)
            .await;
        let mut store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(schedule) = store
            .current
            .schedules
            .iter_mut()
            .find(|schedule| schedule.id == due.id)
        {
            schedule.finished_epoch = epoch_seconds();
            match result {
                Ok(ref execution) => {
                    schedule.status = AirconScheduleStatus::Completed;
                    schedule.message.clone_from(&execution.message);
                    info!(schedule_id = %schedule.id, "scheduled aircon command completed");
                }
                Err(ref error) => {
                    schedule.status = AirconScheduleStatus::Failed;
                    schedule.message = error.to_string();
                    warn!(schedule_id = %schedule.id, %error, "scheduled aircon command failed");
                }
            }
        }
        store.current.revision = store.current.revision.saturating_add(1);
        store.save()
    }
}

fn schedule_epoch(
    request: &AirconCommandRequest,
    now: u64,
) -> std::result::Result<Option<u64>, AirconError> {
    if request.delay_minutes.is_some() && request.execute_at_epoch.is_some() {
        return Err(AirconError::Invalid(
            "delay_minutes 和 execute_at_epoch 不能同时设置".to_owned(),
        ));
    }
    if let Some(minutes) = request.delay_minutes {
        if minutes == 0 {
            return Ok(None);
        }
        if minutes > MAX_DELAY_MINUTES {
            return Err(AirconError::Invalid("定时不能超过 30 天".to_owned()));
        }
        return Ok(Some(now.saturating_add(u64::from(minutes) * 60)));
    }
    if let Some(epoch) = request.execute_at_epoch {
        if epoch <= now {
            return Err(AirconError::Invalid(
                "定时执行时间必须晚于当前时间".to_owned(),
            ));
        }
        if epoch > now.saturating_add(u64::from(MAX_DELAY_MINUTES) * 60) {
            return Err(AirconError::Invalid("定时不能超过 30 天".to_owned()));
        }
        return Ok(Some(epoch));
    }
    Ok(None)
}

fn validate_auto_off(minutes: Option<u32>) -> std::result::Result<(), AirconError> {
    if let Some(minutes) = minutes
        && !(1..=MAX_AUTO_OFF_MINUTES).contains(&minutes)
    {
        return Err(AirconError::Invalid(
            "自动关机时间必须在 1 分钟到 7 天之间".to_owned(),
        ));
    }
    Ok(())
}

fn normalize_request_id(value: &mut String) -> std::result::Result<(), AirconError> {
    *value = value.trim().to_owned();
    if value.len() > 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_alphanumeric() && !matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(AirconError::Invalid("request_id 格式无效".to_owned()));
    }
    Ok(())
}

fn generate_token() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).context("generate aircon integration token")?;
    Ok(format!("cha_{}", hex::encode(bytes)))
}

fn internal_error(error: impl std::fmt::Display) -> AirconError {
    AirconError::Internal(error.to_string())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let length = left.len().max(right.len());
    for index in 0..length {
        let lhs = left.get(index).copied().unwrap_or_default();
        let rhs = right.get(index).copied().unwrap_or_default();
        difference |= usize::from(lhs ^ rhs);
    }
    difference == 0
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, routing::post};
    use serde_json::{Value, json};

    fn temporary_config(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "camera-hub-aircon-{label}-{}-{nonce}.json",
            std::process::id()
        ))
    }

    #[test]
    fn failed_schedule_save_does_not_publish_task() {
        let path = temporary_config("save-failure");
        let mut store = AirconStore::load(path.clone()).unwrap();
        let revision = store.current.revision;
        let original_path = store.path.clone();
        // Use an ordinary file as the parent to deterministically reject writes.
        store.path = path.join("cannot-write.json");
        assert!(
            store
                .add_schedule(
                    AirconOperation::Off {},
                    "rejected".to_owned(),
                    epoch_seconds() + 60,
                    None
                )
                .is_err()
        );
        assert!(store.current.schedules.is_empty());
        assert_eq!(store.current.revision, revision);
        store.path = original_path;
        store.save().unwrap();
        assert!(
            AirconStore::load(path.clone())
                .unwrap()
                .current
                .schedules
                .is_empty()
        );
        fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn auto_off_capacity_is_rejected_before_dispatch() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let path = temporary_config("auto-off-capacity");
        let mut store = AirconStore::load(path.clone()).unwrap();
        for index in 0..MAX_SCHEDULES {
            store
                .add_schedule(
                    AirconOperation::Off {},
                    format!("task-{index}"),
                    epoch_seconds() + 3600,
                    None,
                )
                .unwrap();
        }
        // No listener: a capacity rejection must happen before touching the worker.
        let service = AirconService {
            store: Mutex::new(store),
            ir: Arc::new(IrControl::from_url("http://127.0.0.1:1", PathBuf::new()).unwrap()),
            execution: AsyncMutex::new(()),
        };
        let result = service
            .submit(AirconCommandRequest {
                command: AirconOperation::Set {
                    temperature_c: 24,
                    mode: AcMode::Cool,
                    eco: false,
                },
                request_id: String::new(),
                delay_minutes: None,
                execute_at_epoch: None,
                auto_off_minutes: Some(1),
            })
            .await;
        assert!(matches!(result, Err(AirconError::Invalid(_))), "{result:?}");
        assert!(!service.overview().state_known);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn applies_absolute_and_relative_operations() {
        let state = AirconState::default();
        let set = AirconOperation::Set {
            temperature_c: 24,
            mode: AcMode::Cool,
            eco: false,
        }
        .apply(&state)
        .unwrap();
        assert!(set.power);
        assert_eq!(set.temperature_c, 24);

        let adjusted = AirconOperation::AdjustTemperature { delta_c: 2 }
            .apply(&set)
            .unwrap();
        assert_eq!(adjusted.temperature_c, 26);

        let dry = AirconOperation::SetMode { mode: AcMode::Dry }
            .apply(&adjusted)
            .unwrap();
        assert_eq!(dry.mode, AcMode::Dry);
        assert!(!dry.eco);
        assert!(
            AirconOperation::SetEco { enabled: true }
                .apply(&dry)
                .is_err()
        );
    }

    #[test]
    fn validates_schedule_bounds() {
        let request = AirconCommandRequest {
            command: AirconOperation::Off {},
            request_id: String::new(),
            delay_minutes: Some(30),
            execute_at_epoch: None,
            auto_off_minutes: None,
        };
        assert_eq!(schedule_epoch(&request, 1_000).unwrap(), Some(2_800));

        let invalid = AirconCommandRequest {
            delay_minutes: Some(MAX_DELAY_MINUTES + 1),
            ..request
        };
        assert!(schedule_epoch(&invalid, 1_000).is_err());
    }

    #[test]
    fn persists_token_state_and_schedules() {
        let path = temporary_config("persistence");
        let mut store = AirconStore::load(path.clone()).unwrap();
        let token = store.current.integration_token.clone();
        store.current.state = AirconState {
            power: true,
            temperature_c: 23,
            mode: AcMode::Cool,
            eco: false,
        };
        store.current.state_known = true;
        store
            .add_schedule(
                AirconOperation::Off {},
                "request-1".to_owned(),
                epoch_seconds() + 60,
                None,
            )
            .unwrap();
        drop(store);

        let reloaded = AirconStore::load(path.clone()).unwrap();
        assert_eq!(reloaded.current.integration_token, token);
        assert_eq!(reloaded.current.state.temperature_c, 23);
        assert_eq!(reloaded.current.schedules.len(), 1);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn gates_external_integration_on_persisted_physical_verification() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let path = temporary_config("verification-gate");
        let service = AirconService {
            store: Mutex::new(AirconStore::load(path.clone()).unwrap()),
            ir: Arc::new(IrControl::from_url("http://127.0.0.1:1", PathBuf::new()).unwrap()),
            execution: AsyncMutex::new(()),
        };
        let original_token = service
            .store
            .lock()
            .unwrap()
            .current
            .integration_token
            .clone();
        assert!(!service.overview().integration_enabled);
        assert!(!service.verify_token(&original_token));
        assert!(matches!(
            service.update_integration(AirconIntegrationUpdate { enabled: true }),
            Err(AirconError::Invalid(_))
        ));

        for capability in REQUIRED_CAPABILITIES {
            service
                .update_verification(AirconVerificationUpdate {
                    capability,
                    verified: true,
                })
                .unwrap();
        }
        let verified = service.overview();
        assert!(verified.verification_complete);
        assert!(!verified.integration_enabled);
        assert!(verified.integration_token.is_empty());

        let enabled = service
            .update_integration(AirconIntegrationUpdate { enabled: true })
            .unwrap();
        assert!(enabled.integration_enabled);
        assert_ne!(enabled.integration_token, original_token);
        assert!(service.verify_token(&enabled.integration_token));

        let disabled = service
            .update_verification(AirconVerificationUpdate {
                capability: AirconCapability::EcoOff,
                verified: false,
            })
            .unwrap();
        assert!(!disabled.integration_enabled);
        assert!(!disabled.verification_complete);
        assert!(disabled.integration_token.is_empty());
        assert!(!service.verify_token(&enabled.integration_token));

        drop(service);
        let reloaded = AirconStore::load(path.clone()).unwrap();
        assert!(!reloaded.current.integration_enabled);
        assert!(
            !reloaded
                .current
                .verified_capabilities
                .contains(&AirconCapability::EcoOff)
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_new_schedule_when_active_capacity_is_reached() {
        let path = temporary_config("capacity");
        let mut store = AirconStore::load(path.clone()).unwrap();
        for index in 0..MAX_SCHEDULES {
            store
                .add_schedule(
                    AirconOperation::Off {},
                    format!("request-{index}"),
                    epoch_seconds() + index as u64 + 60,
                    None,
                )
                .unwrap();
        }

        let error = store
            .add_schedule(
                AirconOperation::Off {},
                "overflow".to_owned(),
                epoch_seconds() + 3_600,
                None,
            )
            .unwrap_err();
        assert!(matches!(error, AirconError::Invalid(_)));
        assert_eq!(store.current.schedules.len(), MAX_SCHEDULES);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn delayed_auto_off_reserves_its_child_capacity() {
        let path = temporary_config("child-capacity");
        let mut store = AirconStore::load(path.clone()).unwrap();
        for index in 0..MAX_SCHEDULES - 2 {
            store
                .add_schedule(
                    AirconOperation::Off {},
                    format!("task-{index}"),
                    epoch_seconds() + 3600,
                    None,
                )
                .unwrap();
        }
        let parent = store
            .add_schedule(
                AirconOperation::Power { enabled: true },
                "parent".to_owned(),
                epoch_seconds() + 60,
                Some(30),
            )
            .unwrap();
        assert!(matches!(
            store.add_schedule(
                AirconOperation::Off {},
                "overflow".to_owned(),
                epoch_seconds() + 3600,
                None,
            ),
            Err(AirconError::Invalid(_))
        ));
        // Execution transfers the reserved child slot without requiring a new one.
        let mut next = store.current.clone();
        let task = next
            .schedules
            .iter_mut()
            .find(|task| task.id == parent.id)
            .unwrap();
        task.status = AirconScheduleStatus::Running;
        insert_schedule(
            &mut next,
            AirconOperation::Off {},
            String::new(),
            epoch_seconds() + 1800,
            None,
        )
        .unwrap();
        assert_eq!(next.schedules.len(), MAX_SCHEDULES);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn integration_query_maps_to_typed_request() {
        let request = AirconIntegrationQuery {
            token: "token".to_owned(),
            operation: "set".to_owned(),
            temperature_c: Some(24),
            mode: Some(AcMode::Cool),
            eco: Some(false),
            enabled: None,
            delta_c: None,
            schedule_id: String::new(),
            request_id: "doubao-1".to_owned(),
            delay_minutes: Some(120),
            execute_at_epoch: None,
            auto_off_minutes: Some(60),
        }
        .into_request()
        .unwrap();
        assert_eq!(request.delay_minutes, Some(120));
        assert_eq!(request.auto_off_minutes, Some(60));
        assert!(matches!(request.command, AirconOperation::Set { .. }));
    }

    #[tokio::test]
    async fn executes_immediate_and_scheduled_commands_through_ir_worker() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        let app = Router::new().route(
            "/v1/aircon/state",
            post(move |Json(body): Json<Value>| {
                let tx = tx.clone();
                async move {
                    tx.send(body).unwrap();
                    Json(json!({"ok":true,"transmission":{
                        "action":"aircon-state","label":"空调状态","carrier_hz":38000,
                        "pulse_count":500,"duration_us":560000,"spi_bytes":67584
                    }}))
                }
            }),
        );
        let worker_url = format!("http://{}", listener.local_addr().unwrap());
        let worker = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let ir = Arc::new(IrControl::from_url(&worker_url, PathBuf::new()).unwrap());
        let path = temporary_config("execution");
        let service = AirconService {
            store: Mutex::new(AirconStore::load(path.clone()).unwrap()),
            ir,
            execution: AsyncMutex::new(()),
        };

        let immediate = service
            .submit(AirconCommandRequest {
                command: AirconOperation::Set {
                    temperature_c: 24,
                    mode: AcMode::Cool,
                    eco: false,
                },
                request_id: String::new(),
                delay_minutes: None,
                execute_at_epoch: None,
                auto_off_minutes: Some(30),
            })
            .await
            .unwrap();
        assert_eq!(immediate.status, AirconResultStatus::Executed);
        assert!(matches!(
            immediate
                .schedule
                .as_ref()
                .map(|schedule| &schedule.command),
            Some(AirconOperation::Off {})
        ));
        assert_eq!(
            rx.recv().await.unwrap(),
            json!({"operation":"set","temperature_c":24,"mode":"cool","eco":false})
        );

        let scheduled = service
            .submit(AirconCommandRequest {
                command: AirconOperation::AdjustTemperature { delta_c: 1 },
                request_id: "scheduled-1".to_owned(),
                delay_minutes: Some(1),
                execute_at_epoch: None,
                auto_off_minutes: None,
            })
            .await
            .unwrap();
        let schedule_id = scheduled.schedule.unwrap().id;
        {
            let mut store = service
                .store
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            store
                .current
                .schedules
                .iter_mut()
                .find(|schedule| schedule.id == schedule_id)
                .unwrap()
                .execute_at_epoch = epoch_seconds();
            store.save().unwrap();
        }
        service.run_due().await.unwrap();
        assert_eq!(
            rx.recv().await.unwrap(),
            json!({"operation":"set","temperature_c":25,"mode":"cool","eco":false})
        );
        let overview = service.overview();
        assert_eq!(overview.state.temperature_c, 25);
        assert_eq!(
            overview
                .schedules
                .iter()
                .find(|schedule| schedule.id == schedule_id)
                .unwrap()
                .status,
            AirconScheduleStatus::Completed
        );

        worker.abort();
        fs::remove_file(path).unwrap();
    }
}
