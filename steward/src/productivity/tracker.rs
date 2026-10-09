use super::{Classification, Policy, SUMMARY};
use crate::{
    agent_metrics::Snapshot,
    focus::FocusState,
    relations::LocusClient,
    workdays::{date, next_midnight},
};
use locus::RelationEndpoint;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub const METRICS: [&str; 9] = [
    "background_idle",
    "goal_background_idle",
    "personal_active",
    "personal_idle",
    "personal_active_with_background_work",
    "personal_idle_with_background_work",
    "work_while_locked",
    "unclassified_active",
    "idle_agents_with_idle_human",
];

#[derive(Clone, Default, Serialize, Deserialize)]
struct Day {
    seconds: BTreeMap<String, f64>,
    checkpoint: f64,
}

#[derive(Default)]
pub struct Tracker {
    days: BTreeMap<String, Day>,
    dirty: BTreeSet<String>,
    since: Option<f64>,
    focus: Option<FocusState>,
    agents: Snapshot,
    policy: Option<Policy>,
    gauges: HashMap<String, f64>,
}

pub fn now() -> f64 {
    chrono::Utc::now().timestamp_millis() as f64 / 1000.0
}

impl Tracker {
    pub async fn load(locus: &LocusClient) -> anyhow::Result<Self> {
        let mut tracker = Self::default();
        for record in locus.list(SUMMARY).await? {
            if let RelationEndpoint::StableKey { kind, id } = record.subject {
                if kind != "org.rsynapse.calendar-day"
                    || chrono::NaiveDate::parse_from_str(&id, "%Y-%m-%d").is_err()
                {
                    continue;
                }
                if let Some(json) = record.metadata.get("summary") {
                    if let Ok(day) = serde_json::from_str(json) {
                        tracker.days.insert(id.clone(), day);
                        tracker.dirty.insert(id);
                    }
                }
            }
        }
        Ok(tracker)
    }

    fn flags(&self, day: &str) -> [bool; 9] {
        let (Some(focus), Some(policy)) = (&self.focus, &self.policy) else {
            return [false; 9];
        };
        let class = policy.human_class(focus);
        let mut background = false;
        let mut goal_background = false;
        let mut work_busy = false;
        let mut background_idle_agent = false;
        for session in &self.agents.sessions {
            let focused = focus.window_id == session.window
                && session.window.is_some()
                && session.window.is_some_and(|window| {
                    self.agents
                        .active_links
                        .get(&window)
                        .is_none_or(|key| key == &session.key)
                });
            if policy.eligible_agent(session, day) {
                work_busy = true;
                if !focused && focus.window_id.is_some() {
                    background = true;
                    goal_background |= policy.agent_goals(session, day);
                }
            }
            if session.state == "idle"
                && !session.subagent
                && !focused
                && session.window.is_some()
                && focus.window_id.is_some()
            {
                // Reuse the context eligibility rules without granting idle any work credit.
                let mut candidate = session.clone();
                candidate.state = "thinking".to_owned();
                background_idle_agent |= policy.eligible_agent(&candidate, day);
            }
        }
        let personal = class == Classification::Personal && !focus.locked;
        let idle = focus.idle && !focus.locked;
        [
            idle && background,
            idle && goal_background,
            personal && !focus.idle,
            personal && focus.idle,
            personal && !focus.idle && background,
            personal && focus.idle && background,
            focus.locked && work_busy,
            !focus.locked && !focus.idle && class == Classification::Unclassified,
            idle && background_idle_agent && !background,
        ]
    }

    fn advance(&mut self, timestamp: f64) {
        if self.focus.is_some() && self.policy.is_some() {
            if let Some(mut cursor) = self.since {
                while cursor < timestamp {
                    let end = next_midnight(cursor).min(timestamp);
                    let key = date(cursor);
                    let flags = self.flags(&key);
                    let day = self.days.entry(key.clone()).or_default();
                    for (metric, enabled) in METRICS.iter().zip(flags) {
                        if enabled {
                            *day.seconds.entry((*metric).to_owned()).or_default() += end - cursor;
                        }
                    }
                    day.checkpoint = end;
                    self.dirty.insert(key);
                    cursor = end;
                }
            }
        }
        self.since = Some(timestamp);
    }
    pub fn focus(&mut self, focus: FocusState, timestamp: f64) {
        self.advance(timestamp);
        self.focus = Some(focus);
    }
    pub fn agents(&mut self, agents: Snapshot, timestamp: f64) {
        self.advance(timestamp);
        self.agents = agents;
    }
    pub fn policy(&mut self, policy: Policy, timestamp: f64) {
        self.advance(timestamp);
        self.policy = Some(policy);
    }
    pub fn finish(&mut self, timestamp: f64) {
        self.advance(timestamp);
        self.focus = None;
    }

    pub async fn publish(&mut self, locus: &LocusClient) -> anyhow::Result<HashMap<String, f64>> {
        let timestamp = self.since.unwrap_or_else(now);
        let today = date(timestamp);
        let mut metrics = HashMap::new();
        if self.focus.is_some() && self.policy.is_some() && !self.days.contains_key(&today) {
            self.days.insert(
                today.clone(),
                Day {
                    checkpoint: timestamp,
                    ..Day::default()
                },
            );
            self.dirty.insert(today.clone());
        }
        let keys: Vec<_> = self.dirty.iter().cloned().collect();
        for key in keys {
            let day = &self.days[&key];
            locus
                .set_one_with_persistence(
                    RelationEndpoint::stable_key("org.rsynapse.calendar-day", &key),
                    SUMMARY,
                    RelationEndpoint::stable_key("org.rsynapse.productivity-components", &key),
                    HashMap::from([("summary".to_owned(), serde_json::to_string(day)?)]),
                    true,
                )
                .await?;
            for metric in METRICS {
                metrics.insert(
                    format!("productivity.{key}.{metric}_seconds.state"),
                    day.seconds.get(metric).copied().unwrap_or(0.0),
                );
            }
            self.dirty.remove(&key);
        }
        if self.focus.is_some() && self.policy.is_some() || self.days.contains_key(&today) {
            let flags = self.flags(&today);
            let day = self.days.entry(today.clone()).or_default();
            // Gauges describe intervals starting now. Today's live view adds time
            // since this checkpoint; shutdown makes every availability flag false.
            for (metric, enabled) in METRICS.iter().zip(flags) {
                metrics.insert(
                    format!("productivity.today.{metric}_seconds.state"),
                    day.seconds.get(*metric).copied().unwrap_or(0.0),
                );
                metrics.insert(
                    format!("productivity.today.{metric}_active.state"),
                    if enabled { 1.0 } else { 0.0 },
                );
            }
            metrics.insert("productivity.today.checkpoint.state".to_owned(), timestamp);
        }
        let mut next = HashMap::new();
        if let Some(policy) = &self.policy {
            for (workspace, class) in &policy.workspaces {
                next.insert(
                    format!(
                        "productivity.classification.workspace_name.{}.state",
                        crate::focus::sanitize(workspace)
                    ),
                    match class {
                        Classification::Unclassified => 0.0,
                        Classification::Work => 1.0,
                        Classification::Personal => 2.0,
                    },
                );
            }
        }
        for name in self.gauges.keys() {
            next.entry(name.clone()).or_insert(0.0);
        }
        for (name, value) in &next {
            if self.gauges.get(name) != Some(value) {
                metrics.insert(name.clone(), *value);
            }
        }
        self.gauges = next;
        Ok(metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Goal, Priority, Status};
    use super::*;
    use crate::agent_metrics::Session;
    fn start() -> f64 {
        chrono::Local::now()
            .date_naive()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_local_timezone(chrono::Local)
            .unwrap()
            .timestamp() as f64
    }
    fn session(id: &str, window: u64) -> Session {
        Session {
            key: id.into(),
            session_id: id.into(),
            agent: "codex".into(),
            state: "thinking".into(),
            cwd: "/work/src".into(),
            workspace: "work".into(),
            window: Some(window),
            ..Session::default()
        }
    }
    fn setup(timestamp: f64) -> Tracker {
        let mut tracker = Tracker::default();
        let policy = Policy {
            workspaces: HashMap::from([
                ("work".into(), Classification::Work),
                ("personal".into(), Classification::Personal),
            ]),
            ..Policy::default()
        };
        tracker.policy(policy, timestamp);
        tracker.focus(
            FocusState {
                idle: true,
                window_id: Some(1),
                workspace_name: Some("personal".into()),
                ..FocusState::default()
            },
            timestamp,
        );
        tracker
    }
    #[test]
    fn parallel_agents_are_wall_time_union_and_personal_overlap_is_explicit() {
        let t = start();
        let mut tracker = setup(t);
        tracker.agents(
            Snapshot {
                sessions: vec![session("a", 2), session("b", 3)],
                ..Snapshot::default()
            },
            t,
        );
        tracker.finish(t + 60.0);
        let values = &tracker.days[&date(t)].seconds;
        assert_eq!(values["background_idle"], 60.0);
        assert_eq!(values["personal_idle"], 60.0);
        assert_eq!(values["personal_idle_with_background_work"], 60.0);
    }
    #[test]
    fn focused_child_compacting_personal_and_unknown_agents_do_not_earn_credit() {
        let t = start();
        for variant in 0..5 {
            let mut tracker = setup(t);
            let mut s = session("a", 2);
            match variant {
                0 => s.window = Some(1),
                1 => s.subagent = true,
                2 => s.state = "compacting".into(),
                3 => s.workspace = "personal".into(),
                _ => s.workspace = "unknown".into(),
            }
            tracker.agents(
                Snapshot {
                    sessions: vec![s],
                    ..Snapshot::default()
                },
                t,
            );
            tracker.finish(t + 60.0);
            assert_eq!(tracker.days[&date(t)].seconds.get("background_idle"), None);
        }
    }
    #[test]
    fn shared_terminal_link_distinguishes_focused_sessions_and_lock_is_separate() {
        let t = start();
        let mut tracker = setup(t);
        tracker.agents(
            Snapshot {
                sessions: vec![session("a", 1)],
                active_links: HashMap::from([(1, "other".into())]),
            },
            t,
        );
        tracker.focus(
            FocusState {
                locked: true,
                ..FocusState::default()
            },
            t + 30.0,
        );
        tracker.finish(t + 60.0);
        let values = &tracker.days[&date(t)].seconds;
        assert_eq!(values["background_idle"], 30.0);
        assert_eq!(values["work_while_locked"], 30.0);
        assert_eq!(values["personal_idle"], 30.0);
    }
    #[test]
    fn goals_expire_at_midnight_and_never_override_personal_classification() {
        let t = next_midnight(start()) - 30.0;
        let mut tracker = setup(t);
        tracker.policy.as_mut().unwrap().goals.push(Goal {
            id: "goal1".into(),
            date: date(t),
            title: "title".into(),
            success: "criterion".into(),
            kind: super::super::GoalType::Outcome,
            project: Some("/work".into()),
            priority: Priority::High,
            status: Status::InProgress,
        });
        let mut s = session("a", 2);
        s.workspace = "unknown".into();
        tracker.agents(
            Snapshot {
                sessions: vec![s.clone()],
                ..Snapshot::default()
            },
            t,
        );
        tracker.finish(t + 60.0);
        assert_eq!(tracker.days[&date(t)].seconds["goal_background_idle"], 30.0);
        assert_eq!(
            tracker.days[&date(t + 60.0)].seconds.get("background_idle"),
            None
        );
        s.workspace = "personal".into();
        assert!(
            !tracker
                .policy
                .as_ref()
                .unwrap()
                .eligible_agent(&s, &date(t))
        );
    }
    #[test]
    fn classification_changes_are_not_retroactive_and_restarts_do_not_backfill() {
        let t = start();
        let mut tracker = setup(t);
        tracker.focus(
            FocusState {
                idle: false,
                window_id: Some(1),
                workspace_name: Some("personal".into()),
                ..FocusState::default()
            },
            t,
        );
        let mut policy = tracker.policy.clone().unwrap();
        policy
            .workspaces
            .insert("personal".into(), Classification::Work);
        tracker.policy(policy.clone(), t + 20.0);
        tracker.finish(t + 30.0);
        assert_eq!(tracker.days[&date(t)].seconds["personal_active"], 20.0);
        let json = serde_json::to_string(&tracker.days).unwrap();
        let mut restored = Tracker {
            days: serde_json::from_str(&json).unwrap(),
            ..Tracker::default()
        };
        restored.policy(policy, t + 100.0);
        restored.focus(
            FocusState {
                workspace_name: Some("personal".into()),
                ..FocusState::default()
            },
            t + 100.0,
        );
        restored.finish(t + 120.0);
        assert_eq!(restored.days[&date(t)].seconds["personal_active"], 20.0);
    }
    #[test]
    fn idle_agent_time_is_diagnostic_not_productive() {
        let t = start();
        let mut tracker = setup(t);
        let mut s = session("a", 2);
        s.state = "idle".into();
        tracker.agents(
            Snapshot {
                sessions: vec![s],
                ..Snapshot::default()
            },
            t,
        );
        tracker.finish(t + 30.0);
        assert_eq!(
            tracker.days[&date(t)].seconds["idle_agents_with_idle_human"],
            30.0
        );
        assert_eq!(tracker.days[&date(t)].seconds.get("background_idle"), None);
    }
}
