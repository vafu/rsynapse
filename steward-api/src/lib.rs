//! Typed client for the Steward association policy API.
//!
//! Steward owns association policy; consumers call this client instead of
//! re-declaring the D-Bus interface.
use proj_model::ProjectInfo;

#[zbus::proxy(
    interface = "org.rsynapse.Steward.Associations1",
    default_service = "org.rsynapse.Steward",
    default_path = "/org/rsynapse/Steward"
)]
pub trait Associations {
    fn bind_workspace_project(&self, workspace: &str, path: &str) -> zbus::Result<ProjectInfo>;
    fn unassign_workspace_project(&self, workspace: &str) -> zbus::Result<()>;
    fn get_init_workspace(&self) -> zbus::Result<String>;
    fn init_workspace_project(&self, workspace: &str, path: &str) -> zbus::Result<ProjectInfo>;
    fn bind_current_project(&self, path: &str) -> zbus::Result<ProjectInfo>;
}
