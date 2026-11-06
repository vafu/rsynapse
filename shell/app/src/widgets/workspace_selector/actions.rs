//! Side effects for the workspace selector.
use std::collections::HashMap;

pub(super) async fn assign(workspace: String, path: String) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    shell_core::source::steward::AssociationsProxy::new(&connection)
        .await?
        .bind_workspace_project(&workspace, &path)
        .await?;
    Ok(())
}

pub(super) async fn unassign(workspace: String) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    shell_core::source::steward::AssociationsProxy::new(&connection)
        .await?
        .unassign_workspace_project(&workspace)
        .await?;
    Ok(())
}

pub(super) async fn remove_project(id: String) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    shell_core::source::proj::ManagerProxy::new(&connection)
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
