use std::collections::HashMap;

use super::{NotificationsWindow, model::NotificationRequest};

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
    NotificationsWindow {
        center_visible: false,
        _request_server: None,
        dbus_service: None,
        generation: 1,
        notifications: Vec::new(),
        popup_notifications: vec![notification],
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
