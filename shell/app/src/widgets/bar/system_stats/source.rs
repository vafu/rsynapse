use std::{fs, time::Duration};

use shell_core::source::{
    Observable,
    rx::{Observable as _, ObservableFactory as _, Shared},
};

use super::SysStatsView;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CpuSample {
    idle: u64,
    total: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SystemSample {
    cpu: CpuSample,
    ram: u8,
}

pub(super) fn sys_stats() -> Observable<SysStatsView> {
    let initial = read_system_sample()
        .map(|sample| SysStatsView {
            cpu: 0,
            ram: sample.ram,
        })
        .unwrap_or_default();

    Shared::<()>::interval(Duration::from_secs(3))
        .start_with(vec![0])
        .filter_map(|_| read_system_sample().ok())
        .pairwise()
        .map(|(previous, current)| SysStatsView {
            cpu: cpu_percent(previous.cpu, current.cpu),
            ram: current.ram,
        })
        .start_with(vec![initial])
        .map_err(|error| error.to_string())
        .distinct_until_changed()
        .box_it()
}

fn read_system_sample() -> Result<SystemSample, String> {
    Ok(SystemSample {
        cpu: read_cpu_sample()?,
        ram: read_ram_percent()?,
    })
}

fn read_cpu_sample() -> Result<CpuSample, String> {
    let stat = fs::read_to_string("/proc/stat")
        .map_err(|error| format!("failed to read /proc/stat: {error}"))?;
    let cpu = stat
        .lines()
        .find(|line| line.starts_with("cpu "))
        .ok_or_else(|| "missing aggregate cpu line in /proc/stat".to_owned())?;
    let values = cpu
        .split_whitespace()
        .skip(1)
        .map(|value| value.parse::<u64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to parse /proc/stat cpu line: {error}"))?;
    if values.len() < 4 {
        return Err("aggregate cpu line has too few fields".to_owned());
    }

    let idle = values[3] + values.get(4).copied().unwrap_or_default();
    let total = values.iter().sum();
    Ok(CpuSample { idle, total })
}

fn read_ram_percent() -> Result<u8, String> {
    let meminfo = fs::read_to_string("/proc/meminfo")
        .map_err(|error| format!("failed to read /proc/meminfo: {error}"))?;
    let mut total = None;
    let mut available = None;

    for line in meminfo.lines() {
        if let Some(value) = meminfo_kib(line, "MemTotal:") {
            total = Some(value);
        } else if let Some(value) = meminfo_kib(line, "MemAvailable:") {
            available = Some(value);
        }
    }

    let total = total.ok_or_else(|| "missing MemTotal in /proc/meminfo".to_owned())?;
    let available = available.ok_or_else(|| "missing MemAvailable in /proc/meminfo".to_owned())?;
    if total == 0 {
        return Ok(0);
    }

    let used = total.saturating_sub(available);
    Ok(((used as f64 / total as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8)
}

fn meminfo_kib(line: &str, key: &str) -> Option<u64> {
    line.strip_prefix(key)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn cpu_percent(previous: CpuSample, current: CpuSample) -> u8 {
    let total = current.total.saturating_sub(previous.total);
    if total == 0 {
        return 0;
    }

    let idle = current.idle.saturating_sub(previous.idle);
    (((total.saturating_sub(idle)) as f64 / total as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8
}
