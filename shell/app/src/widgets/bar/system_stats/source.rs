use std::{
    ffi::CString,
    fs, io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    time::Duration,
};

use shell_core::source::{
    Observable,
    rx::{Observable as _, ObservableFactory as _, Shared},
};

use super::{DiskStats, SysStatsView};

const DISK_DEVICE: &str = "/dev/mapper/ubuntu--vg-ubuntu--lv";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CpuSample {
    idle: u64,
    total: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SystemSample {
    cpu: CpuSample,
    ram: u8,
    disk: DiskStats,
}

pub(super) fn sys_stats() -> Observable<SysStatsView> {
    let initial = read_system_sample()
        .map(|sample| SysStatsView {
            cpu: 0,
            ram: sample.ram,
            disk: sample.disk,
        })
        .unwrap_or_default();

    Shared::<()>::interval(Duration::from_secs(3))
        .start_with(vec![0])
        .filter_map(|_| read_system_sample().ok())
        .pairwise()
        .map(|(previous, current)| SysStatsView {
            cpu: cpu_percent(previous.cpu, current.cpu),
            ram: current.ram,
            disk: current.disk,
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
        disk: read_disk_stats().unwrap_or_default(),
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

fn read_disk_stats() -> Result<DiskStats, String> {
    let mounts = fs::read_to_string("/proc/mounts")
        .map_err(|error| format!("failed to read /proc/mounts: {error}"))?;
    let mountpoint = mountpoint_for_device(&mounts, DISK_DEVICE)
        .ok_or_else(|| format!("missing mount for {DISK_DEVICE} in /proc/mounts"))?;

    disk_stats(&mountpoint)
}

fn mountpoint_for_device(mounts: &str, device: &str) -> Option<PathBuf> {
    mounts.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let source = fields.next()?;
        let mountpoint = fields.next()?;
        (source == device).then(|| PathBuf::from(decode_mount_path(mountpoint)))
    })
}

fn decode_mount_path(path: &str) -> String {
    let mut output = String::new();
    let mut chars = path.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        let mut octal = String::new();
        for _ in 0..3 {
            match chars.peek().copied() {
                Some(digit @ '0'..='7') => {
                    octal.push(digit);
                    chars.next();
                }
                _ => break,
            }
        }

        if octal.len() == 3
            && let Ok(byte) = u8::from_str_radix(&octal, 8)
        {
            output.push(char::from(byte));
        } else {
            output.push('\\');
            output.push_str(&octal);
        }
    }

    output
}

fn disk_stats(path: &Path) -> Result<DiskStats, String> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| format!("mount path contains nul byte: {}", path.display()))?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return Err(format!(
            "failed to stat disk usage: {}",
            io::Error::last_os_error()
        ));
    }

    let stats = unsafe { stats.assume_init() };
    Ok(disk_stats_from_blocks(
        stats.f_blocks,
        stats.f_bfree,
        u64::try_from(stats.f_frsize).unwrap_or_default(),
    ))
}

fn disk_stats_from_blocks(
    total: libc::fsblkcnt_t,
    free: libc::fsblkcnt_t,
    block_size: u64,
) -> DiskStats {
    if total == 0 || block_size == 0 {
        return DiskStats::default();
    }

    let used = total.saturating_sub(free);
    DiskStats {
        percent: (((used as f64 / total as f64) * 100.0)
            .round()
            .clamp(0.0, 100.0) as u8),
        used: (used as u64).saturating_mul(block_size),
        free: (free as u64).saturating_mul(block_size),
        total: (total as u64).saturating_mul(block_size),
    }
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{disk_stats_from_blocks, mountpoint_for_device};

    #[test]
    fn mountpoint_for_device_decodes_proc_mount_escapes() {
        let mounts = "\
/dev/sda1 /boot ext4 rw 0 0
/dev/mapper/ubuntu--vg-ubuntu--lv /home/vafu/My\\040Disk ext4 rw 0 0
";

        assert_eq!(
            mountpoint_for_device(mounts, "/dev/mapper/ubuntu--vg-ubuntu--lv"),
            Some(PathBuf::from("/home/vafu/My Disk"))
        );
    }

    #[test]
    fn mountpoint_for_device_ignores_other_mounts() {
        assert_eq!(
            mountpoint_for_device("/dev/sda1 / ext4 rw 0 0", "/dev/dm-0"),
            None
        );
    }

    #[test]
    fn disk_stats_from_blocks_handles_zero_rounding_and_bytes() {
        assert_eq!(disk_stats_from_blocks(0, 0, 4096).percent, 0);

        let stats = disk_stats_from_blocks(10, 3, 4096);
        assert_eq!(stats.percent, 70);
        assert_eq!(stats.used, 7 * 4096);
        assert_eq!(stats.free, 3 * 4096);
        assert_eq!(stats.total, 10 * 4096);

        assert_eq!(disk_stats_from_blocks(3, 1, 4096).percent, 67);
    }
}
