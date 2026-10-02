mod attribution;
mod source;
mod tracker;

pub(super) use source::snapshots;
pub(super) use tracker::AgentTracker;

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Session {
    pub key: String,
    pub session_id: String,
    pub agent: String,
    pub state: String,
    pub cwd: String,
    pub window: Option<u64>,
    pub parent: String,
    pub subagent: bool,
    pub project: String,
    pub workspace: String,
}

impl Session {
    fn prefixes(&self) -> [String; 2] {
        let agent = crate::focus::sanitize(&self.agent);
        let role = if self.subagent { "subagent" } else { "root" };
        [
            format!(
                "agents.project.{}.agent.{agent}.role.{role}",
                crate::focus::sanitize(&self.project)
            ),
            format!(
                "agents.workspace_name.{}.agent.{agent}.role.{role}",
                crate::focus::sanitize(&self.workspace)
            ),
        ]
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Snapshot {
    pub sessions: Vec<Session>,
    // Exact active session links disambiguate multiple sessions sharing a window.
    pub active_links: std::collections::HashMap<u64, String>,
}
