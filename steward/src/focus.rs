use std::{collections::HashMap, time::Instant};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FocusState {
    pub locked: bool,
    pub idle: bool,
    pub workspace_id: Option<u64>,
    pub workspace_name: Option<String>,
    pub window_id: Option<u64>,
    pub app_id: Option<String>,
    pub project: Option<String>,
    pub output: Option<String>,
}

#[derive(Debug)]
struct Interval {
    metric: String,
    started: Instant,
}

/// Independent focus timers. Nothing is published until an interval ends.
/// A window switch ends the window/app intervals, not the workspace interval.
/// Lock and shutdown end every open interval. No heartbeat or duration cap.
/// Idle changes split every focus interval without suspending attribution.
#[derive(Debug, Default)]
pub struct Tracker {
    current: FocusState,
    intervals: HashMap<&'static str, Interval>,
    pending: HashMap<String, f64>,
}

impl Tracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, state: FocusState, now: Instant) {
        let (activity, code) = if state.locked {
            ("locked", 2.0)
        } else if state.idle {
            ("idle", 0.0)
        } else {
            ("active", 1.0)
        };
        let metric = format!("activity.{activity}.seconds");
        if self.intervals.get("activity").map(|i| i.metric.as_str()) != Some(metric.as_str()) {
            self.end("activity", now);
            self.intervals.insert(
                "activity",
                Interval {
                    metric,
                    started: now,
                },
            );
            self.pending.insert("activity.state".to_owned(), code);
        }
        let workspace_changed = self.current.workspace_id != state.workspace_id;
        let window_changed = self.current.window_id != state.window_id;
        let transition = !self.current.locked && !state.locked;
        if transition {
            for (dimension, before, after, forced) in [
                (
                    "workspace",
                    self.current.workspace_id.map(|id| id.to_string()),
                    state.workspace_id.map(|id| id.to_string()),
                    false,
                ),
                (
                    "window",
                    self.current.window_id.map(|id| id.to_string()),
                    state.window_id.map(|id| id.to_string()),
                    false,
                ),
                (
                    "app",
                    self.current.app_id.clone(),
                    state.app_id.clone(),
                    window_changed,
                ),
                (
                    "project",
                    self.current.project.clone(),
                    state.project.clone(),
                    workspace_changed,
                ),
                (
                    "output",
                    self.current.output.clone(),
                    state.output.clone(),
                    false,
                ),
            ] {
                // Initial selection and unlock start intervals, not switches.
                let switched = match dimension {
                    "app" => window_changed,
                    "project" => workspace_changed,
                    _ => before != after || forced,
                };
                if before.is_some() && after.is_some() && switched {
                    *self
                        .pending
                        .entry(format!("switches.{dimension}"))
                        .or_default() += 1.0;
                }
            }
        }
        for (dimension, value, forced) in [
            (
                "workspace",
                state.workspace_id.map(|id| id.to_string()),
                workspace_changed,
            ),
            (
                "workspace_name",
                state.workspace_name.clone(),
                workspace_changed,
            ),
            (
                "window",
                state.window_id.map(|id| id.to_string()),
                window_changed,
            ),
            ("app", state.app_id.clone(), window_changed),
            ("project", state.project.clone(), workspace_changed),
            ("output", state.output.clone(), false),
        ] {
            let next = if state.locked {
                None
            } else {
                value.map(|v| {
                    format!(
                        "focus.{dimension}.{}.idle.{}.seconds",
                        sanitize(&v),
                        state.idle
                    )
                })
            };
            let changed = forced
                || self.intervals.get(dimension).map(|i| i.metric.as_str()) != next.as_deref();
            if changed {
                self.end(dimension, now);
                if let Some(metric) = next {
                    self.intervals.insert(
                        dimension,
                        Interval {
                            metric,
                            started: now,
                        },
                    );
                }
            }
        }
        self.current = state;
    }

    pub fn finish(&mut self, now: Instant) {
        for dimension in [
            "workspace",
            "workspace_name",
            "window",
            "app",
            "project",
            "output",
            "activity",
        ] {
            self.end(dimension, now);
        }
    }

    pub fn drain(&mut self) -> HashMap<String, f64> {
        std::mem::take(&mut self.pending)
    }

    fn end(&mut self, dimension: &str, now: Instant) {
        if let Some(interval) = self.intervals.remove(dimension) {
            let elapsed = now.duration_since(interval.started).as_secs_f64();
            // Active/idle totals derive from the attributed focus series.
            // Locked duration is separate because locked time has no focus.
            if elapsed > 0.0
                && (dimension != "activity" || interval.metric == "activity.locked.seconds")
            {
                *self.pending.entry(interval.metric).or_default() += elapsed;
            }
        }
    }
}

/// Graphite replaces points sharing a storage slot. On each incoming event,
/// immediately publish the updated sum for that slot so rapid switches are
/// not overwritten. This is event-triggered aggregation, never a timer.
#[derive(Default)]
pub struct GraphiteSlots {
    timestamp: Option<u64>,
    totals: HashMap<String, f64>,
}

impl GraphiteSlots {
    pub fn event(&mut self, batch: Vec<(String, f64, u64)>) -> Vec<(String, f64, u64)> {
        batch
            .into_iter()
            .map(|(name, value, timestamp)| {
                let timestamp = timestamp / 10 * 10; // Graphite's configured 10s storage resolution.
                if self.timestamp != Some(timestamp) {
                    self.timestamp = Some(timestamp);
                    self.totals.clear();
                }
                let total = self.totals.entry(name.clone()).or_default();
                if name.ends_with(".state") {
                    *total = value;
                } else {
                    *total += value;
                }
                (name, *total, timestamp)
            })
            .collect()
    }
}

pub fn canonical_app_id(app_id: &str) -> String {
    APP_ALIASES
        .iter()
        .find(|(from, _)| *from == app_id)
        .map(|(_, to)| (*to).to_owned())
        .unwrap_or_else(|| {
            APP_PREFIXES
                .iter()
                .find(|(prefix, _)| app_id.starts_with(*prefix))
                .map(|(_, to)| (*to).to_owned())
                .unwrap_or_else(|| app_id.to_owned())
        })
}

const APP_ALIASES: &[(&str, &str)] = &[
    ("com.mitchellh.ghostty", "ghostty"),
    ("org.gnome.Terminal", "terminal"),
    ("org.wezfurlong.wezterm", "wezterm"),
    ("google-chrome", "chrome"),
];
const APP_PREFIXES: &[(&str, &str)] = &[("chrome-", "chrome"), ("firefox-", "firefox")];

pub fn sanitize(value: &str) -> String {
    let mut cleaned = String::with_capacity(value.len());
    let mut last_underscore = false;
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' {
            cleaned.push(byte as char);
            last_underscore = false;
        } else if !last_underscore && !cleaned.is_empty() {
            cleaned.push('_');
            last_underscore = true;
        }
    }
    if cleaned.is_empty() {
        "unknown".to_owned()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn state(ws: u64, window: u64, app: &str) -> FocusState {
        FocusState {
            workspace_id: Some(ws),
            workspace_name: Some(format!("ws-{ws}")),
            window_id: Some(window),
            app_id: Some(app.to_owned()),
            project: Some(format!("project-{ws}")),
            output: Some("DP-2".to_owned()),
            locked: false,
            idle: false,
        }
    }

    #[test]
    fn window_switch_publishes_app_duration_but_keeps_workspace_timer() {
        let start = Instant::now();
        let mut t = Tracker::new();
        t.observe(state(1, 10, "codex"), start);
        assert_eq!(t.drain().get("activity.state"), Some(&1.0));
        t.observe(state(1, 11, "neovim"), start + Duration::from_secs(6));
        let event = t.drain();
        assert_eq!(event.get("focus.app.codex.idle.false.seconds"), Some(&6.0));
        assert_eq!(event.get("switches.window"), Some(&1.0));
        assert_eq!(event.get("switches.app"), Some(&1.0));
        assert!(!event.contains_key("focus.workspace.1.idle.false.seconds"));
        assert!(!event.contains_key("switches.workspace"));
        t.observe(state(2, 12, "opencode"), start + Duration::from_secs(10));
        let event = t.drain();
        assert_eq!(
            event.get("focus.workspace.1.idle.false.seconds"),
            Some(&10.0)
        );
        assert_eq!(event.get("focus.app.neovim.idle.false.seconds"), Some(&4.0));
        assert_eq!(event.get("switches.workspace"), Some(&1.0));
    }

    #[test]
    fn same_app_in_new_window_is_a_new_app_focus_interval() {
        let start = Instant::now();
        let mut t = Tracker::new();
        t.observe(state(1, 10, "codex"), start);
        t.observe(state(1, 11, "codex"), start + Duration::from_secs(3));
        assert_eq!(
            t.drain().get("focus.app.codex.idle.false.seconds"),
            Some(&3.0)
        );
    }

    #[test]
    fn no_event_means_no_report_and_long_intervals_are_not_capped() {
        let start = Instant::now();
        let mut t = Tracker::new();
        t.observe(state(1, 10, "codex"), start);
        t.drain();
        t.observe(state(1, 10, "codex"), start + Duration::from_secs(600));
        assert!(t.drain().is_empty());
        t.finish(start + Duration::from_secs(1800));
        assert_eq!(
            t.drain().get("focus.app.codex.idle.false.seconds"),
            Some(&1800.0)
        );
    }

    #[test]
    fn lock_closes_timers_unlock_does_not_count_switches() {
        let start = Instant::now();
        let mut t = Tracker::new();
        t.observe(state(1, 10, "codex"), start);
        t.observe(
            FocusState {
                locked: true,
                ..FocusState::default()
            },
            start + Duration::from_secs(5),
        );
        let event = t.drain();
        assert_eq!(
            event.get("focus.workspace.1.idle.false.seconds"),
            Some(&5.0)
        );
        assert!(!event.contains_key("switches.workspace"));
        t.observe(state(2, 11, "neovim"), start + Duration::from_secs(605));
        let resumed = t.drain();
        assert_eq!(resumed.get("activity.locked.seconds"), Some(&600.0));
        assert!(!resumed.contains_key("switches.workspace"));
        t.finish(start + Duration::from_secs(610));
        assert_eq!(
            t.drain().get("focus.app.neovim.idle.false.seconds"),
            Some(&5.0)
        );
    }

    #[test]
    fn rapid_events_in_same_graphite_slot_preserve_sums_without_waiting() {
        let mut s = GraphiteSlots::default();
        let metric = "switches.window".to_owned();
        assert_eq!(
            s.event(vec![(metric.clone(), 1.0, 101)]),
            vec![(metric.clone(), 1.0, 100)]
        );
        assert_eq!(
            s.event(vec![(metric.clone(), 1.0, 102)]),
            vec![(metric.clone(), 2.0, 100)]
        );
        assert_eq!(
            s.event(vec![(metric.clone(), 1.0, 111)]),
            vec![(metric, 1.0, 110)]
        );
    }

    #[test]
    fn idle_splits_and_keeps_attributing_all_focus_dimensions() {
        let start = Instant::now();
        let mut t = Tracker::new();
        t.observe(state(1, 10, "codex"), start);
        t.drain();
        t.observe(
            FocusState {
                idle: true,
                ..state(1, 10, "codex")
            },
            start + Duration::from_secs(30),
        );
        let idle = t.drain();
        assert!(!idle.contains_key("activity.active.seconds"));
        assert_eq!(idle.get("activity.state"), Some(&0.0));
        assert_eq!(idle.get("focus.app.codex.idle.false.seconds"), Some(&30.0));
        assert!(!idle.contains_key("switches.workspace"));
        assert!(!idle.contains_key("switches.app"));
        t.observe(
            FocusState {
                idle: true,
                ..state(1, 10, "codex")
            },
            start + Duration::from_secs(60),
        );
        assert!(t.drain().is_empty());
        t.observe(
            FocusState {
                locked: true,
                idle: true,
                ..FocusState::default()
            },
            start + Duration::from_secs(90),
        );
        let locked = t.drain();
        assert!(!locked.contains_key("activity.idle.seconds"));
        for metric in [
            "focus.workspace.1.idle.true.seconds",
            "focus.workspace_name.ws-1.idle.true.seconds",
            "focus.window.10.idle.true.seconds",
            "focus.app.codex.idle.true.seconds",
            "focus.project.project-1.idle.true.seconds",
            "focus.output.DP-2.idle.true.seconds",
        ] {
            assert_eq!(locked.get(metric), Some(&60.0), "{metric}");
        }
        assert_eq!(locked.get("activity.state"), Some(&2.0));
        t.observe(state(2, 20, "neovim"), start + Duration::from_secs(690));
        let resumed = t.drain();
        assert_eq!(resumed.get("activity.locked.seconds"), Some(&600.0));
        assert_eq!(resumed.get("activity.state"), Some(&1.0));
        assert!(!resumed.contains_key("switches.workspace"));
        assert!(!resumed.contains_key("focus.app.codex.idle.true.seconds"));
    }

    #[test]
    fn input_resuming_ends_idle_interval_without_counting_a_focus_switch() {
        let start = Instant::now();
        let mut t = Tracker::new();
        t.observe(
            FocusState {
                idle: true,
                ..state(1, 10, "codex")
            },
            start,
        );
        t.drain();
        t.observe(state(1, 10, "codex"), start + Duration::from_secs(10));
        let event = t.drain();
        assert_eq!(
            event.get("focus.workspace.1.idle.true.seconds"),
            Some(&10.0)
        );
        assert_eq!(event.get("focus.app.codex.idle.true.seconds"), Some(&10.0));
        assert!(!event.keys().any(|name| name.starts_with("switches.")));
        t.finish(start + Duration::from_secs(15));
        assert_eq!(
            t.drain().get("focus.app.codex.idle.false.seconds"),
            Some(&5.0)
        );
    }

    #[test]
    fn real_workspace_and_window_switches_while_idle_are_still_counted() {
        let start = Instant::now();
        let mut t = Tracker::new();
        t.observe(
            FocusState {
                idle: true,
                ..state(1, 10, "codex")
            },
            start,
        );
        t.drain();
        t.observe(
            FocusState {
                idle: true,
                ..state(2, 20, "neovim")
            },
            start + Duration::from_secs(10),
        );
        let event = t.drain();
        assert_eq!(event.get("focus.app.codex.idle.true.seconds"), Some(&10.0));
        assert_eq!(event.get("switches.workspace"), Some(&1.0));
        assert_eq!(event.get("switches.window"), Some(&1.0));
    }

    #[test]
    fn activity_gauge_uses_last_value_not_sum_in_same_storage_slot() {
        let mut slots = GraphiteSlots::default();
        let name = "rsynapse.activity.state".to_owned();
        slots.event(vec![(name.clone(), 1.0, 100)]);
        assert_eq!(
            slots.event(vec![(name.clone(), 2.0, 101)]),
            vec![(name.clone(), 2.0, 100)]
        );
        assert_eq!(
            slots.event(vec![(name.clone(), 0.0, 102)]),
            vec![(name, 0.0, 100)]
        );
    }

    #[test]
    fn app_names_are_canonical_and_graphite_safe() {
        assert_eq!(canonical_app_id("com.mitchellh.ghostty"), "ghostty");
        assert_eq!(canonical_app_id("google-chrome"), "chrome");
        assert_eq!(sanitize("a.b"), "a_b");
        assert_eq!(sanitize(""), "unknown");
    }
}
