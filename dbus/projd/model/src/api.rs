//! Typed zbus proxies for the projd public API.
//!
//! DTOs stay in the model crate; these proxies are the single client surface
//! shared by data-sources, dashboard-api, and Steward.
use zbus::proxy;

use crate::{CheckoutInfo, ProjectInfo, RemovedProjectInfo};

#[proxy(
    interface = "org.rsynapse.Proj.Manager1",
    default_service = "org.rsynapse.Proj",
    default_path = "/org/rsynapse/Proj"
)]
pub trait Manager {
    fn list_projects(&self) -> zbus::Result<Vec<ProjectInfo>>;
    fn list_checkouts(&self) -> zbus::Result<Vec<CheckoutInfo>>;
    fn register_project(&self, path: &str) -> zbus::Result<ProjectInfo>;
    fn resolve_project(&self, path: &str) -> zbus::Result<ProjectInfo>;
    fn refresh(&self, path: &str) -> zbus::Result<ProjectInfo>;
    fn rename_project(&self, id: &str, name: &str) -> zbus::Result<ProjectInfo>;
    fn remove_project(&self, id: &str) -> zbus::Result<bool>;
    fn set_project_icon(&self, id: &str, glyph: &str) -> zbus::Result<ProjectInfo>;
    fn set_project_icon_if_unset(&self, id: &str, glyph: &str) -> zbus::Result<ProjectInfo>;
    fn clear_project_icon(&self, id: &str) -> zbus::Result<ProjectInfo>;
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
