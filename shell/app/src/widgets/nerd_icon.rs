use shell_core::gtk::{self, prelude::*};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NerdIcon {
    key: String,
    glyph: String,
}

impl NerdIcon {
    pub(crate) fn new(key: impl Into<String>, glyph: impl Into<String>) -> Self {
        let key = non_empty(key.into()).unwrap_or_else(|| "nf-md-application".to_owned());
        let glyph = non_empty(glyph.into()).unwrap_or_else(|| "󰣆".to_owned());
        Self { key, glyph }
    }

    pub(crate) fn from_parts(
        key: impl Into<String>,
        glyph: Option<String>,
        fallback: Self,
    ) -> Self {
        let key = key.into();
        glyph
            .and_then(non_empty)
            .map(|glyph| Self::new(key.clone(), glyph))
            .or_else(|| icon_alias(&key))
            .unwrap_or(fallback)
    }

    pub(crate) fn application() -> Self {
        Self::new("nf-md-application", "󰣆")
    }

    pub(crate) fn workspace() -> Self {
        Self::new("nf-cod-workspace_unknown", "")
    }

    pub(crate) fn automatic() -> Self {
        Self::new("nf-fa-wand_magic", "")
    }

    pub(crate) fn move_handle() -> Self {
        Self::new("nf-cod-move", "")
    }

    pub(crate) fn folder() -> Self {
        Self::new("nf-custom-folder", "")
    }

    pub(crate) fn branch() -> Self {
        Self::new("nf-pl-branch", "")
    }

    pub(crate) fn key(&self) -> &str {
        self.key.as_str()
    }

    pub(crate) fn glyph(&self) -> &str {
        self.glyph.as_str()
    }

    pub(crate) fn label(&self) -> String {
        format!("{} {}", self.key, self.glyph)
    }

    pub(crate) fn from_name(name: &str) -> Self {
        icon_alias(name).unwrap_or_else(Self::application)
    }

    pub(crate) fn for_agent_hints<'a>(hints: impl IntoIterator<Item = &'a str>) -> Self {
        hints
            .into_iter()
            .filter_map(non_empty_str)
            .find_map(icon_alias)
            .unwrap_or_else(|| Self::new("nf-md-robot", "󰚩"))
    }
}

pub(crate) trait NerdIconLabelExt {
    fn set_nerd_icon(&self, icon: NerdIcon);
}

impl NerdIconLabelExt for gtk::Label {
    fn set_nerd_icon(&self, icon: NerdIcon) {
        self.set_halign(gtk::Align::Center);
        self.set_valign(gtk::Align::Center);
        self.set_hexpand(false);
        self.set_vexpand(false);
        self.set_xalign(0.5);
        self.set_yalign(0.5);
        self.set_justify(gtk::Justification::Center);
        self.set_single_line_mode(true);

        if self.widget_name().as_str() == icon.key() && self.label().as_str() == icon.glyph() {
            return;
        }

        let tooltip = icon.label();
        self.set_widget_name(icon.key());
        self.set_label(icon.glyph());
        self.set_tooltip_text(Some(tooltip.as_str()));
    }
}

fn icon_alias(value: &str) -> Option<NerdIcon> {
    let value = normalize(value);
    let icon = match value.as_str() {
        "skip previous" => NerdIcon::new("nf-fa-step_backward", ""),
        "skip next" => NerdIcon::new("nf-fa-step_forward", ""),
        "play" | "play arrow" => NerdIcon::new("nf-fa-play", ""),
        "pause" => NerdIcon::new("nf-fa-pause", ""),
        "bolt" => NerdIcon::new("nf-md-bolt", "󰶳"),
        "eco" | "leaf" => NerdIcon::new("nf-md-leaf", "󰌪"),
        "speed" | "speedometer" => NerdIcon::new("nf-md-speedometer", "󰓅"),
        "bluetooth" => NerdIcon::new("nf-md-bluetooth", "󰂯"),
        "bluetooth connected" => NerdIcon::new("nf-md-bluetooth_connect", "󰂱"),
        "bluetooth disabled" | "bluetooth off" => NerdIcon::new("nf-md-bluetooth_off", "󰂲"),
        "keyboard" => NerdIcon::new("nf-md-keyboard_return", "󰌑"),
        "headphones" | "audio headphones" => NerdIcon::new("nf-md-headphones_settings", "󰋍"),
        "mouse" | "pointer" => NerdIcon::new("nf-md-mouse_variant", "󰍿"),
        "smartphone" | "phone" => NerdIcon::new("nf-md-cellphone_information", "󰽁"),
        "build" | "build circle" | "wrench" => NerdIcon::new("nf-md-wrench", "󰖷"),
        "check" | "check circle" => NerdIcon::new("nf-fa-check_circle_o", ""),
        "priority high" | "error" | "alert" | "warning" => NerdIcon::new("nf-md-alert", "󰀦"),
        "cloud off" => NerdIcon::new("nf-md-cloud_off_outline", "󰅟"),
        _ if value.contains("chrome") || value.contains("chromium") => {
            NerdIcon::new("nf-md-google_chrome", "󰊯")
        }
        _ if value.contains("slack") => NerdIcon::new("nf-dev-slack", ""),
        _ if value.contains("neovim") || value.contains("nvim") => {
            NerdIcon::new("nf-custom-neovim", "")
        }
        _ if value.contains("ghostty") || value.contains("terminal") || value.contains("term") => {
            NerdIcon::new("nf-dev-terminal", "")
        }
        _ if value.contains("codex") || value.contains("agent") || value.contains("cognition") => {
            NerdIcon::new("nf-md-robot", "󰚩")
        }
        _ if value.contains("workspace") || value == "workspaces" => NerdIcon::workspace(),
        _ if value.contains("folder") || value.contains("project") => NerdIcon::folder(),
        _ if value.contains("git")
            || value.contains("branch")
            || value.contains("account tree") =>
        {
            NerdIcon::branch()
        }
        _ if value.contains("application")
            || value.contains("executable")
            || value.contains("window") =>
        {
            NerdIcon::application()
        }
        _ => return None,
    };
    Some(icon)
}

fn normalize(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(".desktop")
        .replace(['_', '-', '.'], " ")
        .to_ascii_lowercase()
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn non_empty_str(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parts_without_glyph_use_alias_or_fallback() {
        assert_eq!(
            NerdIcon::from_parts("workspaces", None, NerdIcon::application()).key(),
            "nf-cod-workspace_unknown"
        );
        assert_eq!(
            NerdIcon::from_parts("unknown", None, NerdIcon::workspace()).key(),
            "nf-cod-workspace_unknown"
        );
    }
}
