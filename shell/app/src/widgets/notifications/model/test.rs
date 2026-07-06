use std::collections::HashMap;

use zvariant::OwnedValue;

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
