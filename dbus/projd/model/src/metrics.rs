use super::GoalInfo;
use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

pub const METRICS_INTERFACE: &str = "org.rsynapse.Proj.Metrics1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
#[zvariant(signature = "s")]
pub enum WorkKind {
    Active,
    Idle,
    Agent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct GoalIdentityInfo {
    pub instance_id: String,
    pub key: String,
    pub goal: GoalInfo,
    pub project_name: String,
    pub created_at_ms: u64,
    pub observed_at_ms: u64,
    pub deleted_at_ms: u64,
    pub ancestor_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct GoalEventInfo {
    pub sequence: u64,
    pub at_ms: u64,
    pub kind: String,
    pub instance_id: String,
    pub key: String,
    pub old_status: String,
    pub goal: GoalInfo,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct GoalTrackingInfo {
    pub instance_id: String,
    pub key: String,
    pub title: String,
    pub project_id: String,
    pub project_name: String,
    pub status: String,
    pub parent_instance_id: String,
    pub selected: bool,
    pub ancestor_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct WorkSessionInfo {
    pub id: String,
    pub instance_id: String,
    pub goal_key: String,
    pub goal_title: String,
    pub project_id: String,
    pub project_name: String,
    pub ancestor_ids: Vec<String>,
    pub source: String,
    pub kind: WorkKind,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub duration_ms: u64,
    pub open: bool,
    pub end_reason: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct GoalStatsInfo {
    pub instance_id: String,
    pub key: String,
    pub title: String,
    pub project_id: String,
    pub project_name: String,
    pub status: String,
    pub created_at_ms: u64,
    pub observed_at_ms: u64,
    pub first_started_at_ms: u64,
    pub completed_at_ms: u64,
    pub lead_time_ms: u64,
    pub cycle_time_ms: u64,
    pub in_progress_ms: u64,
    pub active_ms: u64,
    pub idle_ms: u64,
    pub agent_ms: u64,
    pub completion_count: u32,
    pub interrupted_sessions: u32,
    pub history_partial: bool,
}
