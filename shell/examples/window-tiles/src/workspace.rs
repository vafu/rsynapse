use shell_core::source::Source;

#[shell_macros::dbus_model(
    module = root,
    interface = "org.rsynapse.Workspace.Manager1",
    default_service = "org.rsynapse.Workspace",
    default_path = "/org/rsynapse/Workspace"
)]
struct WorkspaceRoot {
    #[dbus(model)]
    windows: Vec<WorkspaceWindow>,
}

#[shell_macros::dbus_model(
    module = window,
    interface = "org.rsynapse.Workspace.Window1",
    default_service = "org.rsynapse.Workspace"
)]
pub(crate) struct WorkspaceWindow {
    id: workspace_dbus_api::Id,
    title: Option<String>,
    app_id: Option<String>,
    focused: bool,
}

pub(crate) fn windows() -> Source<Vec<WorkspaceWindow>> {
    WorkspaceRoot::new().windows()
}

#[cfg(test)]
mod tests {
    use super::{WorkspaceRoot, WorkspaceWindow};

    #[test]
    fn dbus_models_are_generated() {
        let _ = std::any::type_name::<WorkspaceRoot>();
        let _ = std::any::type_name::<WorkspaceWindow>();
    }
}
