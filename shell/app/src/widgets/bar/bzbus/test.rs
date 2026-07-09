use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

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
fn invocation_from_properties_decodes_bzbus_dbus_payload() {
    let path = OwnedObjectPath::try_from(
        "/com/snap/BzBus/invocations/invocation_4c4bf6a1_4e5b_4fd8_b6a0_c03d50ed1b33",
    )
    .unwrap();
    let properties = properties(&[
        ("Id", value("invocation-live")),
        ("BuildId", value("build-1")),
        ("Component", value("TOOL")),
        ("Source", value("bes")),
        ("LastObservedSequenceNumber", value(99i64)),
        ("Status", value("running")),
        ("Outcome", value("unknown")),
        ("CommandName", value("run")),
        ("ProgressCompleted", value(72_427u32)),
        ("ProgressTotal", value(72_435u32)),
        ("ActionsCompleted", value(8u32)),
        ("TotalActions", value(12i64)),
        ("ActionsFailed", value(1u32)),
        ("RunningActions", value(3u32)),
        ("StartedAtUnixMs", value(1_783_556_374_071i64)),
        ("EndedAtUnixMs", value(0i64)),
    ]);

    let invocation = source::invocation_from_properties(&path, &properties);

    assert_eq!(invocation.id, "invocation-live");
    assert_eq!(invocation.build_id, "build-1");
    assert_eq!(invocation.component, "TOOL");
    assert_eq!(invocation.source, "bes");
    assert_eq!(invocation.last_sequence, 99);
    assert_eq!(invocation.status, "running");
    assert_eq!(invocation.outcome, "unknown");
    assert_eq!(invocation.command_name, "run");
    assert_eq!(invocation.progress_completed, 72_427);
    assert_eq!(invocation.progress_total, 72_435);
    assert_eq!(invocation.actions_completed, 8);
    assert_eq!(invocation.total_actions, 12);
    assert_eq!(invocation.actions_failed, 1);
    assert_eq!(invocation.running_actions, 3);
    assert_eq!(invocation.started_at_unix_ms, 1_783_556_374_071);
    assert_eq!(invocation.ended_at_unix_ms, 0);
}

#[test]
fn invocation_uses_path_id_when_id_property_is_missing() {
    let path = OwnedObjectPath::try_from(
        "/com/snap/BzBus/invocations/invocation_fd898886_c248_437c_8a1a_5d374dc68888",
    )
    .unwrap();

    let invocation = source::invocation_from_properties(&path, &HashMap::new());

    assert_eq!(
        invocation.id,
        "invocation_fd898886_c248_437c_8a1a_5d374dc68888"
    );
}

#[test]
fn apply_invocation_properties_updates_existing_invocation() {
    let mut invocation = Invocation {
        id: "existing".to_owned(),
        status: "queued".to_owned(),
        progress_completed: 1,
        progress_total: 4,
        total_actions: 0,
        ..Invocation::default()
    };
    let changed = properties(&[
        ("Status", value("running")),
        ("ProgressCompleted", value(3u32)),
        ("TotalActions", value(9i64)),
    ]);

    source::apply_invocation_properties(&mut invocation, &changed);

    assert_eq!(invocation.id, "existing");
    assert_eq!(invocation.status, "running");
    assert_eq!(invocation.progress_completed, 3);
    assert_eq!(invocation.progress_total, 4);
    assert_eq!(invocation.total_actions, 9);
}

#[test]
fn numeric_values_accept_bzbus_integer_shapes() {
    assert_eq!(
        source::i64_value(&value(1_782_436_305_258i64)),
        Some(1_782_436_305_258)
    );
    assert_eq!(source::i64_value(&value(17u32)), Some(17));
    assert_eq!(source::u32_value(&value(17u32)), Some(17));
    assert_eq!(source::u32_value(&value(17i64)), Some(17));
    assert_eq!(source::u64_value(&value(13261i64)), Some(13261));
}

fn properties(entries: &[(&str, OwnedValue)]) -> HashMap<String, OwnedValue> {
    entries
        .iter()
        .map(|(name, value)| (name.to_string(), value.try_clone().unwrap()))
        .collect()
}

fn value<T>(value: T) -> OwnedValue
where
    Value<'static>: From<T>,
{
    OwnedValue::try_from(Value::from(value)).unwrap()
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}
