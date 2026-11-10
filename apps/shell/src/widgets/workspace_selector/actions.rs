//! Side effects for the workspace selector.

pub(super) async fn assign(workspace: String, path: String) -> Result<(), String> {
    data_sources::proj::bind_workspace_project(workspace, &path, false)
        .await
        .map(|_| ())
}

pub(super) async fn unassign(workspace: String) -> Result<(), String> {
    data_sources::proj::unassign_workspace_project(workspace).await
}

pub(super) async fn remove_project(id: String) -> Result<(), String> {
    data_sources::proj::remove_project(id).await
}

pub(super) async fn rename(workspace: String, name: String) -> Result<(), String> {
    data_sources::workspace_name::set_workspace_name(workspace, name).await
}
