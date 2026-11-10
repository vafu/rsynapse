use std::{cell::Cell, marker::PhantomData, rc::Rc};

use gtk::{gdk, glib, prelude::*};

use super::MODIFIER_MASK;

/// A key plus the modifiers that must be held for it to match.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyChord {
    pub key: gdk::Key,
    pub modifiers: gdk::ModifierType,
    ignore_modifiers: bool,
}

impl KeyChord {
    pub const fn new(key: gdk::Key, modifiers: gdk::ModifierType) -> Self {
        Self {
            key,
            modifiers,
            ignore_modifiers: false,
        }
    }

    pub const fn plain(key: gdk::Key) -> Self {
        Self::new(key, gdk::ModifierType::empty())
    }

    /// Matches the key regardless of which modifiers are held.
    ///
    /// Useful for navigation keys that should keep working while a hold
    /// modifier is still down.
    pub const fn any_modifiers(key: gdk::Key) -> Self {
        Self {
            key,
            modifiers: gdk::ModifierType::empty(),
            ignore_modifiers: true,
        }
    }

    pub(crate) fn matches(self, key: gdk::Key, modifiers: gdk::ModifierType) -> bool {
        self.key == key
            && (self.ignore_modifiers
                || (self.modifiers & MODIFIER_MASK) == (modifiers & MODIFIER_MASK))
    }
}

/// One configurable key chord and the action it emits.
#[derive(Clone, Debug)]
pub struct Keybind<A> {
    pub chord: KeyChord,
    pub action: A,
}

impl<A> Keybind<A> {
    pub const fn new(chord: KeyChord, action: A) -> Self {
        Self { chord, action }
    }
}

/// Watches configured key chords on any GTK widget.
///
/// Only matching chords are consumed; all other key events continue to the
/// focused widget. Dropping the watcher removes its controller.
pub struct KeybindWatcher<A> {
    controller: gtk::EventControllerKey,
    target: glib::WeakRef<gtk::Widget>,
    enabled: Rc<Cell<bool>>,
    _action: PhantomData<A>,
}

impl<A> KeybindWatcher<A>
where
    A: Clone + 'static,
{
    pub fn install(
        target: &impl IsA<gtk::Widget>,
        bindings: impl IntoIterator<Item = Keybind<A>>,
        on_action: impl Fn(A) + 'static,
    ) -> Self {
        let bindings = Rc::new(bindings.into_iter().collect::<Vec<_>>());
        let on_action = Rc::new(on_action);
        let enabled = Rc::new(Cell::new(true));
        let controller = gtk::EventControllerKey::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);

        let bindings_for_press = bindings.clone();
        let action_for_press = on_action.clone();
        let enabled_for_press = enabled.clone();
        controller.connect_key_pressed(move |_, key, _, modifiers| {
            if !enabled_for_press.get() {
                return glib::Propagation::Proceed;
            }
            let state = modifiers & MODIFIER_MASK;
            if let Some(binding) = bindings_for_press
                .iter()
                .find(|binding| binding.chord.matches(key, state))
            {
                action_for_press(binding.action.clone());
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });

        target.add_controller(controller.clone());
        let widget = target.upcast_ref::<gtk::Widget>().clone();
        Self {
            controller,
            target: widget.downgrade(),
            enabled,
            _action: PhantomData,
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.get()
    }
}

impl<A> Drop for KeybindWatcher<A> {
    fn drop(&mut self) {
        if let Some(target) = self.target.upgrade() {
            target.remove_controller(&self.controller);
        }
    }
}
