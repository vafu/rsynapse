use std::collections::HashMap;

use super::{NotificationsWindow, model::NotificationRequest, policy::NotificationCenterPolicy};

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

fn notifications_window_with_popup(notification: super::NotificationView) -> NotificationsWindow {
    let mut window = notifications_window();
    window.popup_notifications.push(notification);
    window
}

fn notifications_window_with_policy(
    notification: super::NotificationView,
    center_policy: NotificationCenterPolicy,
) -> NotificationsWindow {
    let mut window = notifications_window();
    window.center_policy = center_policy;
    window.popup_notifications.push(notification);
    window
}

fn notifications_window() -> NotificationsWindow {
    NotificationsWindow {
        center_visible: false,
        _request_server: None,
        center_policy: NotificationCenterPolicy::default(),
        dbus_service: None,
        session_locked: false,
        _session_lock_task: None,
        generation: 1,
        notifications: Vec::new(),
        popup_notifications: Vec::new(),
    }
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
