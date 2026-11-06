//! Reusable keyboard binding and hold-session watchers.
use gtk::gdk;

mod hold;
mod watcher;

#[cfg(test)]
mod test;

pub use hold::{HoldAction, HoldBindings, HoldWatcher};
pub use watcher::{KeyChord, Keybind, KeybindWatcher};

pub(crate) const MODIFIER_MASK: gdk::ModifierType = gdk::ModifierType::SHIFT_MASK
    .union(gdk::ModifierType::CONTROL_MASK)
    .union(gdk::ModifierType::ALT_MASK)
    .union(gdk::ModifierType::SUPER_MASK)
    .union(gdk::ModifierType::META_MASK)
    .union(gdk::ModifierType::HYPER_MASK);

pub(crate) fn modifier_mask(key: gdk::Key) -> gdk::ModifierType {
    match key {
        gdk::Key::Shift_L | gdk::Key::Shift_R => gdk::ModifierType::SHIFT_MASK,
        gdk::Key::Control_L | gdk::Key::Control_R => gdk::ModifierType::CONTROL_MASK,
        gdk::Key::Alt_L | gdk::Key::Alt_R => gdk::ModifierType::ALT_MASK,
        gdk::Key::Super_L | gdk::Key::Super_R => gdk::ModifierType::SUPER_MASK,
        gdk::Key::Meta_L | gdk::Key::Meta_R => gdk::ModifierType::META_MASK,
        gdk::Key::Hyper_L | gdk::Key::Hyper_R => gdk::ModifierType::HYPER_MASK,
        _ => gdk::ModifierType::empty(),
    }
}
