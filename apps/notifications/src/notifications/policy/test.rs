use super::super::model::{NotificationAction, NotificationUrgency, NotificationView};
use super::{NotificationCenterContext, NotificationCenterPolicy};

#[test]
fn default_policy_keeps_actionable_notifications() {
    let notification = notification_with_actions(vec![action("default", "Open")]);

    assert!(NotificationCenterPolicy::default().should_store(&notification, context(false)));
}

#[test]
fn default_policy_keeps_critical_notifications() {
    let notification = NotificationView {
        urgency: NotificationUrgency::Critical,
        ..notification()
    };

    assert!(NotificationCenterPolicy::default().should_store(&notification, context(false)));
}

#[test]
fn default_policy_rejects_plain_normal_notifications() {
    assert!(!NotificationCenterPolicy::default().should_store(&notification(), context(false)));
}

#[test]
fn default_policy_keeps_locked_notifications() {
    assert!(NotificationCenterPolicy::default().should_store(&notification(), context(true)));
}

#[test]
fn config_matches_title_content_and_action_presence() {
    let policy = NotificationCenterPolicy::from_toml(
        r#"
[[rules]]
title_match = "^Build"
content_match = "failed$"
actions = false
"#,
    )
    .unwrap();
    let notification = NotificationView {
        summary: "Build android".to_owned(),
        body: "compile failed".to_owned(),
        ..notification()
    };

    assert!(policy.should_store(&notification, context(false)));
}

#[test]
fn config_rules_are_ordered_first_match() {
    let policy = NotificationCenterPolicy::from_toml(
        r#"
[[rules]]
app_name_match = "^niri$"
keep = false

[[rules]]
is_urgent = true
"#,
    )
    .unwrap();
    let notification = NotificationView {
        app_name: "niri".to_owned(),
        urgency: NotificationUrgency::Critical,
        ..notification()
    };

    assert!(!policy.should_store(&notification, context(false)));
}

#[test]
fn config_can_match_session_locked() {
    let policy = NotificationCenterPolicy::from_toml(
        r#"
[[rules]]
session_locked = true
"#,
    )
    .unwrap();

    assert!(policy.should_store(&notification(), context(true)));
    assert!(!policy.should_store(&notification(), context(false)));
}

#[test]
fn config_can_match_action_key_or_label() {
    let policy = NotificationCenterPolicy::from_toml(
        r#"
[[rules]]
action_key_match = "^default$"
action_label_match = "Open"
"#,
    )
    .unwrap();
    let notification = notification_with_actions(vec![action("default", "Open")]);

    assert!(policy.should_store(&notification, context(false)));
}

#[test]
fn invalid_regex_is_rejected() {
    let error = NotificationCenterPolicy::from_toml(
        r#"
[[rules]]
title_match = "["
"#,
    )
    .unwrap_err();

    assert!(error.to_string().contains("rules[0].title_match"));
}

fn notification() -> NotificationView {
    NotificationView {
        id: 1,
        app_name: "App".to_owned(),
        app_icon: String::new(),
        image_path: None,
        summary: "Summary".to_owned(),
        body: "Body".to_owned(),
        actions: Vec::new(),
        urgency: NotificationUrgency::Normal,
        created_at: "12:00".to_owned(),
        generation: 1,
    }
}

fn notification_with_actions(actions: Vec<NotificationAction>) -> NotificationView {
    NotificationView {
        actions,
        ..notification()
    }
}

fn action(key: &str, label: &str) -> NotificationAction {
    NotificationAction {
        key: key.to_owned(),
        label: label.to_owned(),
    }
}

fn context(session_locked: bool) -> NotificationCenterContext {
    NotificationCenterContext { session_locked }
}
