#[cfg(test)]
mod test;

use std::{
    fs,
    path::{Path, PathBuf},
};

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use shell_core::source::{self, Observable, rx::Observable as _};

const BACKLIGHT_ROOT: &str = "/sys/class/backlight";
const BRIGHTNESS_ICON: &str = "display-brightness-symbolic";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct BrightnessView {
    pub(super) visible: bool,
    pub(super) icon: &'static str,
    pub(super) percent: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BacklightDevice {
    name: String,
    dir: PathBuf,
    brightness_path: PathBuf,
    actual_brightness_path: PathBuf,
    max_brightness_path: PathBuf,
}

pub(super) fn brightness_status() -> Observable<BrightnessView> {
    source::shared_by_key("rsynapse.brightness-status", BACKLIGHT_ROOT, || {
        source::from_task(|sender| async move {
            if let Err(error) = watch_brightness(sender.clone()).await {
                let _ = sender.send(Err(error)).await;
            }
        })
        .distinct_until_changed()
        .box_it()
    })
}

async fn watch_brightness(
    sender: async_channel::Sender<Result<BrightnessView, String>>,
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
    sender: &async_channel::Sender<Result<BrightnessView, String>>,
    device: &BacklightDevice,
    latest: &mut Option<BrightnessView>,
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

fn read_brightness_view(device: &BacklightDevice) -> Result<BrightnessView, String> {
    let current =
        read_u64(&device.actual_brightness_path).or_else(|_| read_u64(&device.brightness_path))?;
    let max = read_u64(&device.max_brightness_path)?;
    let percent = brightness_percent(current, max);

    Ok(BrightnessView {
        visible: max > 0,
        icon: BRIGHTNESS_ICON,
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
