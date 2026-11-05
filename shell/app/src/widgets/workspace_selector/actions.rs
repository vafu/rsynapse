//! Side effects for the workspace selector.
use std::collections::HashMap;

use shell_core::source::proj::ProjectInfo;

#[zbus::proxy(
    interface = "org.rsynapse.Steward.Associations1",
    default_service = "org.rsynapse.Steward",
    default_path = "/org/rsynapse/Steward"
)]
trait Associations {
    fn bind_workspace_project(&self, workspace: &str, path: &str) -> zbus::Result<ProjectInfo>;
    fn unassign_workspace_project(&self, workspace: &str) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.rsynapse.Proj.Manager1",
    default_service = "org.rsynapse.Proj",
    default_path = "/org/rsynapse/Proj"
)]
trait Projects {
    fn remove_project(&self, id: &str) -> zbus::Result<bool>;
}

pub(super) async fn assign(workspace: String, path: String) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    AssociationsProxy::new(&connection)
        .await?
        .bind_workspace_project(&workspace, &path)
        .await?;
    Ok(())
}

pub(super) async fn unassign(workspace: String) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    AssociationsProxy::new(&connection)
        .await?
        .unassign_workspace_project(&workspace)
        .await?;
    Ok(())
}

pub(super) async fn remove_project(id: String) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    ProjectsProxy::new(&connection)
        .await?
        .remove_project(&id)
        .await?;
    Ok(())
}

pub(super) async fn rename(workspace: String, name: String) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    let proxy = locus::RelationsProxy::new(&connection).await?;
    proxy
        .set_one_with_persistence(
            locus::RelationEndpoint::stable_key(locus::keys::WORKSPACE_ID, workspace),
            "org.rsynapse.workspace.name",
            locus::RelationEndpoint::stable_key("org.rsynapse.workspace.name", name),
            HashMap::from([("source".to_owned(), "manual".to_owned())]),
            true,
        )
        .await?;
    Ok(())
}
