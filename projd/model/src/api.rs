//! Typed zbus proxies for the projd public API.
//!
//! DTOs stay in the model crate; these proxies are the single client surface
//! shared by shell-source, dashboard-api, and Steward.
use zbus::proxy;

use crate::metrics::{
    GoalEventInfo, GoalIdentityInfo, GoalStatsInfo, GoalTrackingInfo, WorkKind, WorkSessionInfo,
};
use std::collections::HashMap;

use crate::{CheckoutInfo, GoalInfo, ProjectInfo, RemovedProjectInfo};

#[proxy(
    interface = "org.rsynapse.Proj.Manager1",
    default_service = "org.rsynapse.Proj",
    default_path = "/org/rsynapse/Proj"
)]
pub trait Manager {
    fn list_projects(&self) -> zbus::Result<Vec<ProjectInfo>>;
    fn list_checkouts(&self) -> zbus::Result<Vec<CheckoutInfo>>;
    fn list_goals(&self, query: &str) -> zbus::Result<Vec<GoalInfo>>;
    fn register_project(&self, path: &str) -> zbus::Result<ProjectInfo>;
    fn resolve_project(&self, path: &str) -> zbus::Result<ProjectInfo>;
    fn refresh(&self, path: &str) -> zbus::Result<ProjectInfo>;
    fn rename_project(&self, id: &str, name: &str) -> zbus::Result<ProjectInfo>;
    fn remove_project(&self, id: &str) -> zbus::Result<bool>;
    fn set_project_icon(&self, id: &str, glyph: &str) -> zbus::Result<ProjectInfo>;
    fn set_project_icon_if_unset(&self, id: &str, glyph: &str) -> zbus::Result<ProjectInfo>;
    fn clear_project_icon(&self, id: &str) -> zbus::Result<ProjectInfo>;
    fn set_goal_status(&self, key: &str, status: &str) -> zbus::Result<GoalInfo>;
    fn set_active_goal(&self, project: &str, key: &str) -> zbus::Result<()>;
    fn remove_goal(&self, date: &str, id: &str) -> zbus::Result<bool>;
    fn create_goal(&self, info: &GoalInfo) -> zbus::Result<GoalInfo>;
    fn edit_goal(&self, key: &str, fields: HashMap<String, String>) -> zbus::Result<GoalInfo>;
    fn add_project_labels(
        &self,
        project_id: &str,
        labels: Vec<String>,
    ) -> zbus::Result<Vec<String>>;
    fn remove_project_labels(
        &self,
        project_id: &str,
        labels: Vec<String>,
    ) -> zbus::Result<Vec<String>>;

    #[zbus(signal)]
    fn changed(&self, kind: String, id: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn project_removed(&self, info: RemovedProjectInfo) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.rsynapse.Proj.Metrics1",
    default_service = "org.rsynapse.Proj",
    default_path = "/org/rsynapse/Proj"
)]
pub trait Metrics {
    fn list_goal_identities(&self, query: &str) -> zbus::Result<Vec<GoalIdentityInfo>>;
    fn list_goal_tracking(&self) -> zbus::Result<Vec<GoalTrackingInfo>>;
    fn goal_history(
        &self,
        instance_id: &str,
        from_ms: u64,
        to_ms: u64,
    ) -> zbus::Result<Vec<GoalEventInfo>>;
    fn work_timeline(
        &self,
        instance_id: &str,
        project_id: &str,
        from_ms: u64,
        to_ms: u64,
        include_subgoals: bool,
    ) -> zbus::Result<Vec<WorkSessionInfo>>;
    fn goal_stats(
        &self,
        instance_id: &str,
        from_ms: u64,
        to_ms: u64,
        include_subgoals: bool,
    ) -> zbus::Result<GoalStatsInfo>;
    fn begin_goal_work(
        &self,
        instance_id: &str,
        kind: WorkKind,
        source: &str,
    ) -> zbus::Result<WorkSessionInfo>;
    fn end_goal_work(&self, session_id: &str, reason: &str) -> zbus::Result<WorkSessionInfo>;
    fn get_work_session(&self, session_id: &str) -> zbus::Result<WorkSessionInfo>;

    #[zbus(signal)]
    fn work_changed(&self, session_id: String) -> zbus::Result<()>;
}
