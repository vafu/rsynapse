use shell_core::gtk::{self, prelude::*};

pub(super) const ITEM_CLASS: &str = "bar-item";
pub(super) const ICON_CLASS: &str = "bar-item-icon";
pub(super) const SQUARE_CLASS: &str = "bar-item-square";
pub(super) const ACTION_CLASS: &str = "bar-action";

pub(super) fn classes(extra: &[&'static str]) -> Vec<&'static str> {
    let mut classes = vec![ITEM_CLASS];
    classes.extend_from_slice(extra);
    classes
}

pub(super) fn action_classes(extra: &[&'static str]) -> Vec<&'static str> {
    let mut classes = vec!["flat", ACTION_CLASS];
    classes.extend_from_slice(extra);
    classes
}

pub(super) fn container(extra: &[&'static str]) -> gtk::Box {
    let widget = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .build();
    widget.add_css_class(ITEM_CLASS);
    for class in extra {
        widget.add_css_class(class);
    }
    widget.set_halign(gtk::Align::Center);
    widget.set_valign(gtk::Align::Center);
    widget
}

/// Shared decorative marker used by workspace/agent and GitHub badges.
/// Retains the existing marker geometry and theme classes in one place.
pub(super) fn unseen_badge() -> gtk::Box {
    let badge = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    badge.set_css_classes(&["bar-badge", "agent-unseen-badge"]);
    badge.set_can_target(false);
    badge.set_width_request(8);
    badge.set_height_request(8);
    badge.set_halign(gtk::Align::End);
    badge.set_valign(gtk::Align::Start);
    badge
}
