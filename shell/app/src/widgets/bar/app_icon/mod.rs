#[cfg(test)]
mod test;

use shell_core::gtk;

const FALLBACK_APPLICATION_ICON: &str = "application-x-executable-symbolic";

pub(in crate::widgets::bar) fn icon_name(icon: &str) -> String {
    icon_name_with_lookup(icon, theme_has_icon)
}

fn icon_name_with_lookup(icon: &str, has_icon: impl Fn(&str) -> bool) -> String {
    let icon = icon.trim();
    if icon.is_empty() || icon.contains('/') || !has_icon(icon) {
        FALLBACK_APPLICATION_ICON.to_owned()
    } else {
        icon.to_owned()
    }
}

fn theme_has_icon(icon: &str) -> bool {
    gtk::gdk::Display::default()
        .map(|display| gtk::IconTheme::for_display(&display).has_icon(icon))
        .unwrap_or(true)
}
