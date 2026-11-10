mod imp;

#[cfg(test)]
mod test;

use shell_core::gtk::{self, glib, prelude::*, subclass::prelude::*};

/// Direction away from the owner; also determines the group's orientation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    fn layout(self) -> (gtk::Orientation, gtk::RevealerTransitionType, bool) {
        use gtk::{Orientation as O, RevealerTransitionType as T};
        match self {
            Self::Left => (O::Horizontal, T::SlideLeft, false),
            Self::Right => (O::Horizontal, T::SlideRight, true),
            Self::Up => (O::Vertical, T::SlideUp, false),
            Self::Down => (O::Vertical, T::SlideDown, true),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RevealMode {
    #[default]
    Hover,
    Always,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Config {
    pub direction: Direction,
    pub reveal_mode: RevealMode,
}

glib::wrapper! {
    /// Owner + revealed content, with one layout and hover/hold implementation.
    pub struct ButtonGroup(ObjectSubclass<imp::ButtonGroup>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for ButtonGroup {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}

impl ButtonGroup {
    pub fn set_config(&self, config: Config) {
        self.imp().config.set(config);
        self.imp().update_layout();
        self.imp().update_reveal();
    }

    /// The owner fills the cross axis so its contents can remain centered.
    pub fn set_owner(&self, owner: &impl IsA<gtk::Widget>) {
        let imp = self.imp();
        if let Some(previous) = imp.owner.take() {
            self.remove(&previous);
        }
        let owner = owner.as_ref();
        owner.add_css_class("button-group-owner");
        owner.set_halign(gtk::Align::Fill);
        owner.set_valign(gtk::Align::Fill);
        self.append(owner);
        imp.owner.replace(Some(owner.clone()));
        imp.update_layout();
        imp.update_reveal();
    }

    pub fn set_content(&self, content: &gtk::Box) {
        content.add_css_class("button-subgroup");
        self.imp().revealer.get().unwrap().set_child(Some(content));
        self.imp().content.replace(Some(content.clone()));
        self.imp().update_layout();
    }

    /// Keep hover-mode groups open while any registered popup is visible.
    /// Weak references avoid retaining popovers or cycling through their parents.
    pub fn hold_while_visible(&self, widget: &impl IsA<gtk::Widget>) {
        let widget = widget.as_ref();
        self.imp().holds.borrow_mut().push(widget.downgrade());
        widget.connect_visible_notify(glib::clone!(
            #[weak(rename_to = group)]
            self,
            move |_| {
                group.imp().update_reveal();
            }
        ));
        self.imp().update_reveal();
    }
}
