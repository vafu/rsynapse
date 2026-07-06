use std::{cell::RefCell, path::PathBuf, process::Command};

use shell_core::gtk::{self, gio, prelude::*};

const MATERIAL_ICON_THEME: &str = "Material";
const INTERFACE_SCHEMA: &str = "org.gnome.desktop.interface";
const DEFAULT_LIGHT_THEME: &str = "kanso";
const LIGHT_SCHEME: &str = "prefer-light";
const DARK_SCHEME: &str = "prefer-dark";

pub(crate) fn prepare_theme() {
    prepare_desktop_theme();
    prepare_accent_sync();
    sync_color_scheme();
    sync_accent_color();
    prepare_icons();
}

pub(crate) fn toggle_color_scheme() -> Result<(), String> {
    let settings = interface_settings();
    let next = toggled_color_scheme(settings.string("color-scheme").as_str());
    settings
        .set_string("color-scheme", next)
        .map_err(|error| format!("set color-scheme to {next}: {error}"))?;
    gio::Settings::sync();
    update_gtk_theme(&settings, next)
}

pub(crate) fn set_frost_mode(frosted: bool) -> Result<(), String> {
    let settings = interface_settings();
    let current_theme = settings.string("gtk-theme");
    let color_scheme = settings.string("color-scheme");
    let theme = theme_for_frost_mode(current_theme.as_str(), color_scheme.as_str(), frosted);
    settings
        .set_string("gtk-theme", &theme)
        .map_err(|error| format!("set gtk-theme to {theme}: {error}"))?;
    gio::Settings::sync();
    Ok(())
}

fn prepare_icons() {
    if let Some(display) = gtk::gdk::Display::default() {
        let icon_theme = gtk::IconTheme::for_display(&display);
        icon_theme.add_search_path(icon_search_path());
        icon_theme.set_theme_name(Some(MATERIAL_ICON_THEME));
    }

    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_icon_theme_name(Some(MATERIAL_ICON_THEME));
    }
}

fn prepare_desktop_theme() {
    let settings = interface_settings();
    settings.connect_changed(Some("color-scheme"), |settings, _| {
        if let Err(error) = update_gtk_theme(settings, settings.string("color-scheme").as_str()) {
            eprintln!("[theme] failed to update GTK theme: {error}");
        }
    });
    SETTINGS.with(|cell| {
        *cell.borrow_mut() = Some(settings);
    });
}

fn prepare_accent_sync() {
    let settings = interface_settings();
    settings.connect_changed(Some("accent-color"), |settings, _| {
        if let Err(error) = sync_accent(settings.string("accent-color").as_str()) {
            eprintln!("[theme] failed to sync accent color: {error}");
        }
    });
    ACCENT_SETTINGS.with(|cell| {
        *cell.borrow_mut() = Some(settings);
    });
}

fn sync_color_scheme() {
    let settings = interface_settings();
    if let Err(error) = update_gtk_theme(&settings, settings.string("color-scheme").as_str()) {
        eprintln!("[theme] failed to sync GTK theme: {error}");
    }
}

fn sync_accent_color() {
    let settings = interface_settings();
    if let Err(error) = sync_accent(settings.string("accent-color").as_str()) {
        eprintln!("[theme] failed to sync accent color: {error}");
    }
}

fn update_gtk_theme(settings: &gio::Settings, color_scheme: &str) -> Result<(), String> {
    let current_theme = settings.string("gtk-theme");
    let theme = theme_for_scheme(current_theme.as_str(), color_scheme);
    settings
        .set_string("gtk-theme", &theme)
        .map_err(|error| format!("set gtk-theme to {theme}: {error}"))?;
    gio::Settings::sync();
    Ok(())
}

fn sync_accent(color: &str) -> Result<(), String> {
    let script = config_home().join("ags/scripts/sync_accent.sh");
    if !script.exists() {
        return Ok(());
    }
    let status = Command::new("bash")
        .arg(&script)
        .arg(color)
        .status()
        .map_err(|error| format!("run {}: {error}", script.display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} exited with {status}", script.display()))
    }
}

fn theme_for_scheme(current_theme: &str, color_scheme: &str) -> String {
    theme_for_frost_mode(
        current_theme,
        color_scheme,
        current_theme.ends_with("-frosted"),
    )
}

fn theme_for_frost_mode(current_theme: &str, color_scheme: &str, frosted: bool) -> String {
    let base = theme_base(current_theme);

    if frosted {
        if color_scheme == DARK_SCHEME {
            format!("{base}-dark-frosted")
        } else {
            format!("{base}-light-frosted")
        }
    } else if color_scheme == DARK_SCHEME {
        format!("{base}-dark")
    } else {
        base.to_string()
    }
}

fn theme_base(current_theme: &str) -> &str {
    current_theme
        .strip_suffix("-frosted")
        .map(frosted_theme_base)
        .unwrap_or_else(|| non_frosted_theme_base(current_theme))
}

fn frosted_theme_base(theme: &str) -> &str {
    theme
        .strip_suffix("-dark")
        .or_else(|| theme.strip_suffix("-light"))
        .filter(|base| !base.is_empty())
        .or_else(|| (!theme.is_empty() && theme != "-dark").then_some(theme))
        .unwrap_or(DEFAULT_LIGHT_THEME)
}

fn non_frosted_theme_base(current_theme: &str) -> &str {
    current_theme
        .strip_suffix("-dark")
        .filter(|base| !base.is_empty())
        .or_else(|| {
            (!current_theme.is_empty() && current_theme != "-dark").then_some(current_theme)
        })
        .unwrap_or(DEFAULT_LIGHT_THEME)
}

fn toggled_color_scheme(current: &str) -> &'static str {
    if current == LIGHT_SCHEME {
        DARK_SCHEME
    } else {
        LIGHT_SCHEME
    }
}

fn interface_settings() -> gio::Settings {
    gio::Settings::new(INTERFACE_SCHEMA)
}

fn icon_search_path() -> PathBuf {
    data_home().join("icons")
}

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from(".local/share"))
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
}

thread_local! {
    static SETTINGS: RefCell<Option<gio::Settings>> = const { RefCell::new(None) };
    static ACCENT_SETTINGS: RefCell<Option<gio::Settings>> = const { RefCell::new(None) };
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_LIGHT_THEME, theme_for_frost_mode, theme_for_scheme, toggled_color_scheme,
    };

    #[test]
    fn toggles_color_scheme_like_ags() {
        assert_eq!(toggled_color_scheme("prefer-light"), "prefer-dark");
        assert_eq!(toggled_color_scheme("prefer-dark"), "prefer-light");
        assert_eq!(toggled_color_scheme("default"), "prefer-light");
    }

    #[test]
    fn derives_gtk_theme_name_from_scheme() {
        assert_eq!(theme_for_scheme("Adwaita", "prefer-dark"), "Adwaita-dark");
        assert_eq!(theme_for_scheme("Adwaita-dark", "prefer-light"), "Adwaita");
        assert_eq!(
            theme_for_scheme("kanso-light-frosted", "prefer-dark"),
            "kanso-dark-frosted"
        );
        assert_eq!(
            theme_for_scheme("kanso-dark-frosted", "prefer-light"),
            "kanso-light-frosted"
        );
        assert_eq!(theme_for_scheme("", "prefer-dark"), "kanso-dark");
        assert_eq!(theme_for_scheme("-dark", "prefer-dark"), "kanso-dark");
        assert_eq!(
            theme_for_scheme("-dark", "prefer-light"),
            DEFAULT_LIGHT_THEME
        );
    }

    #[test]
    fn derives_gtk_theme_name_from_frost_mode() {
        assert_eq!(
            theme_for_frost_mode("kanso-dark-frosted", "prefer-dark", false),
            "kanso-dark"
        );
        assert_eq!(
            theme_for_frost_mode("kanso-light-frosted", "prefer-light", false),
            DEFAULT_LIGHT_THEME
        );
        assert_eq!(
            theme_for_frost_mode("kanso-dark", "prefer-dark", true),
            "kanso-dark-frosted"
        );
        assert_eq!(
            theme_for_frost_mode("kanso", "prefer-light", true),
            "kanso-light-frosted"
        );
    }
}
