use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    thread,
};

use shell_core::gtk;

const DEFAULT_SIZE: u16 = 24;
const DEFAULT_STYLE: &str = "outlined";
const DEFAULT_WEIGHT: u16 = 400;
const DEFAULT_GRAD: &str = "0";
const DEFAULT_FILL: bool = true;
const MATERIAL_THEME: &str = "Material";
const MATERIAL_REPO: &str =
    "https://raw.githubusercontent.com/google/material-design-icons/refs/heads/master/symbols/web";

pub(super) fn icon_name(icon: &str) -> String {
    let name = resolved_icon_name(icon);
    if icon_requires_fetch(icon, &name) {
        fetch_icon_once(icon.to_owned(), name.clone());
    }
    name
}

fn resolved_icon_name(icon: &str) -> String {
    if icon.is_empty() || icon.ends_with("-symbolic") {
        icon.to_owned()
    } else {
        material_icon_name(icon)
    }
}

fn material_icon_name(icon: &str) -> String {
    format!("{}-{DEFAULT_STYLE}-symbolic", material_resource_name(icon))
}

fn material_resource_name(icon: &str) -> String {
    let mut resource = format!("{icon}_");

    if DEFAULT_WEIGHT != 400 {
        resource.push_str(&format!("wght{DEFAULT_WEIGHT}"));
    }
    if DEFAULT_GRAD != "0" {
        resource.push_str(&format!("grad{DEFAULT_GRAD}"));
    }
    if DEFAULT_FILL {
        resource.push_str("fill1");
    }
    if !resource.ends_with('_') {
        resource.push('_');
    }
    resource.push_str(&format!("{DEFAULT_SIZE}px"));

    resource
}

fn material_icon_exists(icon_name: &str) -> bool {
    if material_icon_existing_cache()
        .lock()
        .expect("material icon existing cache is poisoned")
        .contains(icon_name)
    {
        return true;
    }
    if material_icon_missing_cache()
        .lock()
        .expect("material icon missing cache is poisoned")
        .contains(icon_name)
    {
        return false;
    }

    let exists = material_icon_file(icon_name).exists();
    let cache = if exists {
        material_icon_existing_cache()
    } else {
        material_icon_missing_cache()
    };
    cache
        .lock()
        .expect("material icon existence cache is poisoned")
        .insert(icon_name.to_owned());
    exists
}

fn icon_requires_fetch(icon: &str, icon_name: &str) -> bool {
    !icon.is_empty() && !icon.ends_with("-symbolic") && !material_icon_exists(icon_name)
}

fn fetch_icon_once(icon: String, icon_name: String) {
    static REQUESTED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

    let requested = REQUESTED.get_or_init(|| Mutex::new(HashSet::new()));
    if !requested
        .lock()
        .expect("material icon requested cache is poisoned")
        .insert(icon_name.clone())
    {
        return;
    }

    fetch_icon(icon, icon_name, None);
}

fn fetch_icon(
    icon: String,
    icon_name: String,
    sender: Option<async_channel::Sender<Result<PathBuf, String>>>,
) {
    thread::spawn(move || {
        let result = fetch_material_icon(&icon, &icon_name);
        if result.is_ok() {
            gtk::glib::MainContext::default().invoke(refresh_icon_theme);
        }
        if let Some(sender) = sender {
            let _ = sender.send_blocking(result);
        } else if let Err(error) = result {
            eprintln!("[material-icon] {icon_name}: {error}");
        }
    });
}

fn fetch_material_icon(icon: &str, icon_name: &str) -> Result<PathBuf, String> {
    let icon_file = material_icon_file(icon_name);
    if icon_file.exists() {
        remember_material_icon_exists(icon_name);
        return Ok(icon_file);
    }

    let resource_name = material_resource_name(icon);
    let url = format!("{MATERIAL_REPO}/{icon}/materialsymbols{DEFAULT_STYLE}/{resource_name}.svg");
    let output = Command::new("curl")
        .args(["-fsSL", url.as_str()])
        .output()
        .map_err(|error| format!("failed to start curl: {error}"))?;
    if !output.status.success() {
        return Err(format!("download failed with status {}", output.status));
    }

    let svg = normalize_svg(String::from_utf8(output.stdout).map_err(|error| error.to_string())?);
    let parent = icon_file
        .parent()
        .ok_or_else(|| format!("invalid icon path {}", icon_file.display()))?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    fs::write(&icon_file, svg).map_err(|error| error.to_string())?;
    update_icon_cache(material_theme_dir());
    remember_material_icon_exists(icon_name);

    Ok(icon_file)
}

fn remember_material_icon_exists(icon_name: &str) {
    material_icon_existing_cache()
        .lock()
        .expect("material icon existing cache is poisoned")
        .insert(icon_name.to_owned());
    material_icon_missing_cache()
        .lock()
        .expect("material icon missing cache is poisoned")
        .remove(icon_name);
}

fn material_icon_existing_cache() -> &'static Mutex<HashSet<String>> {
    static EXISTING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    EXISTING.get_or_init(|| Mutex::new(HashSet::new()))
}

fn material_icon_missing_cache() -> &'static Mutex<HashSet<String>> {
    static MISSING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    MISSING.get_or_init(|| Mutex::new(HashSet::new()))
}

fn normalize_svg(svg: String) -> String {
    if !svg.contains("viewBox=\"0 -960 960 960\"") {
        return svg;
    }

    svg.replace("viewBox=\"0 -960 960 960\"", "viewBox=\"0 0 960 960\"")
        .replace("<path ", "<path transform=\"translate(0, 960)\" ")
}

fn update_icon_cache(theme_dir: PathBuf) {
    if let Err(error) = Command::new("gtk-update-icon-cache")
        .arg("-f")
        .arg(theme_dir)
        .status()
    {
        eprintln!("[material-icon] failed to update icon cache: {error}");
    }
}

fn refresh_icon_theme() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let icon_theme = gtk::IconTheme::for_display(&display);
    icon_theme.add_search_path(data_home().join("icons"));
    icon_theme.set_theme_name(Some(MATERIAL_THEME));
}

fn material_icon_file(icon_name: &str) -> PathBuf {
    material_icon_dir().join(format!("{icon_name}.svg"))
}

fn material_icon_dir() -> PathBuf {
    material_theme_dir().join("symbolic")
}

fn material_theme_dir() -> PathBuf {
    data_home().join("icons").join(MATERIAL_THEME)
}

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from(".local/share"))
}
