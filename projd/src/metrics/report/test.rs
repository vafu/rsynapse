use super::*;
use proj_model::GoalInfo;

fn identity() -> GoalIdentityInfo {
    GoalIdentityInfo {
        instance_id: "i".into(),
        key: "g".into(),
        goal: GoalInfo {
            id: "g".into(),
            title: "Goal".into(),
            status: "completed".into(),
            ..Default::default()
        },
        project_name: "Project".into(),
        created_at_ms: 100,
        observed_at_ms: 100,
        deleted_at_ms: 0,
        ancestor_ids: vec![],
    }
}
fn event(at: u64, kind: &str, old: &str, status: &str) -> GoalEventInfo {
    GoalEventInfo {
        sequence: at,
        at_ms: at,
        kind: kind.into(),
        instance_id: "i".into(),
        key: "g".into(),
        old_status: old.into(),
        goal: GoalInfo {
            status: status.into(),
            ..Default::default()
        },
    }
}
fn work(kind: WorkKind, start: u64, end: u64, duration: u64) -> WorkSessionInfo {
    WorkSessionInfo {
        id: "s".into(),
        instance_id: "i".into(),
        goal_key: "g".into(),
        goal_title: "Goal".into(),
        project_id: "p".into(),
        project_name: "Project".into(),
        ancestor_ids: vec![],
        source: "test".into(),
        kind,
        started_at_ms: start,
        ended_at_ms: end,
        duration_ms: duration,
        open: false,
        end_reason: "changed".into(),
    }
}
#[test]
fn effort_status_elapsed_and_completion_cycles_are_distinct() {
    let events = vec![
        event(100, "created", "", "planned"),
        event(200, "status-changed", "planned", "in-progress"),
        event(800, "status-changed", "in-progress", "completed"),
        event(900, "status-changed", "completed", "in-progress"),
        event(1200, "status-changed", "in-progress", "completed"),
    ];
    let sessions = vec![
        work(WorkKind::Active, 300, 500, 200),
        work(WorkKind::Idle, 500, 600, 100),
        work(WorkKind::Agent, 200, 800, 600),
    ];
    let s = stats(identity(), &events, &sessions, 0, 1500);
    assert_eq!((s.active_ms, s.idle_ms, s.agent_ms), (200, 100, 600));
    assert_eq!(
        (
            s.in_progress_ms,
            s.lead_time_ms,
            s.cycle_time_ms,
            s.completion_count
        ),
        (900, 1100, 1000, 2)
    );
    assert_eq!(
        stats(identity(), &events, &[], 400, 1000).in_progress_ms,
        500
    );
}
#[test]
fn legacy_observation_does_not_invent_start_or_creation_times() {
    let mut i = identity();
    i.created_at_ms = 0;
    i.observed_at_ms = 500;
    let events = vec![
        event(500, "observed", "", "in-progress"),
        event(900, "status-changed", "in-progress", "completed"),
    ];
    let s = stats(i, &events, &[], 0, 1000);
    assert!(s.history_partial);
    assert_eq!(s.first_started_at_ms, 0);
    assert_eq!(s.lead_time_ms, 0);
    assert_eq!(s.in_progress_ms, 400);
}

#[test]
fn reopening_a_legacy_task_does_not_turn_the_next_recorded_start_into_its_first_start() {
    let mut i = identity();
    i.created_at_ms = 0;
    i.observed_at_ms = 500;
    let events = vec![
        event(500, "observed", "", "in-progress"),
        event(600, "status-changed", "in-progress", "completed"),
        event(700, "status-changed", "completed", "in-progress"),
        event(900, "status-changed", "in-progress", "completed"),
    ];
    let s = stats(i, &events, &[], 0, 1000);
    assert_eq!(s.first_started_at_ms, 0);
    assert_eq!(s.cycle_time_ms, 0);
    assert_eq!(s.completion_count, 2);
}
#[test]
fn range_clips_measured_spans_without_double_counting() {
    let s = clip(work(WorkKind::Active, 100, 500, 400), 200, 400).unwrap();
    assert_eq!(
        (s.started_at_ms, s.ended_at_ms, s.duration_ms),
        (200, 400, 200)
    );
    assert!(clip(work(WorkKind::Idle, 100, 200, 100), 300, 400).is_none());
}
