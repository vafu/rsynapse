//! Backlight sysfs adapter.
//!
//! `/sys/class/backlight` is watched through `notify`; no polling is needed for
//! brightness changes.
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{Observable, rx::Observable as _};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

const BACKLIGHT_ROOT: &str = "/sys/class/backlight";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Backlight {
    pub visible: bool,
    pub percent: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BacklightDevice {
    name: String,
    dir: PathBuf,
    brightness_path: PathBuf,
    actual_brightness_path: PathBuf,
    max_brightness_path: PathBuf,
}

pub fn status() -> Observable<Backlight> {
    crate::shared_by_key("rsynapse.brightness-status", BACKLIGHT_ROOT, || {
        crate::from_task(|sender| async move {
            if let Err(error) = watch_brightness(sender.clone()).await {
                let _ = sender.send(Err(error)).await;
            }
        })
        .distinct_until_changed()
        .box_it()
    })
}

async fn watch_brightness(
    sender: async_channel::Sender<Result<Backlight, String>>,
) -> Result<(), String> {
    let device = find_backlight_device(Path::new(BACKLIGHT_ROOT))?;
    let mut latest = None;
    emit_brightness_if_changed(&sender, &device, &mut latest).await?;

    let (event_sender, event_receiver) = async_channel::bounded(1);
    let mut watcher = brightness_watcher(event_sender)?;
    watch_device(&mut watcher, &device)?;

    while event_receiver.recv().await.is_ok() {
        emit_brightness_if_changed(&sender, &device, &mut latest).await?;
    }

    Ok(())
}

async fn emit_brightness_if_changed(
    sender: &async_channel::Sender<Result<Backlight, String>>,
    device: &BacklightDevice,
    latest: &mut Option<Backlight>,
) -> Result<(), String> {
    let view = read_brightness_view(device)?;
    if latest.as_ref() != Some(&view) {
        *latest = Some(view.clone());
        sender
            .send(Ok(view))
            .await
            .map_err(|_| "brightness subscriber dropped".to_owned())?;
    }
    Ok(())
}

fn brightness_watcher(events: async_channel::Sender<()>) -> Result<RecommendedWatcher, String> {
    RecommendedWatcher::new(
        move |result: Result<notify::Event, notify::Error>| {
            if let Err(error) = result {
                eprintln!("[brightness-source] backlight watch error: {error}");
                return;
            }
            let _ = events.try_send(());
        },
        Config::default(),
    )
    .map_err(|error| format!("create backlight watcher: {error}"))
}

fn watch_device(watcher: &mut RecommendedWatcher, device: &BacklightDevice) -> Result<(), String> {
    let mut watched = 0usize;
    let mut last_error = None;
    for path in device.watch_paths() {
        match watcher.watch(path.as_path(), RecursiveMode::NonRecursive) {
            Ok(()) => watched += 1,
            Err(error) => {
                last_error = Some(format!("watch {}: {error}", path.display()));
            }
        }
    }

    if watched == 0 {
        Err(last_error.unwrap_or_else(|| {
            format!(
                "no watchable backlight paths found for {}",
                device.dir.display()
            )
        }))
    } else {
        Ok(())
    }
}

fn find_backlight_device(root: &Path) -> Result<BacklightDevice, String> {
    let mut devices = fs::read_dir(root)
        .map_err(|error| format!("read {}: {error}", root.display()))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| BacklightDevice::from_entry_path(entry.path()))
        .collect::<Vec<_>>();
    devices.sort_by(|left, right| left.name.cmp(&right.name));
    devices
        .into_iter()
        .next()
        .ok_or_else(|| format!("no backlight devices under {}", root.display()))
}

fn read_brightness_view(device: &BacklightDevice) -> Result<Backlight, String> {
    let current =
        read_u64(&device.actual_brightness_path).or_else(|_| read_u64(&device.brightness_path))?;
    let max = read_u64(&device.max_brightness_path)?;
    let percent = brightness_percent(current, max);

    Ok(Backlight {
        visible: max > 0,
        percent,
    })
}

fn read_u64(path: &Path) -> Result<u64, String> {
    fs::read_to_string(path)
        .map_err(|error| format!("read {}: {error}", path.display()))?
        .trim()
        .parse::<u64>()
        .map_err(|error| format!("parse {}: {error}", path.display()))
}

fn brightness_percent(current: u64, max: u64) -> u8 {
    if max == 0 {
        return 0;
    }

    ((current as f64 / max as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8
}

impl BacklightDevice {
    fn from_entry_path(entry_path: PathBuf) -> Option<Self> {
        let name = entry_path.file_name()?.to_string_lossy().into_owned();
        let dir = fs::canonicalize(&entry_path).unwrap_or(entry_path);
        let brightness_path = dir.join("brightness");
        let actual_brightness_path = dir.join("actual_brightness");
        let max_brightness_path = dir.join("max_brightness");

        (brightness_path.is_file() && max_brightness_path.is_file()).then_some(Self {
            name,
            dir,
            brightness_path,
            actual_brightness_path,
            max_brightness_path,
        })
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![
            self.dir.clone(),
            self.brightness_path.clone(),
            self.max_brightness_path.clone(),
        ];
        if self.actual_brightness_path.is_file() {
            paths.push(self.actual_brightness_path.clone());
        }
        paths.sort();
        paths.dedup();
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn brightness_percent_rounds_and_clamps() {
        assert_eq!(brightness_percent(0, 400), 0);
        assert_eq!(brightness_percent(201, 400), 50);
        assert_eq!(brightness_percent(400, 400), 100);
        assert_eq!(brightness_percent(12, 0), 0);
    }

    #[test]
    fn finds_first_backlight_device_and_reads_percent() {
        let root = temp_root();
        write_backlight_device(&root, "z_backlight", 360, 400);
        write_backlight_device(&root, "a_backlight", 180, 400);
        let device = find_backlight_device(&root).unwrap();
        let view = read_brightness_view(&device).unwrap();
        assert_eq!(device.name, "a_backlight");
        assert!(view.visible);
        assert_eq!(view.percent, 45);
        let _ = fs::remove_dir_all(root);
    }

    fn write_backlight_device(root: &PathBuf, name: &str, brightness: u64, max: u64) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("brightness"), brightness.to_string()).unwrap();
        fs::write(dir.join("actual_brightness"), brightness.to_string()).unwrap();
        fs::write(dir.join("max_brightness"), max.to_string()).unwrap();
    }

    fn temp_root() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rsynapse-brightness-test-{}-{suffix}",
            std::process::id()
        ))
    }
}
