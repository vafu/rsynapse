use std::{collections::HashMap, time::Instant};

/// Attribution for one flush interval, in seconds.
pub const MAX_ATTRIBUTION_SECS: f64 = 120.0;

/// Focus state at a point in time. Only app/project names go into metric
/// paths; window titles and other content never leave the process.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FocusState {
    pub workspace_id: Option<u64>,
    pub workspace_name: Option<String>,
    pub window_id: Option<u64>,
    pub app_id: Option<String>,
    /// Inner session (editor, agent, …) from locus app-instance relations.
    /// Absent for plain windows; [`Self::app_id`] still covers those.
    pub session: Option<String>,
    pub project: Option<String>,
    pub output: Option<String>,
}

/// Folds focus states into per-bucket seconds plus a workspace-switch count.
/// Pure and synchronous: D-Bus resolution happens in the caller.
#[derive(Debug)]
pub struct Tracker {
    current: FocusState,
    last_change: Instant,
    pending: HashMap<String, f64>,
}

impl Tracker {
    pub fn new(now: Instant) -> Self {
        Self {
            current: FocusState::default(),
            last_change: now,
            pending: HashMap::new(),
        }
    }

    /// Attribute time since the last call to the previous state, then adopt
    /// the new state. Workspace changes bump the switch counter.
    pub fn observe(&mut self, state: FocusState, now: Instant) {
        let elapsed = self.elapsed(now);
        self.attribute(&self.current.clone(), elapsed);
        if state.workspace_id != self.current.workspace_id {
            *self.pending.entry("switches.workspace".to_owned()).or_insert(0.0) += 1.0;
        }
        self.current = state;
        self.last_change = now;
    }

    /// Attribute time since the last call without changing state.
    pub fn heartbeat(&mut self, now: Instant) {
        let elapsed = self.elapsed(now);
        self.attribute(&self.current.clone(), elapsed);
        self.last_change = now;
    }

    pub fn drain(&mut self) -> HashMap<String, f64> {
        std::mem::take(&mut self.pending)
    }

    fn elapsed(&self, now: Instant) -> f64 {
        now.duration_since(self.last_change).as_secs_f64()
    }

    fn attribute(&mut self, state: &FocusState, elapsed: f64) {
        if elapsed <= 0.0 {
            return;
        }
        // Cap single attributions so suspend/resume cycles cannot dump
        // hours into one bucket.
        let elapsed = elapsed.min(MAX_ATTRIBUTION_SECS);
        if let Some(id) = state.workspace_id {
            self.add(&format!("focus.workspace.{id}.seconds"), elapsed);
        }
        if let Some(name) = state.workspace_name.as_deref() {
            self.add(
                &format!("focus.workspace_name.{}.seconds", sanitize(name)),
                elapsed,
            );
        }
        if let Some(session) = state.session.as_deref() {
            self.add(
                &format!("focus.session.{}.seconds", sanitize(session)),
                elapsed,
            );
        }
        if let Some(project) = state.project.as_deref() {
            self.add(&format!("focus.project.{}.seconds", sanitize(project)), elapsed);
        }
        if let Some(app) = state.app_id.as_deref() {
            self.add(&format!("focus.app.{}.seconds", sanitize(app)), elapsed);
        }
        if let Some(output) = state.output.as_deref() {
            self.add(&format!("focus.output.{}.seconds", sanitize(output)), elapsed);
        }
    }

    fn add(&mut self, metric: &str, seconds: f64) {
        *self.pending.entry(metric.to_owned()).or_insert(0.0) += seconds;
    }
}

/// Graphite path segments allow `[A-Za-z0-9_-]`; dots separate nodes.
pub fn sanitize(value: &str) -> String {
    let mut cleaned = String::with_capacity(value.len());
    let mut last_underscore = false;
    for byte in value.bytes() {
        let ok = byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_';
        if ok {
            cleaned.push(byte as char);
            last_underscore = false;
        } else if !last_underscore && !cleaned.is_empty() {
            cleaned.push('_');
            last_underscore = true;
        }
    }
    if cleaned.is_empty() {
        return "unknown".to_owned();
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn state(workspace: u64, project: &str, app: &str) -> FocusState {
        FocusState {
            workspace_id: Some(workspace),
            workspace_name: Some(project.to_owned()),
            window_id: Some(workspace * 10),
            app_id: Some(app.to_owned()),
            session: Some("neovim".to_owned()),
            project: Some(project.to_owned()),
            output: Some("DP-2".to_owned()),
        }
    }

    #[test]
    fn attributes_elapsed_time_to_all_buckets() {
        let start = Instant::now();
        let mut tracker = Tracker::new(start);
        // Adopt the state first (attributes nothing), then attribute 40s.
        tracker.observe(state(10, "rsynapse", "ghostty"), start);
        tracker.observe(state(10, "rsynapse", "ghostty"), start + Duration::from_secs(40));
        let pending = tracker.drain();

        assert_eq!(pending.get("focus.workspace.10.seconds"), Some(&40.0));
        assert_eq!(
            pending.get("focus.workspace_name.rsynapse.seconds"),
            Some(&40.0)
        );
        assert_eq!(pending.get("focus.session.neovim.seconds"), Some(&40.0));
        assert_eq!(pending.get("focus.project.synapse.seconds"), None);
        assert_eq!(pending.get("focus.project.rsynapse.seconds"), Some(&40.0));
        assert_eq!(pending.get("focus.app.ghostty.seconds"), Some(&40.0));
        assert_eq!(pending.get("focus.output.DP-2.seconds"), Some(&40.0));
    }

    #[test]
    fn counts_workspace_switches_only() {
        let start = Instant::now();
        let mut tracker = Tracker::new(start);
        tracker.observe(state(10, "a", "x"), start + Duration::from_secs(5));
        tracker.observe(state(10, "a", "x"), start + Duration::from_secs(6));
        tracker.observe(state(11, "b", "y"), start + Duration::from_secs(16));
        let pending = tracker.drain();

        // Initial observe from the empty state counts as one switch.
        assert_eq!(pending.get("switches.workspace"), Some(&2.0));
        assert_eq!(pending.get("focus.workspace.10.seconds"), Some(&11.0));
        assert_eq!(pending.get("focus.workspace.11.seconds"), None);
    }

    #[test]
    fn caps_attribution_after_long_gaps() {
        let start = Instant::now();
        let mut tracker = Tracker::new(start);
        tracker.observe(state(10, "a", "x"), start);
        tracker.observe(state(10, "a", "x"), start + Duration::from_secs(10_000));
        let pending = tracker.drain();

        assert_eq!(
            pending.get("focus.workspace.10.seconds"),
            Some(&MAX_ATTRIBUTION_SECS)
        );
    }

    #[test]
    fn sanitize_keeps_paths_graphite_safe() {
        assert_eq!(sanitize("com.mitchellh.ghostty"), "com_mitchellh_ghostty");
        assert_eq!(sanitize("coro-uiq"), "coro-uiq");
        assert_eq!(sanitize("a  b//c"), "a_b_c");
        assert_eq!(sanitize(""), "unknown");
        assert_eq!(sanitize("..."), "unknown");
    }
}
