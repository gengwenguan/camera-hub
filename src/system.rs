use serde::Serialize;
use std::ffi::CString;
use std::fs;
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Clone, Copy)]
struct CpuTimes {
    total: u64,
    idle: u64,
}

pub struct SystemMonitor {
    disk_path: PathBuf,
    previous_cpu: Mutex<Option<CpuTimes>>,
}

#[derive(Serialize)]
pub struct SystemStatus {
    pub ok: bool,
    pub hostname: String,
    pub kernel: String,
    pub cpu: CpuStatus,
    pub load: LoadStatus,
    pub mem: MemoryStatus,
    pub process: ProcessStatus,
    pub disk: DiskStatus,
    pub uptime: DurationStatus,
    pub process_uptime: DurationStatus,
}

#[derive(Serialize)]
pub struct CpuStatus {
    pub valid: bool,
    pub percent: f64,
    pub cores: usize,
}

#[derive(Serialize)]
pub struct LoadStatus {
    pub valid: bool,
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

#[derive(Serialize)]
pub struct MemoryStatus {
    pub valid: bool,
    pub total_kb: u64,
    pub available_kb: u64,
}

#[derive(Serialize)]
pub struct ProcessStatus {
    pub valid: bool,
    pub rss_kb: u64,
    pub data_kb: u64,
}

#[derive(Serialize)]
pub struct DiskStatus {
    pub valid: bool,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Serialize)]
pub struct DurationStatus {
    pub valid: bool,
    pub seconds: u64,
}

impl SystemMonitor {
    pub fn new(disk_path: PathBuf) -> Self {
        Self {
            disk_path,
            previous_cpu: Mutex::new(None),
        }
    }

    pub fn sample(&self, process_uptime_seconds: u64) -> SystemStatus {
        let current_cpu = read_cpu_times();
        let cpu = {
            let mut previous = self
                .previous_cpu
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let percent = previous
                .zip(current_cpu)
                .and_then(|(before, current)| cpu_percent(before, current));
            if current_cpu.is_some() {
                *previous = current_cpu;
            }
            CpuStatus {
                valid: percent.is_some(),
                percent: percent.unwrap_or_default(),
                cores: cpu_cores(),
            }
        };
        let (load, mem, process, uptime) = (
            read_load(),
            read_memory(),
            read_process_memory(),
            read_uptime(),
        );
        SystemStatus {
            ok: true,
            hostname: read_trimmed("/etc/hostname"),
            kernel: read_trimmed("/proc/sys/kernel/osrelease"),
            cpu,
            load,
            mem,
            process,
            disk: read_disk(&self.disk_path),
            uptime,
            process_uptime: DurationStatus {
                valid: true,
                seconds: process_uptime_seconds,
            },
        }
    }
}

fn read_cpu_times() -> Option<CpuTimes> {
    let text = fs::read_to_string("/proc/stat").ok()?;
    parse_cpu_times(text.lines().next()?)
}

fn parse_cpu_times(line: &str) -> Option<CpuTimes> {
    let mut fields = line.split_whitespace();
    if fields.next()? != "cpu" {
        return None;
    }
    let values = fields
        .take(8)
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() < 4 {
        return None;
    }
    Some(CpuTimes {
        total: values.iter().sum(),
        idle: values[3] + values.get(4).copied().unwrap_or_default(),
    })
}

fn cpu_percent(before: CpuTimes, current: CpuTimes) -> Option<f64> {
    let total = current.total.checked_sub(before.total)?;
    let idle = current.idle.checked_sub(before.idle)?;
    (total > 0)
        .then(|| (100.0 * (total.saturating_sub(idle)) as f64 / total as f64).clamp(0.0, 100.0))
}

fn cpu_cores() -> usize {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .map(|text| {
            text.lines()
                .filter(|line| line.starts_with("processor"))
                .count()
        })
        .filter(|cores| *cores > 0)
        .or_else(|| std::thread::available_parallelism().ok().map(usize::from))
        .unwrap_or(1)
}

fn read_load() -> LoadStatus {
    let values = fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|text| {
            text.split_whitespace()
                .take(3)
                .map(str::parse::<f64>)
                .collect::<Result<Vec<_>, _>>()
                .ok()
        })
        .unwrap_or_default();
    LoadStatus {
        valid: values.len() == 3,
        one: values.first().copied().unwrap_or_default(),
        five: values.get(1).copied().unwrap_or_default(),
        fifteen: values.get(2).copied().unwrap_or_default(),
    }
}

fn read_memory() -> MemoryStatus {
    let text = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let total = parse_kb(&text, "MemTotal");
    let available = parse_kb(&text, "MemAvailable").or_else(|| parse_kb(&text, "MemFree"));
    MemoryStatus {
        valid: total.is_some() && available.is_some(),
        total_kb: total.unwrap_or_default(),
        available_kb: available.unwrap_or_default(),
    }
}

fn read_process_memory() -> ProcessStatus {
    let text = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let rss = parse_kb(&text, "VmRSS");
    let data = parse_kb(&text, "VmData");
    ProcessStatus {
        valid: rss.is_some() || data.is_some(),
        rss_kb: rss.unwrap_or_default(),
        data_kb: data.unwrap_or_default(),
    }
}

fn parse_kb(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name == key)
            .then(|| value.split_whitespace().next()?.parse::<u64>().ok())
            .flatten()
    })
}

fn read_disk(path: &PathBuf) -> DiskStatus {
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return DiskStatus::default();
    };
    let mut value = MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: path is a valid NUL-terminated string and statvfs initializes value on success.
    if unsafe { libc::statvfs(path.as_ptr(), value.as_mut_ptr()) } != 0 {
        return DiskStatus::default();
    }
    // SAFETY: statvfs returned success, so all fields are initialized.
    let value = unsafe { value.assume_init() };
    let block_size = if value.f_frsize > 0 {
        value.f_frsize
    } else {
        value.f_bsize
    } as u64;
    DiskStatus {
        valid: value.f_blocks > 0,
        total_bytes: value.f_blocks as u64 * block_size,
        available_bytes: value.f_bavail as u64 * block_size,
    }
}

impl Default for DiskStatus {
    fn default() -> Self {
        Self {
            valid: false,
            total_bytes: 0,
            available_bytes: 0,
        }
    }
}

fn read_uptime() -> DurationStatus {
    let seconds = fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|text| text.split_whitespace().next()?.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value as u64);
    DurationStatus {
        valid: seconds.is_some(),
        seconds: seconds.unwrap_or_default(),
    }
}

fn read_trimmed(path: &str) -> String {
    fs::read_to_string(path)
        .map(|text| text.trim().to_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux_cpu_and_memory_formats() {
        let cpu = parse_cpu_times("cpu  100 5 20 400 10 2 3 0 0 0").unwrap();
        assert_eq!(cpu.total, 540);
        assert_eq!(cpu.idle, 410);

        let memory = "MemTotal:       2048000 kB\nMemAvailable:   1024000 kB\n";
        assert_eq!(parse_kb(memory, "MemTotal"), Some(2_048_000));
        assert_eq!(parse_kb(memory, "MemAvailable"), Some(1_024_000));
    }

    #[test]
    fn calculates_cpu_usage_from_two_samples() {
        let before = CpuTimes {
            total: 1000,
            idle: 700,
        };
        let current = CpuTimes {
            total: 1100,
            idle: 740,
        };
        assert_eq!(cpu_percent(before, current), Some(60.0));
    }
}
