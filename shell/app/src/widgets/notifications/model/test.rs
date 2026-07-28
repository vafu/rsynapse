use std::collections::HashMap;

use zvariant::{OwnedValue, Value};

use super::{NotificationRequest, NotificationUrgency};

#[test]
fn notification_request_pairs_actions() {
    let request = NotificationRequest::new(
        7,
        "App".to_owned(),
        String::new(),
        "Summary".to_owned(),
        "Body".to_owned(),
        vec![
            "default".to_owned(),
            "Open".to_owned(),
            "dismiss".to_owned(),
            "Dismiss".to_owned(),
            "dangling".to_owned(),
        ],
        HashMap::new(),
        -1,
    );

    assert_eq!(request.actions.len(), 2);
    assert_eq!(request.actions[0].key, "default");
    assert_eq!(request.actions[0].label, "Open");
    assert_eq!(request.expire_timeout_ms, 5000);
}

#[test]
fn notification_request_drops_blank_actions() {
    let request = NotificationRequest::new(
        8,
        "App".to_owned(),
        String::new(),
        "Summary".to_owned(),
        "Body".to_owned(),
        vec![
            "default".to_owned(),
            String::new(),
            String::new(),
            "Open".to_owned(),
            "open".to_owned(),
            "Open".to_owned(),
        ],
        HashMap::new(),
        -1,
    );

    assert_eq!(request.actions.len(), 1);
    assert_eq!(request.actions[0].key, "open");
    assert_eq!(request.actions[0].label, "Open");
}

#[test]
fn notification_request_reads_urgency_hint() {
    let mut hints = HashMap::new();
    hints.insert("urgency".to_owned(), OwnedValue::from(2_u8));

    let request = NotificationRequest::new(
        1,
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        Vec::new(),
        hints,
        0,
    );

    assert_eq!(request.urgency, NotificationUrgency::Critical);
    assert_eq!(request.expire_timeout_ms, 0);
}

#[test]
fn notification_request_reads_image_path_hint() {
    let mut hints = HashMap::new();
    hints.insert(
        "image-path".to_owned(),
        OwnedValue::try_from(Value::from("/tmp/screenshot.png")).unwrap(),
    );

    let request = NotificationRequest::new(
        2,
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        Vec::new(),
        hints,
        -1,
    );

    assert_eq!(request.image_path.as_deref(), Some("/tmp/screenshot.png"));
}

#[test]
fn notification_request_ignores_icon_name_image_path_hint() {
    let mut hints = HashMap::new();
    hints.insert(
        "desktop-entry".to_owned(),
        OwnedValue::try_from(Value::from("com.mitchellh.ghostty")).unwrap(),
    );
    hints.insert(
        "image-path".to_owned(),
        OwnedValue::try_from(Value::from("com.mitchellh.ghostty")).unwrap(),
    );

    let request = NotificationRequest::new(
        3,
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        Vec::new(),
        hints,
        -1,
    );

    assert_eq!(request.app_icon, "com.mitchellh.ghostty");
    assert_eq!(request.image_path, None);
}

#[test]
fn notification_request_uses_file_app_icon_as_image_fallback() {
    let request = NotificationRequest::new(
        4,
        "niri".to_owned(),
        "file:///tmp/Screenshot%20One.png".to_owned(),
        String::new(),
        String::new(),
        Vec::new(),
        HashMap::new(),
        -1,
    );

    assert_eq!(request.app_icon, "");
    assert_eq!(
        request.image_path.as_deref(),
        Some("/tmp/Screenshot One.png")
    );
}
