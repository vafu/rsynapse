use nerd_font_symbols::{cod, fa, md, ple, seti};
use shell_core::gtk::{self, prelude::*};

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

    pub(crate) fn automatic() -> Self {
        Self::new(fa::FA_WAND_MAGIC)
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
