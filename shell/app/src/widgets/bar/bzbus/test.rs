use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    source,
    view::{self, Invocation},
};

#[test]
fn offline_view_reports_offline() {
    let view = view::view(false, Vec::new());

    assert!(!view.progress_visible);
    assert_eq!(view.icon, "cloud_off");
    assert!(view.classes.contains(&"offline"));
    assert_eq!(view.progress_level_classes, vec!["level", "offline"]);
}

#[test]
fn active_invocation_uses_progress_and_failures() {
    let started_at_unix_ms = now_unix_ms() - 60_000;
    let view = view::view(
        true,
        vec![Invocation {
            id: "invocation_1".to_owned(),
            build_id: "build-1".to_owned(),
            component: "repo".to_owned(),
            source: "cli".to_owned(),
            last_sequence: 7,
            status: "running".to_owned(),
            outcome: String::new(),
            command_name: "test".to_owned(),
            started_at_unix_ms,
            ended_at_unix_ms: 0,
            progress_completed: 3,
            progress_total: 9,
            actions_completed: 12,
            total_actions: 40,
            actions_failed: 2,
            running_actions: 4,
        }],
    );

    assert_eq!(view.icon, "build_circle");
    assert_eq!(view.progress_percent, 33);
    assert!(view.progress_visible);
    assert!(view.tooltip.contains("progress: 3/9 · 12a/4r"));
    assert!(view.classes.contains(&"running"));
    assert_eq!(view.progress_level_classes, vec!["level", "running"]);
}

#[test]
fn latest_active_invocation_wins_over_finished() {
    let now = now_unix_ms();
    let finished = Invocation {
        id: "finished".to_owned(),
        status: "finished".to_owned(),
        outcome: "success".to_owned(),
        started_at_unix_ms: now - 120_000,
        ended_at_unix_ms: now - 60_000,
        last_sequence: 20,
        ..Invocation::default()
    };
    let running = Invocation {
        id: "running".to_owned(),
        status: "running".to_owned(),
        command_name: "build".to_owned(),
        started_at_unix_ms: now - 30_000,
        last_sequence: 10,
        ..Invocation::default()
    };

    let view = view::view(true, vec![finished, running]);

    assert!(!view.progress_visible);
    assert!(view.tooltip.contains("invocation: running"));
}

#[test]
fn parses_wrapped_dbus_integer_values() {
    assert_eq!(
        source::parse_i64("OwnedValue(I64(1782436305258))"),
        1782436305258
    );
    assert_eq!(source::parse_i64("-42"), -42);
    assert_eq!(source::parse_u32("OwnedValue(U32(17))"), 17);
    assert_eq!(source::parse_u32("13261"), 13261);
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}
