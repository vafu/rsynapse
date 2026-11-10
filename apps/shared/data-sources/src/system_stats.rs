//! /proc-based CPU/RAM/disk statistics adapter.
//!
//! `/proc/stat`, `/proc/meminfo`, `/proc/mounts`, and `/proc/diskstats` are
//! polled on a fixed interval; Linux has no change-notification interface for
//! these values.
use std::{
    ffi::CString,
    fs, io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::{
    Observable,
    rx::{Observable as _, ObservableFactory as _, Shared},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiskStats {
    pub percent: u8,
    pub busy: u8,
    pub used: u64,
    pub free: u64,
    pub total: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SysStatsView {
    pub cpu: u8,
    pub ram: u8,
    pub disk: DiskStats,
}

const DISK_DEVICE_ENV: &str = "RSYNAPSE_DISK_DEVICE";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CpuSample {
    idle: u64,
    total: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SystemSample {
    sampled_at: Instant,
    cpu: CpuSample,
    ram: u8,
    disk: DiskStats,
    disk_io: DiskIoSample,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct DiskIoSample {
    busy_ms: u64,
}

pub fn sys_stats() -> Observable<SysStatsView> {
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
        .map(|(previous, current)| {
            let mut disk = current.disk;
            disk.busy = disk_busy_percent(
                previous.disk_io,
                current.disk_io,
                previous.sampled_at,
                current.sampled_at,
            );
            SysStatsView {
                cpu: cpu_percent(previous.cpu, current.cpu),
                ram: current.ram,
                disk,
            }
        })
        .start_with(vec![initial])
        .map_err(|error| error.to_string())
        .distinct_until_changed()
        .box_it()
}

fn read_system_sample() -> Result<SystemSample, String> {
    Ok(SystemSample {
        sampled_at: Instant::now(),
        cpu: read_cpu_sample()?,
        ram: read_ram_percent()?,
        disk: read_disk_stats().unwrap_or_default(),
        disk_io: read_disk_io_sample().unwrap_or_default(),
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

    if let Some(device) = disk_device_override() {
        let mountpoint = mountpoint_for_device(&mounts, &device)
            .ok_or_else(|| format!("missing mount for {device} in /proc/mounts"))?;
        return disk_stats(&mountpoint);
    }

    if let Some(device) = root_mount_device(&mounts)
        && let Some(mountpoint) = mountpoint_for_device(&mounts, &device)
    {
        return disk_stats(&mountpoint);
    }

    disk_stats(Path::new("/"))
}

fn read_disk_io_sample() -> Result<DiskIoSample, String> {
    let device = match disk_device_override() {
        Some(device) => device,
        None => {
            let mounts = fs::read_to_string("/proc/mounts")
                .map_err(|error| format!("failed to read /proc/mounts: {error}"))?;
            root_mount_device(&mounts)
                .ok_or_else(|| "missing root mount in /proc/mounts".to_owned())?
        }
    };
    let device = diskstats_device_name(&device)?;
    let diskstats = fs::read_to_string("/proc/diskstats")
        .map_err(|error| format!("failed to read /proc/diskstats: {error}"))?;
    parse_disk_io_sample(&diskstats, device.as_str())
}

fn disk_device_override() -> Option<String> {
    std::env::var(DISK_DEVICE_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn root_mount_device(mounts: &str) -> Option<String> {
    mounts.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let source = fields.next()?;
        let mountpoint = fields.next()?;
        (decode_mount_path(mountpoint) == "/").then(|| source.to_owned())
    })
}

fn diskstats_device_name(device: &str) -> Result<String, String> {
    let path =
        fs::canonicalize(device).map_err(|error| format!("failed to resolve {device}: {error}"))?;
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| format!("failed to get diskstats device name for {}", path.display()))
}

fn parse_disk_io_sample(diskstats: &str, device: &str) -> Result<DiskIoSample, String> {
    for line in diskstats.lines() {
        let mut fields = line.split_whitespace();
        let _major = fields.next();
        let _minor = fields.next();
        if fields.next() != Some(device) {
            continue;
        }

        let values = fields
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("failed to parse /proc/diskstats for {device}: {error}"))?;
        let busy_ms = values
            .get(9)
            .copied()
            .ok_or_else(|| format!("/proc/diskstats row for {device} has too few fields"))?;
        return Ok(DiskIoSample { busy_ms });
    }

    Err(format!("missing /proc/diskstats row for {device}"))
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
        busy: 0,
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

fn disk_busy_percent(
    previous: DiskIoSample,
    current: DiskIoSample,
    previous_at: Instant,
    current_at: Instant,
) -> u8 {
    let elapsed_ms = current_at.duration_since(previous_at).as_millis();
    if elapsed_ms == 0 {
        return 0;
    }

    ((u128::from(current.busy_ms.saturating_sub(previous.busy_ms)) * 100 / elapsed_ms).min(100))
        as u8
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };

    use super::{
        DiskIoSample, disk_busy_percent, disk_stats_from_blocks, mountpoint_for_device,
        parse_disk_io_sample, root_mount_device,
    };

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
    fn root_mount_device_selects_the_root_mount() {
        let mounts = "\
/dev/sda1 /boot ext4 rw 0 0
/dev/mapper/ubuntu--vg-ubuntu--lv / ext4 rw 0 0
/dev/sdb1 /home ext4 rw 0 0
";

        assert_eq!(
            root_mount_device(mounts),
            Some("/dev/mapper/ubuntu--vg-ubuntu--lv".to_owned())
        );
    }

    #[test]
    fn root_mount_device_decodes_escaped_mountpoint() {
        let mounts = "/dev/sda1 /home/vafu/My\\040Disk ext4 rw 0 0\noverlay / ext4 rw 0 0\n";

        assert_eq!(root_mount_device(mounts), Some("overlay".to_owned()));
    }

    #[test]
    fn parse_disk_io_sample_reads_busy_ms() {
        let diskstats = "   8       0 sda 1 0 2 3 4 5 6 7 0 9 10 0 0 0 0 0\n 253       0 dm-0 1 0 2 3 4 5 6 7 0 1234 10 0 0 0 0 0\n";

        assert_eq!(
            parse_disk_io_sample(diskstats, "dm-0"),
            Ok(DiskIoSample { busy_ms: 1234 })
        );
    }

    #[test]
    fn disk_busy_percent_uses_elapsed_wall_time() {
        let start = Instant::now();
        let end = start + Duration::from_millis(2_000);

        assert_eq!(
            disk_busy_percent(
                DiskIoSample { busy_ms: 1_000 },
                DiskIoSample { busy_ms: 1_500 },
                start,
                end,
            ),
            25
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
