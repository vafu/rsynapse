use shell_core::gtk::{self, prelude::*};

use crate::widgets::material_icon;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::widgets::bar) enum BuildIndicatorState {
    #[default]
    None,
    Running,
    Failed,
    Finished,
}

pub(in crate::widgets::bar) trait BuildIndicatorImageExt {
    fn set_build_indicator_state(&self, state: BuildIndicatorState);
}

pub(in crate::widgets::bar) fn image() -> gtk::Image {
    let image = gtk::Image::new();
    image.set_can_target(false);
    image.set_halign(gtk::Align::Center);
    image.set_valign(gtk::Align::Center);
    image.set_pixel_size(12);
    image.set_build_indicator_state(BuildIndicatorState::None);
    image
}

impl BuildIndicatorImageExt for gtk::Image {
    fn set_build_indicator_state(&self, state: BuildIndicatorState) {
        self.set_css_classes(&classes(state));
        self.set_icon_name(Some(material_icon::icon_name(icon(state)).as_str()));
        self.set_visible(state != BuildIndicatorState::None);
    }
}

fn classes(state: BuildIndicatorState) -> Vec<&'static str> {
    let mut classes = vec!["materialicon", "bar-build-indicator"];
    match state {
        BuildIndicatorState::None => {}
        BuildIndicatorState::Running => classes.push("build-running"),
        BuildIndicatorState::Failed => classes.push("build-failed"),
        BuildIndicatorState::Finished => classes.push("build-finished"),
    }
    classes
}

fn icon(state: BuildIndicatorState) -> &'static str {
    match state {
        BuildIndicatorState::None => "",
        BuildIndicatorState::Running => "build",
        BuildIndicatorState::Failed => "priority_high",
        BuildIndicatorState::Finished => "check",
    }
}
