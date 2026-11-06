use gtk::gdk;

use super::{HoldBindings, KeyChord, MODIFIER_MASK, modifier_mask};

#[test]
fn chord_matching_ignores_lock_modifiers() {
    let chord = KeyChord::new(gdk::Key::Tab, gdk::ModifierType::SHIFT_MASK);
    assert!(chord.matches(
        gdk::Key::Tab,
        gdk::ModifierType::SHIFT_MASK | gdk::ModifierType::LOCK_MASK
    ));
    assert!(!chord.matches(gdk::Key::Tab, gdk::ModifierType::empty()));
    assert!(!chord.matches(
        gdk::Key::Tab,
        gdk::ModifierType::SHIFT_MASK | gdk::ModifierType::CONTROL_MASK
    ));
}

#[test]
fn any_modifiers_matches_navigation_while_hold_modifier_is_down() {
    let chord = KeyChord::any_modifiers(gdk::Key::Down);
    assert!(chord.matches(gdk::Key::Down, gdk::ModifierType::empty()));
    assert!(chord.matches(gdk::Key::Down, gdk::ModifierType::SUPER_MASK));
    assert!(chord.matches(
        gdk::Key::Down,
        gdk::ModifierType::SUPER_MASK | gdk::ModifierType::CONTROL_MASK
    ));
    assert!(!chord.matches(gdk::Key::Up, gdk::ModifierType::SUPER_MASK));
}

#[test]
fn modifier_mask_maps_modifier_keys() {
    assert_eq!(
        modifier_mask(gdk::Key::Super_L),
        gdk::ModifierType::SUPER_MASK
    );
    assert_eq!(modifier_mask(gdk::Key::Alt_R), gdk::ModifierType::ALT_MASK);
    assert_eq!(modifier_mask(gdk::Key::a), gdk::ModifierType::empty());
}

#[test]
fn default_hold_bindings_cover_common_navigation() {
    let bindings = HoldBindings::default();
    assert!(bindings.next.contains(&KeyChord::plain(gdk::Key::Tab)));
    assert!(bindings.previous.contains(&KeyChord::plain(gdk::Key::Up)));
    assert!(
        bindings
            .activate
            .contains(&KeyChord::plain(gdk::Key::Return))
    );
    assert!(bindings.cancel.contains(&KeyChord::plain(gdk::Key::Escape)));
    assert!(
        bindings
            .release_modifiers
            .contains(gdk::ModifierType::SUPER_MASK)
    );
    assert_eq!(
        bindings.release_modifiers & !MODIFIER_MASK,
        gdk::ModifierType::empty()
    );
}
