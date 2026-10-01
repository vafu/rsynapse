use std::{collections::HashMap, time::Instant};

/// Attribution for one flush interval, in seconds.
pub const MAX_ATTRIBUTION_SECS: f64 = 120.0;

/// Focus state at a point in time. Only app/project names go into metric
/// paths; window titles and other content never leave the process.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FocusState {
    pub locked: bool,
    pub workspace_id: Option<u64>,
    pub workspace_name: Option<String>,
    pub window_id: Option<u64>,
    pub app_id: Option<String>,
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
        if !state.locked && !self.current.locked && state.workspace_id != self.current.workspace_id
        {
            *self
                .pending
                .entry("switches.workspace".to_owned())
                .or_insert(0.0) += 1.0;
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
        if state.locked || elapsed <= 0.0 {
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
        if let Some(project) = state.project.as_deref() {
            self.add(
                &format!("focus.project.{}.seconds", sanitize(project)),
                elapsed,
            );
        }
        if let Some(app) = state.app_id.as_deref() {
            self.add(&format!("focus.app.{}.seconds", sanitize(app)), elapsed);
        }
        if let Some(output) = state.output.as_deref() {
            self.add(
                &format!("focus.output.{}.seconds", sanitize(output)),
                elapsed,
            );
        }
    }

    fn add(&mut self, metric: &str, seconds: f64) {
        *self.pending.entry(metric.to_owned()).or_insert(0.0) += seconds;
    }
}

/// Canonical app name shared by metrics buckets and bar tiles:
/// exact aliases first, then profile-style prefixes, then the raw id.
/// One vocabulary so producers and consumers never diverge.
/// exact aliases first, then profile-style prefixes, then the raw id.
/// One vocabulary so bar tiles, metrics, and dashboards never diverge.
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
            locked: false,
            workspace_id: Some(workspace),
            workspace_name: Some(project.to_owned()),
            window_id: Some(workspace * 10),
            app_id: Some(app.to_owned()),
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
        tracker.observe(
            state(10, "rsynapse", "ghostty"),
            start + Duration::from_secs(40),
        );
        let pending = tracker.drain();

        assert_eq!(pending.get("focus.workspace.10.seconds"), Some(&40.0));
        assert_eq!(
            pending.get("focus.workspace_name.rsynapse.seconds"),
            Some(&40.0)
        );
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
    fn lock_terminates_all_attribution_and_unlock_is_not_a_switch() {
        let start = Instant::now();
        let mut tracker = Tracker::new(start);
        tracker.observe(state(10, "a", "codex"), start);
        tracker.drain();
        tracker.observe(
            FocusState {
                locked: true,
                ..FocusState::default()
            },
            start + Duration::from_secs(5),
        );
        let before_lock = tracker.drain();
        assert_eq!(before_lock.get("focus.app.codex.seconds"), Some(&5.0));
        assert!(!before_lock.contains_key("switches.workspace"));
        tracker.heartbeat(start + Duration::from_secs(30));
        assert!(tracker.drain().is_empty());
        tracker.observe(state(11, "b", "neovim"), start + Duration::from_secs(40));
        assert!(tracker.drain().is_empty());
        tracker.heartbeat(start + Duration::from_secs(45));
        let after_unlock = tracker.drain();
        assert_eq!(after_unlock.get("focus.app.neovim.seconds"), Some(&5.0));
        assert!(!after_unlock.contains_key("focus.app.codex.seconds"));
    }

    #[test]
    fn sanitize_keeps_paths_graphite_safe() {
        assert_eq!(sanitize("com.mitchellh.ghostty"), "com_mitchellh_ghostty");
        assert_eq!(sanitize("coro-uiq"), "coro-uiq");
        assert_eq!(sanitize("a  b//c"), "a_b_c");
        assert_eq!(sanitize(""), "unknown");
        assert_eq!(sanitize("..."), "unknown");
    }

    #[test]
    fn canonical_names_collapse_known_variants() {
        assert_eq!(canonical_app_id("com.mitchellh.ghostty"), "ghostty");
        assert_eq!(canonical_app_id("google-chrome"), "chrome");
        assert_eq!(
            canonical_app_id("chrome-kjbdgfilnfhdoflbpgamdcdgpehopbep-Default"),
            "chrome"
        );
        assert_eq!(canonical_app_id("firefox"), "firefox");
        assert_eq!(canonical_app_id("slack"), "slack");
    }
}
