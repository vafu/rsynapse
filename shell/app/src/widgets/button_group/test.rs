use super::*;

// One GTK-thread test, without presenting a window or touching live surfaces.
#[test]
fn layout_reveal_and_lifetime() {
    gtk::init().expect("GTK display required for button-group integration test");
    let group = ButtonGroup::default();
    let owner = gtk::Box::default();
    let content = gtk::Box::default();
    group.set_owner(&owner);
    group.set_content(&content);
    let revealer = group.imp().revealer.get().unwrap();

    for (direction, orientation, transition, owner_first) in [
        (
            Direction::Left,
            gtk::Orientation::Horizontal,
            gtk::RevealerTransitionType::SlideLeft,
            false,
        ),
        (
            Direction::Right,
            gtk::Orientation::Horizontal,
            gtk::RevealerTransitionType::SlideRight,
            true,
        ),
        (
            Direction::Up,
            gtk::Orientation::Vertical,
            gtk::RevealerTransitionType::SlideUp,
            false,
        ),
        (
            Direction::Down,
            gtk::Orientation::Vertical,
            gtk::RevealerTransitionType::SlideDown,
            true,
        ),
    ] {
        group.set_config(Config {
            direction,
            reveal_mode: RevealMode::Hover,
        });
        assert_eq!(group.orientation(), orientation);
        assert_eq!(content.orientation(), orientation);
        assert_eq!(revealer.transition_type(), transition);
        assert_eq!(group.first_child().unwrap() == owner, owner_first);
        assert_eq!(owner.halign(), gtk::Align::Fill);
        assert_eq!(owner.valign(), gtk::Align::Fill);
    }

    let motion = group
        .observe_controllers()
        .item(0)
        .unwrap()
        .downcast::<gtk::EventControllerMotion>()
        .unwrap();
    assert!(!revealer.reveals_child());
    motion.emit_by_name::<()>("enter", &[&0.0_f64, &0.0_f64]);
    assert!(revealer.reveals_child());
    assert!(owner.has_css_class("opened"));

    // Visibility holds are independent: leaving hover and closing one popup
    // must not collapse another popup's anchor.
    let first_popup = gtk::Box::default();
    let second_popup = gtk::Box::default();
    first_popup.set_visible(false);
    second_popup.set_visible(false);
    group.hold_while_visible(&first_popup);
    group.hold_while_visible(&second_popup);
    first_popup.set_visible(true);
    second_popup.set_visible(true);
    motion.emit_by_name::<()>("leave", &[]);
    assert!(revealer.reveals_child());
    first_popup.set_visible(false);
    assert!(revealer.reveals_child());
    second_popup.set_visible(false);
    assert!(!revealer.reveals_child());
    assert!(!owner.has_css_class("opened"));

    // Already-visible holds are honored at registration, without a new signal.
    first_popup.set_visible(true);
    let other_group = ButtonGroup::default();
    other_group.hold_while_visible(&first_popup);
    assert!(other_group.imp().revealer.get().unwrap().reveals_child());
    first_popup.set_visible(false);

    group.set_config(Config {
        direction: Direction::Down,
        reveal_mode: RevealMode::Always,
    });
    motion.emit_by_name::<()>("enter", &[&0.0_f64, &0.0_f64]);
    motion.emit_by_name::<()>("leave", &[]);
    first_popup.set_visible(true);
    first_popup.set_visible(false);
    assert!(revealer.reveals_child());
    assert!(owner.has_css_class("opened"));

    // Slot replacement respects current layout and state.
    let replacement = gtk::Box::default();
    group.set_owner(&replacement);
    assert!(owner.parent().is_none());
    assert_eq!(group.first_child().unwrap(), replacement);
    assert!(replacement.has_css_class("opened"));
    let new_content = gtk::Box::default();
    group.set_content(&new_content);
    assert!(content.parent().is_none());
    assert_eq!(new_content.orientation(), gtk::Orientation::Vertical);

    // Popup signals and motion callbacks must not retain their group.
    let weak = group.downgrade();
    drop(group);
    assert!(weak.upgrade().is_none());
    first_popup.set_visible(true);
}
