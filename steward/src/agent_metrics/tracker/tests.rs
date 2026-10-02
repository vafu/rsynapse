use super::*;
use std::time::Duration;
fn session(state: &str) -> Session {
    Session {
        key: "session-a".to_owned(),
        session_id: "a".to_owned(),
        agent: "codex".to_owned(),
        state: state.to_owned(),
        window: Some(10),
        project: "project".to_owned(),
        workspace: "workspace".to_owned(),
        ..Session::default()
    }
}
fn snapshot(session: Session) -> Snapshot {
    Snapshot {
        sessions: vec![session],
        ..Snapshot::default()
    }
}
fn key(suffix: &str) -> String {
    format!("agents.project.project.agent.codex.role.root.{suffix}")
}

#[test]
fn state_durations_and_complete_busy_cycle_are_event_driven() {
    let start = Instant::now();
    let mut t = AgentTracker::new();
    t.update(snapshot(session("idle")), start);
    t.drain();
    t.update(
        snapshot(session("thinking")),
        start + Duration::from_secs(2),
    );
    assert_eq!(t.drain().get(&key("state.idle.seconds")), Some(&2.0));
    t.update(
        snapshot(session("tool-use")),
        start + Duration::from_secs(5),
    );
    assert_eq!(t.drain().get(&key("state.thinking.seconds")), Some(&3.0));
    t.update(snapshot(session("idle")), start + Duration::from_secs(9));
    let event = t.drain();
    assert_eq!(event.get(&key("state.tool-use.seconds")), Some(&4.0));
    assert_eq!(event.get(&key("work_cycles.seconds_sum")), Some(&7.0));
    assert_eq!(event.get(&key("responses.completed.count")), Some(&1.0));
}

#[test]
fn reaction_latency_excludes_locked_idle_time_in_available_metric() {
    let start = Instant::now();
    let mut t = AgentTracker::new();
    t.focus(Some(20), true, start);
    t.update(snapshot(session("thinking")), start);
    t.drain();
    t.update(snapshot(session("idle")), start + Duration::from_secs(5));
    t.drain();
    t.focus(Some(10), false, start + Duration::from_secs(8));
    t.drain();
    t.focus(Some(10), true, start + Duration::from_secs(18));
    let read = t.drain();
    assert_eq!(read.get(&key("response_latency.seconds_sum")), Some(&13.0));
    assert_eq!(
        read.get(&key("response_latency.available_seconds_sum")),
        Some(&3.0)
    );
    assert_eq!(read.get(&key("response_latency.count")), Some(&1.0));
    t.focus(Some(10), true, start + Duration::from_secs(20));
    assert!(t.drain().is_empty());
}

#[test]
fn already_watching_gets_zero_latency_but_startup_idle_is_not_a_completion() {
    let start = Instant::now();
    let mut t = AgentTracker::new();
    t.focus(Some(10), true, start);
    t.update(snapshot(session("idle")), start);
    assert!(!t.drain().contains_key(&key("response_latency.count")));
    t.update(
        snapshot(session("thinking")),
        start + Duration::from_secs(1),
    );
    t.drain();
    t.update(snapshot(session("idle")), start + Duration::from_secs(4));
    let read = t.drain();
    assert_eq!(read.get(&key("response_latency.seconds_sum")), Some(&0.0));
    assert_eq!(read.get(&key("responses.read.count")), Some(&1.0));
}

#[test]
fn selected_session_link_disambiguates_shared_terminal_windows() {
    let start = Instant::now();
    let mut t = AgentTracker::new();
    t.focus(Some(10), true, start);
    let mut s = snapshot(session("thinking"));
    s.active_links.insert(10, "another-session".to_owned());
    t.update(s.clone(), start);
    t.drain();
    s.sessions[0].state = "idle".to_owned();
    t.update(s.clone(), start + Duration::from_secs(1));
    assert!(!t.drain().contains_key(&key("response_latency.count")));
    s.active_links.insert(10, "session-a".to_owned());
    t.update(s, start + Duration::from_secs(3));
    assert_eq!(
        t.drain().get(&key("response_latency.seconds_sum")),
        Some(&2.0)
    );
}

#[test]
fn session_count_baseline_does_not_recount_existing_sessions() {
    let start = Instant::now();
    let mut t = AgentTracker::new();
    t.update(snapshot(session("idle")), start);
    let initial = t.drain();
    assert_eq!(initial.get(&key("sessions.live.state")), Some(&1.0));
    assert!(!initial.contains_key(&key("sessions.started.count")));
    t.update(Snapshot::default(), start + Duration::from_secs(2));
    assert_eq!(t.drain().get(&key("sessions.live.state")), Some(&0.0));
    t.update(snapshot(session("idle")), start + Duration::from_secs(3));
    assert!(!t.drain().contains_key(&key("sessions.started.count")));
    let mut new = session("idle");
    new.key = "new".to_owned();
    t.update(snapshot(new), start + Duration::from_secs(4));
    assert_eq!(t.drain().get(&key("sessions.started.count")), Some(&1.0));
}

#[test]
fn superseded_unread_response_is_cancelled_not_fake_read() {
    let start = Instant::now();
    let mut t = AgentTracker::new();
    t.update(snapshot(session("thinking")), start);
    t.drain();
    t.update(snapshot(session("idle")), start + Duration::from_secs(1));
    t.drain();
    t.update(
        snapshot(session("thinking")),
        start + Duration::from_secs(2),
    );
    let event = t.drain();
    assert_eq!(
        event.get(&key("responses.unread_cancelled.count")),
        Some(&1.0)
    );
    assert!(!event.contains_key(&key("response_latency.count")));
}

#[test]
fn subagents_get_state_time_but_not_human_read_latency() {
    let start = Instant::now();
    let mut t = AgentTracker::new();
    t.focus(Some(10), true, start);
    let mut s = session("thinking");
    s.subagent = true;
    t.update(snapshot(s.clone()), start);
    t.drain();
    s.state = "idle".to_owned();
    t.update(snapshot(s), start + Duration::from_secs(3));
    let event = t.drain();
    assert_eq!(
        event.get("agents.project.project.agent.codex.role.subagent.state.thinking.seconds"),
        Some(&3.0)
    );
    assert!(!event.keys().any(|name| name.contains("response_latency")));
}

#[test]
fn unchanged_snapshot_does_not_emit_or_reset_state_duration() {
    let start = Instant::now();
    let mut t = AgentTracker::new();
    t.update(snapshot(session("thinking")), start);
    t.drain();
    t.update(
        snapshot(session("thinking")),
        start + Duration::from_secs(5),
    );
    assert!(t.drain().is_empty());
    t.finish(start + Duration::from_secs(10));
    assert_eq!(t.drain().get(&key("state.thinking.seconds")), Some(&10.0));
}
