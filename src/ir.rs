use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_CARRIER_HZ: u32 = 38_000;
const DEFAULT_SAMPLE_HZ: u32 = 960_000;
const DEFAULT_BITS_PER_WORD: u8 = 32;
const DUTY_PERCENT: u64 = 33;
const MAX_PULSES: usize = 1024;
const MAX_DURATION_US: u64 = 1_000_000;
const MAX_SPI_BYTES: usize = 512 * 1024;
const SPI_ALIGNMENT_BYTES: usize = 1024;
const SPI_CLOCK_PATH: &str =
    "/sys/firmware/devicetree/base/soc/spi@c176000/peel_ir@0/peel_ir,spi-clk-speed";
const SPI_BPW_PATH: &str =
    "/sys/firmware/devicetree/base/soc/spi@c176000/peel_ir@0/peel_ir,spi-bpw";

const MIDEA_MODERN_ON_COOL_26_AUTO: u64 = 0xA18009FFFF37;
const MIDEA_MODERN_OFF: u64 = 0xA10009FFFFB7;
const MIDEA_MODERN_ECO_TOGGLE: u64 = 0xA202FFFFFF7E;
const RN02_AUTO_FAN: u8 = 0xFD;
const RN02_COOL_26: u8 = 0x0B;
const RN02_DRY_26: u8 = 0x2B;
const RN02_TEMPERATURES: [u8; 14] = [
    0x0, 0x8, 0xC, 0x4, 0x6, 0xE, 0xA, 0x2, 0x3, 0xB, 0x9, 0x1, 0x5, 0xD,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcMode {
    Cool,
    Dry,
    Heat,
    Auto,
}

impl AcMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cool => "制冷",
            Self::Dry => "抽湿",
            Self::Heat => "制热",
            Self::Auto => "自动模式",
        }
    }

    fn code(self) -> u8 {
        match self {
            Self::Cool => 0x00,
            Self::Dry => 0x20,
            Self::Heat => 0x30,
            Self::Auto => 0x10,
        }
    }
}

/// Explicit RN02S operations. A set operation sends a complete state, not a
/// delta against an assumed appliance state (infrared has no acknowledgement).
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum AirconCommand {
    Set {
        temperature_c: u8,
        mode: AcMode,
        eco: bool,
    },
    Off {},
    Eco {
        enabled: bool,
    },
}

impl AirconCommand {
    pub fn validate(&self) -> Result<()> {
        if let Self::Set {
            temperature_c,
            mode,
            eco,
        } = self
        {
            if !(17..=30).contains(temperature_c) {
                bail!("空调温度仅支持 17–30°C 的整数");
            }
            if *eco && *mode != AcMode::Cool {
                bail!("ECO 仅支持与制冷模式一起设置");
            }
        }
        Ok(())
    }

    pub fn label(&self) -> String {
        match self {
            Self::Set {
                temperature_c,
                mode,
                eco,
            } => format!(
                "{} {}°C、自动风、ECO {}",
                mode.label(),
                temperature_c,
                if *eco { "开启" } else { "关闭" }
            ),
            Self::Off {} => "关闭空调".to_owned(),
            Self::Eco { enabled } => format!("{}空调 ECO", if *enabled { "开启" } else { "关闭" }),
        }
    }
}

pub fn aircon_pattern(command: &AirconCommand) -> Result<PulsePattern> {
    command.validate()?;
    let pattern = match command {
        AirconCommand::Set {
            temperature_c,
            mode,
            eco,
        } => {
            let c = mode.code() | RN02_TEMPERATURES[usize::from(*temperature_c - 17)];
            let mut pattern = rn02_state(c);
            pattern.append(
                &rn02_short(0xB9, 0xAF, if *eco { 0x24 } else { 0xA4 }),
                20_000,
            );
            pattern
        }
        AirconCommand::Off {} => rn02_short(0xB2, 0xDE, 0x07),
        AirconCommand::Eco { enabled } => {
            rn02_short(0xB9, 0xAF, if *enabled { 0x24 } else { 0xA4 })
        }
    };
    pattern.validate()?;
    Ok(pattern)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrAction {
    AcOn,
    AcCool,
    AcDry,
    AcOff,
    OnModern,
    OnModernEco,
    OnRn02,
    OnRn02Eco,
    OffModern,
    OffRn02,
}

impl IrAction {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "ac-on" => Ok(Self::AcOn),
            "ac-cool" | "ac-on-no-eco" => Ok(Self::AcCool),
            "ac-dry" => Ok(Self::AcDry),
            "ac-off" => Ok(Self::AcOff),
            "on-modern" => Ok(Self::OnModern),
            "on-modern-eco" => Ok(Self::OnModernEco),
            "on-rn02" => Ok(Self::OnRn02),
            "on-rn02-eco" => Ok(Self::OnRn02Eco),
            "off-modern" => Ok(Self::OffModern),
            "off-rn02" => Ok(Self::OffRn02),
            _ => bail!("不支持的红外动作：{value}"),
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::AcOn => "ac-on",
            Self::AcCool => "ac-cool",
            Self::AcDry => "ac-dry",
            Self::AcOff => "ac-off",
            Self::OnModern => "on-modern",
            Self::OnModernEco => "on-modern-eco",
            Self::OnRn02 => "on-rn02",
            Self::OnRn02Eco => "on-rn02-eco",
            Self::OffModern => "off-modern",
            Self::OffRn02 => "off-rn02",
        }
    }

    fn definition(self) -> IrActionDefinition {
        match self {
            Self::AcOn => IrActionDefinition {
                id: self.id(),
                label: "打开空调",
                detail: "已验证：制冷 26°C、自动风，并开启 ECO",
                group: "control",
            },
            Self::AcCool => IrActionDefinition {
                id: self.id(),
                label: "制冷 26°C",
                detail: "RN02S 制冷 26°C、自动风，并关闭 ECO",
                group: "control",
            },
            Self::AcDry => IrActionDefinition {
                id: self.id(),
                label: "抽湿 26°C",
                detail: "RN02S 抽湿 26°C，并关闭 ECO",
                group: "control",
            },
            Self::AcOff => IrActionDefinition {
                id: self.id(),
                label: "关闭空调",
                detail: "已验证：RN02S 明确关机帧",
                group: "control",
            },
            Self::OnModern => IrActionDefinition {
                id: self.id(),
                label: "开机方案 A",
                detail: "标准 Midea 48-bit，制冷 26°C，自动风",
                group: "on",
            },
            Self::OnModernEco => IrActionDefinition {
                id: self.id(),
                label: "开机方案 B",
                detail: "标准 Midea 48-bit，制冷 26°C，自动风，再发送 ECO 切换帧",
                group: "on",
            },
            Self::OnRn02 => IrActionDefinition {
                id: self.id(),
                label: "开机方案 C",
                detail: "RN02S 长帧，制冷 26°C，自动风",
                group: "on",
            },
            Self::OnRn02Eco => IrActionDefinition {
                id: self.id(),
                label: "开机方案 D",
                detail: "RN02S 长帧，制冷 26°C，自动风，再明确开启 ECO",
                group: "on",
            },
            Self::OffModern => IrActionDefinition {
                id: self.id(),
                label: "关机方案 A",
                detail: "标准 Midea 48-bit 明确关机帧",
                group: "off",
            },
            Self::OffRn02 => IrActionDefinition {
                id: self.id(),
                label: "关机方案 B",
                detail: "RN02S 明确关机帧",
                group: "off",
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct IrActionDefinition {
    pub id: &'static str,
    pub label: &'static str,
    pub detail: &'static str,
    pub group: &'static str,
}

pub fn action_catalog() -> Vec<IrActionDefinition> {
    [
        IrAction::AcOn,
        IrAction::AcCool,
        IrAction::AcDry,
        IrAction::AcOff,
    ]
    .into_iter()
    .map(IrAction::definition)
    .collect()
}

#[derive(Clone, Debug)]
pub struct PulsePattern {
    pub carrier_hz: u32,
    pub durations_us: Vec<u32>,
}

impl PulsePattern {
    fn new() -> Self {
        Self {
            carrier_hz: DEFAULT_CARRIER_HZ,
            durations_us: Vec::new(),
        }
    }

    fn push(&mut self, duration_us: u32) {
        self.durations_us.push(duration_us);
    }

    fn append(&mut self, other: &Self, gap_us: u32) {
        if !self.durations_us.is_empty() {
            if self.durations_us.len().is_multiple_of(2) {
                if let Some(space) = self.durations_us.last_mut() {
                    *space = space.saturating_add(gap_us);
                }
            } else {
                self.push(gap_us);
            }
        }
        self.durations_us.extend_from_slice(&other.durations_us);
    }

    fn validate(&self) -> Result<()> {
        if !(30_000..=60_000).contains(&self.carrier_hz) {
            bail!("红外载波频率超出安全范围");
        }
        if self.durations_us.is_empty() || self.durations_us.len() > MAX_PULSES {
            bail!("红外波形脉冲数量无效");
        }
        if self
            .durations_us
            .iter()
            .any(|duration| *duration == 0 || *duration > 100_000)
        {
            bail!("红外波形包含无效脉冲时长");
        }
        let total = self
            .durations_us
            .iter()
            .map(|duration| u64::from(*duration))
            .sum::<u64>();
        if total > MAX_DURATION_US {
            bail!("红外波形总时长超过 1 秒");
        }
        Ok(())
    }
}

pub fn action_pattern(action: IrAction) -> Result<PulsePattern> {
    let pattern = match action {
        IrAction::AcOn => {
            let mut pattern = rn02_state(RN02_COOL_26);
            pattern.append(&rn02_short(0xB9, 0xAF, 0x24), 20_000);
            pattern
        }
        IrAction::AcCool => {
            let mut pattern = rn02_state(RN02_COOL_26);
            pattern.append(&rn02_short(0xB9, 0xAF, 0xA4), 20_000);
            pattern
        }
        IrAction::AcDry => {
            let mut pattern = rn02_state(RN02_DRY_26);
            pattern.append(&rn02_short(0xB9, 0xAF, 0xA4), 20_000);
            pattern
        }
        IrAction::AcOff => rn02_short(0xB2, 0xDE, 0x07),
        IrAction::OnModern => modern_midea(MIDEA_MODERN_ON_COOL_26_AUTO),
        IrAction::OnModernEco => {
            let mut pattern = modern_midea(MIDEA_MODERN_ON_COOL_26_AUTO);
            pattern.append(&modern_midea(MIDEA_MODERN_ECO_TOGGLE), 20_000);
            pattern
        }
        IrAction::OnRn02 => rn02_state(RN02_COOL_26),
        IrAction::OnRn02Eco => {
            let mut pattern = rn02_state(RN02_COOL_26);
            pattern.append(&rn02_short(0xB9, 0xAF, 0x24), 20_000);
            pattern
        }
        IrAction::OffModern => modern_midea(MIDEA_MODERN_OFF),
        IrAction::OffRn02 => rn02_short(0xB2, 0xDE, 0x07),
    };
    pattern.validate()?;
    Ok(pattern)
}

fn modern_midea(state: u64) -> PulsePattern {
    let mut pattern = PulsePattern::new();
    let bytes = state.to_be_bytes();
    let payload = &bytes[2..];
    for inverted in [false, true] {
        pattern.push(4_480);
        pattern.push(4_480);
        for byte in payload {
            push_byte(&mut pattern, if inverted { !byte } else { *byte }, true);
        }
        pattern.push(560);
        pattern.push(5_600);
    }
    pattern
}

fn rn02_state(mode_and_temperature: u8) -> PulsePattern {
    let mut pattern = PulsePattern::new();
    rn02_block(
        &mut pattern,
        [
            (0xB2, true),
            (!0xB2, true),
            (RN02_AUTO_FAN, false),
            (!RN02_AUTO_FAN, false),
            (mode_and_temperature, false),
            (!mode_and_temperature, false),
        ],
    );
    rn02_block(
        &mut pattern,
        [
            (0xB2, true),
            (!0xB2, true),
            (RN02_AUTO_FAN, false),
            (!RN02_AUTO_FAN, false),
            (mode_and_temperature, false),
            (!mode_and_temperature, false),
        ],
    );
    rn02_block(
        &mut pattern,
        [
            (0xAB, false),
            (0x66, false),
            (0x00, false),
            (0x00, false),
            (0x00, false),
            (0xDC, false),
        ],
    );
    pattern
}

fn rn02_short(a: u8, b: u8, c: u8) -> PulsePattern {
    let mut pattern = PulsePattern::new();
    for _ in 0..2 {
        rn02_block(
            &mut pattern,
            [
                (a, true),
                (!a, true),
                (b, false),
                (!b, false),
                (c, false),
                (!c, false),
            ],
        );
    }
    pattern
}

fn rn02_block(pattern: &mut PulsePattern, bytes: [(u8, bool); 6]) {
    pattern.push(4_400);
    pattern.push(4_400);
    for (byte, msb_first) in bytes {
        push_byte(pattern, byte, msb_first);
    }
    pattern.push(500);
    pattern.push(5_220);
}

fn push_byte(pattern: &mut PulsePattern, byte: u8, msb_first: bool) {
    for index in 0..8 {
        let shift = if msb_first { 7 - index } else { index };
        let one = byte & (1 << shift) != 0;
        pattern.push(500);
        pattern.push(if one { 1_600 } else { 550 });
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IrTransmission {
    pub action: String,
    pub label: String,
    pub carrier_hz: u32,
    pub pulse_count: usize,
    pub duration_us: u64,
    pub spi_bytes: usize,
}

pub struct PeelIrTransmitter {
    device: PathBuf,
    sample_hz: u32,
    bits_per_word: u8,
}

impl PeelIrTransmitter {
    pub fn open(device: PathBuf) -> Result<Self> {
        if !device.exists() {
            bail!("红外设备不存在：{}", device.display());
        }
        let sample_hz =
            read_device_tree_u32(Path::new(SPI_CLOCK_PATH)).unwrap_or(DEFAULT_SAMPLE_HZ);
        let bits_per_word = read_device_tree_u32(Path::new(SPI_BPW_PATH))
            .unwrap_or(DEFAULT_BITS_PER_WORD as u32) as u8;
        if !(100_000..=5_000_000).contains(&sample_hz) {
            bail!("红外 SPI 时钟无效：{sample_hz}");
        }
        if bits_per_word != 32 {
            bail!("当前只支持 32-bit peel_ir SPI，设备报告 {bits_per_word}");
        }
        Ok(Self {
            device,
            sample_hz,
            bits_per_word,
        })
    }

    pub fn transmit(&self, action: IrAction) -> Result<IrTransmission> {
        let pattern = action_pattern(action)?;
        let definition = action.definition();
        self.transmit_pattern(&pattern, definition.id, definition.label.to_owned())
    }

    pub fn transmit_aircon(&self, command: &AirconCommand) -> Result<IrTransmission> {
        let pattern = aircon_pattern(command)?;
        self.transmit_pattern(&pattern, "aircon-state", command.label())
    }

    fn transmit_pattern(
        &self,
        pattern: &PulsePattern,
        action: &str,
        label: String,
    ) -> Result<IrTransmission> {
        let data = encode_spi(&pattern, self.sample_hz)?;
        transmit_spi(&self.device, &data, self.sample_hz, self.bits_per_word)?;
        Ok(IrTransmission {
            action: action.to_owned(),
            label,
            carrier_hz: pattern.carrier_hz,
            pulse_count: pattern.durations_us.len(),
            duration_us: pattern
                .durations_us
                .iter()
                .map(|duration| u64::from(*duration))
                .sum(),
            spi_bytes: data.len(),
        })
    }
}

fn read_device_tree_u32(path: &Path) -> Option<u32> {
    let bytes = fs::read(path).ok()?;
    let value = bytes.get(..4)?.try_into().ok()?;
    Some(u32::from_be_bytes(value))
}

pub fn encode_spi(pattern: &PulsePattern, sample_hz: u32) -> Result<Vec<u8>> {
    pattern.validate()?;
    let total_us = pattern
        .durations_us
        .iter()
        .map(|duration| u64::from(*duration))
        .sum::<u64>();
    let sample_count = total_us
        .saturating_mul(u64::from(sample_hz))
        .div_ceil(1_000_000);
    let byte_count =
        usize::try_from(sample_count.div_ceil(8)).context("红外波形长度超出平台限制")?;
    let aligned = byte_count.div_ceil(SPI_ALIGNMENT_BYTES) * SPI_ALIGNMENT_BYTES;
    if aligned == 0 || aligned > MAX_SPI_BYTES {
        bail!("红外 SPI 数据长度无效");
    }
    let mut data = vec![0_u8; aligned];
    let mut sample_cursor = 0_u64;
    let mut rounded_cursor = 0_u64;
    for (index, duration_us) in pattern.durations_us.iter().enumerate() {
        rounded_cursor =
            rounded_cursor.saturating_add(u64::from(*duration_us) * u64::from(sample_hz));
        let segment_end = rounded_cursor.div_ceil(1_000_000);
        if index.is_multiple_of(2) {
            for sample in sample_cursor..segment_end {
                let phase =
                    sample.saturating_mul(u64::from(pattern.carrier_hz)) % u64::from(sample_hz);
                if phase < u64::from(sample_hz) * DUTY_PERCENT / 100 {
                    set_spi_bit(&mut data, sample as usize);
                }
            }
        }
        sample_cursor = segment_end;
    }
    for word in data.chunks_exact_mut(4) {
        word.reverse();
    }
    Ok(data)
}

fn set_spi_bit(data: &mut [u8], sample: usize) {
    let byte = sample / 8;
    let bit = 7 - sample % 8;
    if let Some(value) = data.get_mut(byte) {
        *value |= 1 << bit;
    }
}

#[cfg(target_os = "linux")]
fn transmit_spi(device: &Path, data: &[u8], speed_hz: u32, bits_per_word: u8) -> Result<()> {
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;

    const SPI_IOC_WR_MSG: libc::c_ulong = 0x4001_6B02;

    #[repr(C)]
    struct SpiIocTransfer {
        tx_buf: u64,
        rx_buf: u64,
        len: u32,
        speed_hz: u32,
        delay_usecs: u16,
        bits_per_word: u8,
        cs_change: u8,
    }

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(device)
        .with_context(|| format!("打开红外设备 {}", device.display()))?;
    let transfer = SpiIocTransfer {
        tx_buf: data.as_ptr() as usize as u64,
        rx_buf: 0,
        len: u32::try_from(data.len()).context("红外 SPI 数据过长")?,
        speed_hz,
        delay_usecs: 0,
        bits_per_word,
        cs_change: 0,
    };
    let result = unsafe { libc::ioctl(file.as_raw_fd(), SPI_IOC_WR_MSG, &transfer) };
    if result < 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("向 {} 发送红外波形", device.display()));
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn transmit_spi(_device: &Path, _data: &[u8], _speed_hz: u32, _bits_per_word: u8) -> Result<()> {
    bail!("当前平台不支持 peel_ir")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire_bytes(pattern: &PulsePattern, block: usize) -> Vec<u8> {
        pattern.durations_us[block * 100 + 2..block * 100 + 98]
            .chunks_exact(16)
            .map(|byte| {
                byte.chunks_exact(2).fold(0, |value, bit| {
                    assert_eq!(bit[0], 500);
                    assert!(matches!(bit[1], 550 | 1600));
                    (value << 1) | u8::from(bit[1] == 1600)
                })
            })
            .collect()
    }

    #[test]
    fn parameterized_states_match_independent_rn02_wire_vectors() {
        // Chronological MSB bytes, independently documented RN02/Coolix table.
        let wire_temperatures = [
            0x00, 0x10, 0x30, 0x20, 0x60, 0x70, 0x50, 0x40, 0xC0, 0xD0, 0x90, 0x80, 0xA0, 0xB0,
        ];
        for (mode, mode_bits) in [
            (AcMode::Cool, 0x00),
            (AcMode::Dry, 0x04),
            (AcMode::Heat, 0x0C),
            (AcMode::Auto, 0x08),
        ] {
            for (index, temp_bits) in wire_temperatures.into_iter().enumerate() {
                let command = AirconCommand::Set {
                    temperature_c: index as u8 + 17,
                    mode,
                    eco: false,
                };
                let pattern = aircon_pattern(&command).unwrap();
                let c = temp_bits | mode_bits;
                assert_eq!(wire_bytes(&pattern, 0), [0xB2, 0x4D, 0xBF, 0x40, c, !c]);
                assert_eq!(wire_bytes(&pattern, 1), wire_bytes(&pattern, 0));
                assert_eq!(wire_bytes(&pattern, 2), [0xD5, 0x66, 0, 0, 0, 0x3B]);
                assert_eq!(
                    wire_bytes(&pattern, 3),
                    [0xB9, 0x46, 0xF5, 0x0A, 0x25, 0xDA]
                );
                assert_eq!(pattern.durations_us.len(), 500);
                assert_eq!(&pattern.durations_us[..2], &[4400, 4400]);
                assert_eq!(
                    encode_spi(&pattern, DEFAULT_SAMPLE_HZ).unwrap().len() % 1024,
                    0
                );
            }
        }
    }

    #[test]
    fn preserves_all_four_existing_fixed_action_waveforms() {
        for (action, command) in [
            (
                IrAction::AcOn,
                AirconCommand::Set {
                    temperature_c: 26,
                    mode: AcMode::Cool,
                    eco: true,
                },
            ),
            (
                IrAction::AcCool,
                AirconCommand::Set {
                    temperature_c: 26,
                    mode: AcMode::Cool,
                    eco: false,
                },
            ),
            (
                IrAction::AcDry,
                AirconCommand::Set {
                    temperature_c: 26,
                    mode: AcMode::Dry,
                    eco: false,
                },
            ),
            (IrAction::AcOff, AirconCommand::Off {}),
        ] {
            assert_eq!(
                aircon_pattern(&command).unwrap().durations_us,
                action_pattern(action).unwrap().durations_us
            );
        }
        let eco = aircon_pattern(&AirconCommand::Eco { enabled: true }).unwrap();
        assert_eq!(wire_bytes(&eco, 0), [0xB9, 0x46, 0xF5, 0x0A, 0x24, 0xDB]);
    }

    #[test]
    fn rejects_invalid_aircon_parameters_before_encoding() {
        assert_eq!(
            serde_json::from_str::<AirconCommand>(r#"{"operation":"off"}"#).unwrap(),
            AirconCommand::Off {},
        );
        for temperature_c in [0, 16, 31, 255] {
            let command = AirconCommand::Set {
                temperature_c,
                mode: AcMode::Cool,
                eco: false,
            };
            assert!(command.validate().is_err());
            assert!(aircon_pattern(&command).is_err());
        }
        for mode in [AcMode::Dry, AcMode::Heat, AcMode::Auto] {
            assert!(
                AirconCommand::Set {
                    temperature_c: 26,
                    mode,
                    eco: true
                }
                .validate()
                .is_err()
            );
        }
        for json in [
            r#"{"operation":"set","temperature_c":20.5,"mode":"cool","eco":false}"#,
            r#"{"operation":"set","temperature_c":-1,"mode":"cool","eco":false}"#,
            r#"{"operation":"set","temperature_c":20,"mode":"fan","eco":false}"#,
            r#"{"operation":"set","temperature_c":20,"mode":"cool","eco":false,"timer":60}"#,
            r#"{"operation":"off","temperature_c":20}"#,
            r#"{"operation":"eco","enabled":true,"mode":"dry"}"#,
            r#"{"operation":"set","temperature_c":20,"mode":"cool"}"#,
        ] {
            assert!(
                serde_json::from_str::<AirconCommand>(json).is_err(),
                "{json}"
            );
        }
    }

    #[test]
    fn exposes_only_fixed_air_conditioner_actions() {
        assert!(IrAction::parse("ac-on").is_ok());
        assert!(IrAction::parse("ac-cool").is_ok());
        assert!(IrAction::parse("ac-dry").is_ok());
        assert!(IrAction::parse("ac-off").is_ok());
        assert!(IrAction::parse("on-modern").is_ok());
        assert!(IrAction::parse("on-rn02-eco").is_ok());
        assert!(IrAction::parse("raw").is_err());
    }

    #[test]
    fn builds_bounded_midea_patterns() {
        for action in [
            IrAction::AcOn,
            IrAction::AcCool,
            IrAction::AcDry,
            IrAction::AcOff,
            IrAction::OnModern,
            IrAction::OnModernEco,
            IrAction::OnRn02,
            IrAction::OnRn02Eco,
            IrAction::OffModern,
            IrAction::OffRn02,
        ] {
            let pattern = action_pattern(action).unwrap();
            pattern.validate().unwrap();
            let encoded = encode_spi(&pattern, DEFAULT_SAMPLE_HZ).unwrap();
            assert!(!encoded.is_empty());
            assert_eq!(encoded.len() % SPI_ALIGNMENT_BYTES, 0);
            assert!(encoded.len() <= MAX_SPI_BYTES);
        }
    }

    #[test]
    fn modern_on_and_off_are_explicit_states() {
        assert_ne!(MIDEA_MODERN_ON_COOL_26_AUTO, MIDEA_MODERN_OFF);
        assert_eq!(MIDEA_MODERN_ON_COOL_26_AUTO, 0xA18009FFFF37);
        assert_eq!(MIDEA_MODERN_OFF, 0xA10009FFFFB7);
    }

    #[test]
    fn rn02_cool_and_dry_use_distinct_mode_bytes() {
        assert_eq!(RN02_COOL_26, 0x0B);
        assert_eq!(RN02_DRY_26, 0x2B);
        assert_ne!(
            action_pattern(IrAction::AcCool).unwrap().durations_us,
            action_pattern(IrAction::AcDry).unwrap().durations_us
        );
    }

    #[test]
    fn spi_encoder_contains_carrier_and_idle_space() {
        let pattern = PulsePattern {
            carrier_hz: DEFAULT_CARRIER_HZ,
            durations_us: vec![1_000, 1_000],
        };
        let encoded = encode_spi(&pattern, DEFAULT_SAMPLE_HZ).unwrap();
        assert!(encoded.iter().any(|byte| *byte != 0));
        let nonzero = encoded.iter().filter(|byte| **byte != 0).count();
        assert!(nonzero < encoded.len());
    }
}
