use nerd_font_symbols::{fa, md};
use shell_core::gtk::{self, prelude::*};

use crate::widgets::nerd_icon::{NerdIcon, NerdIconLabelExt};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::widgets::bar) enum BuildIndicatorState {
    #[default]
    None,
    Running,
    Failed,
    Finished,
}

pub(in crate::widgets::bar) trait BuildIndicatorLabelExt {
    fn set_build_indicator_state(&self, state: BuildIndicatorState);
}

pub(in crate::widgets::bar) fn label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_can_target(false);
    label.set_halign(gtk::Align::Center);
    label.set_valign(gtk::Align::Center);
    label.set_build_indicator_state(BuildIndicatorState::None);
    label
}

impl BuildIndicatorLabelExt for gtk::Label {
    fn set_build_indicator_state(&self, state: BuildIndicatorState) {
        self.set_css_classes(&classes(state));
        self.set_nerd_icon(icon(state));
        self.set_visible(state != BuildIndicatorState::None);
    }
}

fn classes(state: BuildIndicatorState) -> Vec<&'static str> {
    let mut classes = vec!["nerdicon", "bar-build-indicator"];
    match state {
        BuildIndicatorState::None => {}
        BuildIndicatorState::Running => classes.push("build-running"),
        BuildIndicatorState::Failed => classes.push("build-failed"),
        BuildIndicatorState::Finished => classes.push("build-finished"),
    }
    classes
}

fn icon(state: BuildIndicatorState) -> NerdIcon {
    match state {
        BuildIndicatorState::None => NerdIcon::application(),
        BuildIndicatorState::Running => NerdIcon::new(md::MD_WRENCH),
        BuildIndicatorState::Failed => NerdIcon::new(md::MD_ALERT),
        BuildIndicatorState::Finished => NerdIcon::new(fa::FA_CIRCLE_CHECK),
    }
}
