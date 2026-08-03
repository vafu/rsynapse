use shell_core::gtk::{self, prelude::*};

pub(crate) mod cod {
    pub(crate) const COD_MOVE: &str = "\u{eb22}";
    pub(crate) const COD_WORKSPACE_UNKNOWN: &str = "\u{ebc3}";
}

pub(crate) mod dev {
    pub(crate) const DEV_RUST: &str = "\u{e7a8}";
    pub(crate) const DEV_SLACK: &str = "\u{e8a4}";
    pub(crate) const DEV_TERMINAL: &str = "\u{e795}";
}

pub(crate) mod fa {
    pub(crate) const FA_BACKWARD_STEP: &str = "\u{f048}";
    pub(crate) const FA_CIRCLE_CHECK: &str = "\u{f05d}";
    pub(crate) const FA_FIREFOX: &str = "\u{f269}";
    pub(crate) const FA_FORWARD_STEP: &str = "\u{f051}";
    pub(crate) const FA_PLAY: &str = "\u{f04b}";
}

pub(crate) mod md {
    pub(crate) const MD_ALERT: &str = "\u{f0026}";
    pub(crate) const MD_APPLICATION: &str = "\u{f08c6}";
    pub(crate) const MD_BLUETOOTH: &str = "\u{f00af}";
    pub(crate) const MD_BLUETOOTH_CONNECT: &str = "\u{f00b1}";
    pub(crate) const MD_BLUETOOTH_OFF: &str = "\u{f00b2}";
    pub(crate) const MD_CAR_TURBOCHARGER: &str = "\u{f101a}";
    pub(crate) const MD_CELLPHONE_INFORMATION: &str = "\u{f0f41}";
    pub(crate) const MD_CLOUD_OFF_OUTLINE: &str = "\u{f0164}";
    pub(crate) const MD_GOOGLE_CHROME: &str = "\u{f02af}";
    pub(crate) const MD_HEADPHONES_SETTINGS: &str = "\u{f02cd}";
    pub(crate) const MD_KEYBOARD_RETURN: &str = "\u{f0311}";
    pub(crate) const MD_LEAF: &str = "\u{f032a}";
    pub(crate) const MD_MOUSE_VARIANT: &str = "\u{f037f}";
    pub(crate) const MD_ROBOT: &str = "\u{f06a9}";
    pub(crate) const MD_SPEEDOMETER: &str = "\u{f04c5}";
    pub(crate) const MD_SPEAKER: &str = "\u{f04c3}";
    pub(crate) const MD_WRENCH: &str = "\u{f05b7}";
}

pub(crate) mod ple {
    pub(crate) const PL_BRANCH: &str = "\u{e0a0}";
}

pub(crate) mod seti {
    pub(crate) const CUSTOM_FOLDER: &str = "\u{e5ff}";
    pub(crate) const CUSTOM_NEOVIM: &str = "\u{e6ae}";
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NerdIcon {
    glyph: String,
}

impl NerdIcon {
    pub(crate) fn new(glyph: impl Into<String>) -> Self {
        let glyph = non_empty(glyph.into()).unwrap_or_else(|| md::MD_APPLICATION.to_owned());
        Self { glyph }
    }

    pub(crate) fn application() -> Self {
        Self::new(md::MD_APPLICATION)
    }

    pub(crate) fn workspace() -> Self {
        Self::new(cod::COD_WORKSPACE_UNKNOWN)
    }

    pub(crate) fn move_handle() -> Self {
        Self::new(cod::COD_MOVE)
    }

    pub(crate) fn folder() -> Self {
        Self::new(seti::CUSTOM_FOLDER)
    }

    pub(crate) fn branch() -> Self {
        Self::new(ple::PL_BRANCH)
    }

    pub(crate) fn glyph(&self) -> &str {
        self.glyph.as_str()
    }
}

pub(crate) trait NerdIconLabelExt {
    fn set_nerd_icon(&self, icon: NerdIcon);
}

impl NerdIconLabelExt for gtk::Label {
    fn set_nerd_icon(&self, icon: NerdIcon) {
        self.set_halign(gtk::Align::Center);
        self.set_valign(gtk::Align::Center);
        self.set_xalign(0.5);
        self.set_yalign(0.5);
        self.set_justify(gtk::Justification::Center);
        self.set_single_line_mode(true);

        if self.label().as_str() == icon.glyph() {
            return;
        }

        self.set_label(icon.glyph());
    }
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn new_uses_the_given_glyph() {
        assert_eq!(NerdIcon::new(md::MD_ROBOT).glyph(), md::MD_ROBOT);
    }

    #[test]
    fn dedicated_constructors_use_their_own_glyph() {
        assert_eq!(NerdIcon::workspace().glyph(), cod::COD_WORKSPACE_UNKNOWN);
    }
}
