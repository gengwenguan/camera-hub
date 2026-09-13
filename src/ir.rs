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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrAction {
    AcOn,
    AcOnNoEco,
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
            "ac-on-no-eco" => Ok(Self::AcOnNoEco),
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
            Self::AcOnNoEco => "ac-on-no-eco",
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
            Self::AcOnNoEco => IrActionDefinition {
                id: self.id(),
                label: "制冷 26°C",
                detail: "已验证：制冷 26°C、自动风，不改变 ECO",
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
    [IrAction::AcOn, IrAction::AcOnNoEco, IrAction::AcOff]
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
            let mut pattern = rn02_state();
            pattern.append(&rn02_short(0xB9, 0xAF, 0x24), 20_000);
            pattern
        }
        IrAction::AcOnNoEco => rn02_state(),
        IrAction::AcOff => rn02_short(0xB2, 0xDE, 0x07),
        IrAction::OnModern => modern_midea(MIDEA_MODERN_ON_COOL_26_AUTO),
        IrAction::OnModernEco => {
            let mut pattern = modern_midea(MIDEA_MODERN_ON_COOL_26_AUTO);
            pattern.append(&modern_midea(MIDEA_MODERN_ECO_TOGGLE), 20_000);
            pattern
        }
        IrAction::OnRn02 => rn02_state(),
        IrAction::OnRn02Eco => {
            let mut pattern = rn02_state();
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

fn rn02_state() -> PulsePattern {
    let mut pattern = PulsePattern::new();
    rn02_block(
        &mut pattern,
        [
            (0xB2, true),
            (!0xB2, true),
            (0xFD, false),
            (!0xFD, false),
            (0x0B, false),
            (!0x0B, false),
        ],
    );
    rn02_block(
        &mut pattern,
        [
            (0xB2, true),
            (!0xB2, true),
            (0xFD, false),
            (!0xFD, false),
            (0x0B, false),
            (!0x0B, false),
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
        let data = encode_spi(&pattern, self.sample_hz)?;
        transmit_spi(&self.device, &data, self.sample_hz, self.bits_per_word)?;
        let definition = action.definition();
        Ok(IrTransmission {
            action: definition.id.to_owned(),
            label: definition.label.to_owned(),
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

    #[test]
    fn exposes_only_fixed_air_conditioner_actions() {
        assert!(IrAction::parse("ac-on").is_ok());
        assert!(IrAction::parse("ac-off").is_ok());
        assert!(IrAction::parse("on-modern").is_ok());
        assert!(IrAction::parse("on-rn02-eco").is_ok());
        assert!(IrAction::parse("raw").is_err());
    }

    #[test]
    fn builds_bounded_midea_patterns() {
        for action in [
            IrAction::AcOn,
            IrAction::AcOnNoEco,
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
