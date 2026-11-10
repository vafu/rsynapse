use std::collections::HashMap;

use super::{
    NotificationsWindow,
    model::{NotificationEntry, NotificationRequest},
    policy::NotificationCenterPolicy,
};

#[test]
fn expiring_normal_popup_does_not_add_center_item() {
    let mut window = notifications_window_with_popup(notification(Vec::new()).into_view(1));

    window.expire_popup(1, 1);

    assert!(window.popup_notifications.is_empty());
    assert!(window.notifications.is_empty());
}

#[test]
fn expiring_actionable_popup_adds_center_item() {
    let mut window = notifications_window_with_popup(
        notification(vec!["default".to_owned(), "Open".to_owned()]).into_view(1),
    );

    window.expire_popup(1, 1);

    assert!(window.popup_notifications.is_empty());
    assert_eq!(window.notifications.len(), 1);
    assert_eq!(window.notifications[0].id, 1);
}

#[test]
fn expiring_configured_title_adds_center_item() {
    let policy = NotificationCenterPolicy::from_toml(
        r#"
[[rules]]
title_match = "^Summary$"
"#,
    )
    .unwrap();
    let mut window =
        notifications_window_with_policy(notification(Vec::new()).into_view(1), policy);

    window.expire_popup(1, 1);

    assert!(window.popup_notifications.is_empty());
    assert_eq!(window.notifications.len(), 1);
    assert_eq!(window.notifications[0].id, 1);
}

#[test]
fn locked_session_show_adds_center_item_without_popup() {
    let mut window = notifications_window();

    window.set_session_locked(true);

    assert_eq!(window.apply_notification(notification(Vec::new())), None);
    assert!(window.popup_notifications.is_empty());
    assert_eq!(window.notifications.len(), 1);
    assert_eq!(window.notifications[0].id, 1);
}

#[test]
fn locking_session_moves_popup_to_center() {
    let mut window = notifications_window_with_popup(notification(Vec::new()).into_view(1));

    window.set_session_locked(true);

    assert!(window.popup_notifications.is_empty());
    assert_eq!(window.notifications.len(), 1);
    assert_eq!(window.notifications[0].id, 1);
}

#[test]
fn close_before_expiry_prevents_center_item() {
    let mut window = notifications_window_with_popup(
        notification(vec!["default".to_owned(), "Open".to_owned()]).into_view(1),
    );

    window.close_notification(1, super::NotificationClosedReason::Dismissed);
    window.expire_popup(1, 1);

    assert!(window.popup_notifications.is_empty());
    assert!(window.notifications.is_empty());
}

#[test]
fn same_id_update_keeps_entry_signal_identity() {
    let mut entries = vec![NotificationEntry::new(
        notification(Vec::new()).into_view(1),
    )];
    let original = entries[0].view.clone();

    let mut updated = notification(Vec::new());
    updated.summary = "Updated".to_owned();
    super::upsert_latest(&mut entries, updated.into_view(2));

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].view, original);
    assert_eq!(entries[0].current_view().summary, "Updated");
    assert_eq!(entries[0].generation, 2);
}

#[test]
fn new_id_inserts_structurally_and_close_removes() {
    let mut entries = vec![NotificationEntry::new(
        notification(Vec::new()).into_view(1),
    )];
    let first = entries[0].view.clone();

    let mut second_request = notification(Vec::new());
    second_request.id = 2;
    super::upsert_latest(&mut entries, second_request.into_view(2));

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].id, 2);
    assert_eq!(entries[1].view, first);

    super::remove_notification(&mut entries, 2);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, 1);
}

#[test]
fn remove_generation_returns_current_view() {
    let mut entries = vec![NotificationEntry::new(
        notification(Vec::new()).into_view(4),
    )];

    let removed = super::remove_generation(&mut entries, 1, 4);
    assert_eq!(removed.map(|entry| entry.generation), Some(4));
    assert!(entries.is_empty());

    let mut entries = vec![NotificationEntry::new(
        notification(Vec::new()).into_view(4),
    )];
    assert!(super::remove_generation(&mut entries, 1, 3).is_none());
    assert_eq!(entries.len(), 1);
}

fn notifications_window_with_popup(notification: super::NotificationView) -> NotificationsWindow {
    let mut window = notifications_window();
    window
        .popup_notifications
        .push(NotificationEntry::new(notification));
    window
}

fn notifications_window_with_policy(
    notification: super::NotificationView,
    center_policy: NotificationCenterPolicy,
) -> NotificationsWindow {
    let mut window = notifications_window();
    window.center_policy = center_policy;
    window
        .popup_notifications
        .push(NotificationEntry::new(notification));
    window
}

fn notifications_window() -> NotificationsWindow {
    NotificationsWindow::new(
        false,
        None,
        NotificationCenterPolicy::default(),
        None,
        1,
        Vec::new(),
        Vec::new(),
        None,
        None,
    )
}

fn notification(actions: Vec<String>) -> NotificationRequest {
    NotificationRequest::new(
        1,
        "App".to_owned(),
        String::new(),
        "Summary".to_owned(),
        "Body".to_owned(),
        actions,
        HashMap::new(),
        5000,
    )
}
