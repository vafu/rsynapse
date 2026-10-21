use std::cell::{Cell, OnceCell, RefCell};

use shell_core::gtk::{self, glib, prelude::*, subclass::prelude::*};

use super::{Config, RevealMode};

#[derive(Default)]
pub struct ButtonGroup {
    pub(super) config: Cell<Config>,
    pub(super) owner: RefCell<Option<gtk::Widget>>,
    pub(super) content: RefCell<Option<gtk::Box>>,
    pub(super) revealer: OnceCell<gtk::Revealer>,
    pub(super) hovered: Cell<bool>,
    pub(super) holds: RefCell<Vec<glib::WeakRef<gtk::Widget>>>,
}

#[glib::object_subclass]
impl ObjectSubclass for ButtonGroup {
    const NAME: &'static str = "RsynapseButtonGroup";
    type Type = super::ButtonGroup;
    type ParentType = gtk::Box;
}

impl ObjectImpl for ButtonGroup {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_css_class("button-group");
        let revealer = gtk::Revealer::new();
        obj.append(&revealer);
        self.revealer.set(revealer).unwrap();

        let motion = gtk::EventControllerMotion::new();
        motion.connect_enter(glib::clone!(
            #[weak]
            obj,
            move |_, _, _| {
                obj.imp().hovered.set(true);
                obj.imp().update_reveal();
            }
        ));
        motion.connect_leave(glib::clone!(
            #[weak]
            obj,
            move |_| {
                obj.imp().hovered.set(false);
                obj.imp().update_reveal();
            }
        ));
        obj.add_controller(motion);
        self.update_layout();
        self.update_reveal();
    }
}

impl WidgetImpl for ButtonGroup {}
impl BoxImpl for ButtonGroup {}

impl ButtonGroup {
    pub(super) fn update_layout(&self) {
        let obj = self.obj();
        let (orientation, transition, owner_first) = self.config.get().direction.layout();
        obj.set_orientation(orientation);
        let revealer = self.revealer.get().unwrap();
        revealer.set_transition_type(transition);
        if let Some(content) = self.content.borrow().as_ref() {
            content.set_orientation(orientation);
        }
        if let Some(owner) = self.owner.borrow().as_ref() {
            let sibling = (!owner_first).then_some(revealer);
            obj.reorder_child_after(owner, sibling);
        }
    }

    pub(super) fn update_reveal(&self) {
        let held = self
            .holds
            .borrow()
            .iter()
            .any(|hold| hold.upgrade().is_some_and(|widget| widget.is_visible()));
        let revealed =
            self.config.get().reveal_mode == RevealMode::Always || self.hovered.get() || held;
        self.revealer.get().unwrap().set_reveal_child(revealed);
        if let Some(owner) = self.owner.borrow().as_ref() {
            if revealed {
                owner.add_css_class("opened");
            } else {
                owner.remove_css_class("opened");
            }
        }
    }
}
