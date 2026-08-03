use shell_core::gtk::{self, prelude::*};

use crate::widgets::BACKGROUND_BLUR_CLASS;

pub(super) const ITEM_CLASS: &str = "bar-item";
pub(super) const ICON_CLASS: &str = "bar-item-icon";
pub(super) const SQUARE_CLASS: &str = "bar-item-square";
pub(super) const ACTION_CLASS: &str = "bar-action";

pub(super) fn classes(extra: &[&'static str]) -> Vec<&'static str> {
    let mut classes = vec![ITEM_CLASS, BACKGROUND_BLUR_CLASS];
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
    widget.add_css_class(BACKGROUND_BLUR_CLASS);
    for class in extra {
        widget.add_css_class(class);
    }
    widget.set_halign(gtk::Align::Center);
    widget.set_valign(gtk::Align::Center);
    widget
}
