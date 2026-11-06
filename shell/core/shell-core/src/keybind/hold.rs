use std::{cell::Cell, rc::Rc};

use gtk::{gdk, glib, prelude::*};

use super::{KeyChord, Keybind, KeybindWatcher, modifier_mask};

/// Actions emitted by a hold-session watcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HoldAction {
    Next,
    Previous,
    Activate,
    Cancel,
    ModifierReleased,
}

/// Configurable chords for a hold-session watcher.
#[derive(Clone, Debug)]
pub struct HoldBindings {
    pub next: Vec<KeyChord>,
    pub previous: Vec<KeyChord>,
    pub activate: Vec<KeyChord>,
    pub cancel: Vec<KeyChord>,
    pub release_modifiers: gdk::ModifierType,
}

impl Default for HoldBindings {
    fn default() -> Self {
        Self {
            next: vec![
                KeyChord::plain(gdk::Key::Tab),
                KeyChord::plain(gdk::Key::Down),
            ],
            previous: vec![
                KeyChord::plain(gdk::Key::ISO_Left_Tab),
                KeyChord::new(gdk::Key::Tab, gdk::ModifierType::SHIFT_MASK),
                KeyChord::plain(gdk::Key::Up),
            ],
            activate: vec![
                KeyChord::plain(gdk::Key::Return),
                KeyChord::plain(gdk::Key::KP_Enter),
            ],
            cancel: vec![KeyChord::plain(gdk::Key::Escape)],
            release_modifiers: gdk::ModifierType::SUPER_MASK.union(gdk::ModifierType::ALT_MASK),
        }
    }
}

/// Watches hold-session keys and modifier release.
///
/// `ModifierReleased` is emitted only while the session is active. The consumer
/// owns window visibility, focus, revealers, and what activation means.
pub struct HoldWatcher {
    press: KeybindWatcher<HoldAction>,
    release: gtk::EventControllerKey,
    target: glib::WeakRef<gtk::Widget>,
    active: Rc<Cell<bool>>,
    action: Rc<dyn Fn(HoldAction)>,
    release_modifiers: gdk::ModifierType,
}

impl HoldWatcher {
    pub fn install(
        target: &impl IsA<gtk::Widget>,
        bindings: HoldBindings,
        on_action: impl Fn(HoldAction) + 'static,
    ) -> Self {
        let active = Rc::new(Cell::new(false));
        let on_action: Rc<dyn Fn(HoldAction)> = Rc::new(on_action);

        let mut keybinds = Vec::new();
        for chord in bindings.next {
            keybinds.push(Keybind::new(chord, HoldAction::Next));
        }
        for chord in bindings.previous {
            keybinds.push(Keybind::new(chord, HoldAction::Previous));
        }
        for chord in bindings.activate {
            keybinds.push(Keybind::new(chord, HoldAction::Activate));
        }
        for chord in bindings.cancel {
            keybinds.push(Keybind::new(chord, HoldAction::Cancel));
        }

        let press_action = on_action.clone();
        let press = KeybindWatcher::install(target, keybinds, move |action| {
            press_action(action);
        });
        press.set_enabled(false);

        let release = gtk::EventControllerKey::new();
        release.set_propagation_phase(gtk::PropagationPhase::Capture);
        let release_active = active.clone();
        let release_action = on_action.clone();
        let release_modifiers = bindings.release_modifiers;
        release.connect_key_released(move |_, key, _, _| {
            if !release_active.get() {
                return;
            }
            let mask = modifier_mask(key);
            if !mask.is_empty() && (mask & release_modifiers) != gdk::ModifierType::empty() {
                release_action(HoldAction::ModifierReleased);
            }
        });
        target.add_controller(release.clone());

        let widget = target.upcast_ref::<gtk::Widget>().clone();
        Self {
            press,
            release,
            target: widget.downgrade(),
            active,
            action: on_action,
            release_modifiers: bindings.release_modifiers,
        }
    }

    pub fn start(&self) {
        self.active.set(true);
        self.press.set_enabled(true);
        // The invoking modifier can be released between the shortcut dispatch
        // and component startup. Release events are not replayed, so check the
        // current modifier state instead of waiting for an event that may
        // already have happened.
        if !self.modifiers_held() {
            (self.action)(HoldAction::ModifierReleased);
        }
    }

    pub fn stop(&self) {
        self.active.set(false);
        self.press.set_enabled(false);
    }

    pub fn is_active(&self) -> bool {
        self.active.get()
    }

    fn modifiers_held(&self) -> bool {
        gdk::Display::default()
            .and_then(|display| display.default_seat())
            .and_then(|seat| seat.keyboard())
            .map(|keyboard| !(keyboard.modifier_state() & self.release_modifiers).is_empty())
            .unwrap_or(true)
    }
}

impl Drop for HoldWatcher {
    fn drop(&mut self) {
        if let Some(target) = self.target.upgrade() {
            target.remove_controller(&self.release);
        }
    }
}
