use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

pub(crate) fn icon_for_app_id(app_id: &str) -> String {
    static CACHE: OnceLock<Mutex<DesktopIconCache>> = OnceLock::new();

    let cache = CACHE.get_or_init(|| Mutex::new(DesktopIconCache::load()));
    cache
        .lock()
        .expect("desktop icon cache lock is poisoned")
        .icon_for_app_id(app_id)
        .unwrap_or_default()
}

#[derive(Default)]
struct DesktopIconCache {
    entries: Vec<DesktopIconEntry>,
    queries: HashMap<String, Option<String>>,
}

impl DesktopIconCache {
    fn load() -> Self {
        let mut entries = Vec::new();
        for directory in application_dirs() {
            collect_desktop_entries(&directory, &mut entries);
        }
        Self {
            entries,
            queries: HashMap::new(),
        }
    }

    fn icon_for_app_id(&mut self, app_id: &str) -> Option<String> {
        let normalized = normalize_app_id(app_id);
        if normalized.is_empty() {
            return None;
        }
        if let Some(icon) = self.queries.get(&normalized) {
            return icon.clone();
        }

        let icon = self
            .entries
            .iter()
            .find(|entry| entry.matches(&normalized))
            .and_then(|entry| entry.icon.clone());
        self.queries.insert(normalized, icon.clone());
        icon
    }
}

struct DesktopIconEntry {
    desktop_key: String,
    startup_wm_class_key: Option<String>,
    icon: Option<String>,
}

impl DesktopIconEntry {
    fn matches(&self, app_id: &str) -> bool {
        self.desktop_key.contains(app_id)
            || self
                .startup_wm_class_key
                .as_ref()
                .is_some_and(|value| value.contains(app_id))
    }
}

fn collect_desktop_entries(directory: &Path, entries: &mut Vec<DesktopIconEntry>) {
    let Ok(files) = fs::read_dir(directory) else {
        return;
    };

    for file in files.flatten() {
        let path = file.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("desktop") {
            continue;
        }

        if let Some(entry) = read_desktop_entry(&path) {
            entries.push(entry);
        }
    }
}

fn read_desktop_entry(path: &Path) -> Option<DesktopIconEntry> {
    let desktop_id = path.file_name()?.to_str()?.to_owned();
    let content = fs::read_to_string(path).ok()?;
    let mut in_desktop_entry = false;
    let mut startup_wm_class = None;
    let mut icon = None;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop_entry {
            continue;
        }

        if let Some(value) = line.strip_prefix("StartupWMClass=") {
            startup_wm_class = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("Icon=") {
            icon = Some(value.to_owned());
        }

        if startup_wm_class.is_some() && icon.is_some() {
            break;
        }
    }

    Some(DesktopIconEntry {
        desktop_key: normalize_app_id(&desktop_id),
        startup_wm_class_key: startup_wm_class.as_deref().map(normalize_app_id),
        icon,
    })
}

fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(data_home).join("applications"));
    } else if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }

    let data_dirs = std::env::var_os("XDG_DATA_DIRS")
        .map(|value| {
            std::env::split_paths(&value)
                .map(|path| path.join("applications"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share/applications"),
                PathBuf::from("/usr/share/applications"),
            ]
        });
    dirs.extend(data_dirs);
    dirs
}

fn normalize_app_id(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(".desktop")
        .to_ascii_lowercase()
}
