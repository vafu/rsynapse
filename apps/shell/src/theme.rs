use std::path::PathBuf;

use shell_core::gtk::{self, prelude::*};

const MATERIAL_ICON_THEME: &str = "Material";

// NOTE: no color-scheme / gtk-theme / accent management here on purpose.
// The external theme system (~/.config/theme: theme-set / theme-toggle /
// theme-accent) owns all of that. This module only sets the icon theme.
// (Previously this also synced accent-color via ags/scripts/sync_accent.sh
// and swapped niri/gtk themes — removed along with that script.)

pub(crate) fn prepare_theme() {
    prepare_icons();
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

fn icon_search_path() -> PathBuf {
    data_home().join("icons")
}

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from(".local/share"))
}
