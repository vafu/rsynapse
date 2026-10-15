use std::collections::BTreeMap;
use workspace_dbus_api::{output_path, window_path, workspace_path};
use zbus::zvariant::OwnedObjectPath;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Manager {
    pub connected: bool,
    pub generation: u64,
    pub outputs: Vec<OwnedObjectPath>,
    pub workspaces: Vec<OwnedObjectPath>,
    pub windows: Vec<OwnedObjectPath>,
    pub focused_output: Vec<OwnedObjectPath>,
    pub focused_workspace: Vec<OwnedObjectPath>,
    pub focused_window: Vec<OwnedObjectPath>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Output {
    pub focused: bool,
    pub current_workspace: Vec<OwnedObjectPath>,
    pub workspaces: Vec<OwnedObjectPath>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Workspace {
    pub name: Vec<String>,
    pub index: u32,
    pub output: Vec<OwnedObjectPath>,
    pub active: bool,
    pub focused: bool,
    pub urgent: bool,
    pub active_window: Vec<OwnedObjectPath>,
    pub windows: Vec<OwnedObjectPath>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Window {
    pub title: Vec<String>,
    pub app_id: Vec<String>,
    pub pid: Vec<i32>,
    pub workspace: Vec<OwnedObjectPath>,
    pub output: Vec<OwnedObjectPath>,
    pub focused: bool,
    pub floating: bool,
    pub urgent: bool,
    pub column_index: Vec<u64>,
    pub row_index: Vec<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Projection {
    pub manager: Manager,
    pub outputs: BTreeMap<String, Output>,
    pub workspaces: BTreeMap<String, Workspace>,
    pub windows: BTreeMap<String, Window>,
}

impl Projection {
    /// New objects are registered without references before the final projection
    /// is published. Cyclic workspace/window links cannot dangle during bootstrap.
    pub fn stage_additions(&self, next: &Self) -> Self {
        let mut staged = self.clone();
        for id in next.outputs.keys() {
            staged.outputs.entry(id.clone()).or_default();
        }
        for (id, value) in &next.workspaces {
            staged
                .workspaces
                .entry(id.clone())
                .or_insert_with(|| Workspace {
                    name: value.name.clone(),
                    index: value.index,
                    active: false,
                    focused: false,
                    urgent: value.urgent,
                    ..Workspace::default()
                });
        }
        for (id, value) in &next.windows {
            staged.windows.entry(id.clone()).or_insert_with(|| {
                let mut value = value.clone();
                value.workspace.clear();
                value.output.clear();
                value.focused = false;
                value
            });
        }
        staged
    }

    /// Keep objects during temporarily unresolved cross-family updates, but never
    /// publish a reference to an object not present in this inventory.
    pub fn normalize(&mut self) {
        let outputs: Vec<_> = self.outputs.keys().map(|id| output_path(id)).collect();
        let workspaces: Vec<_> = self
            .workspaces
            .keys()
            .map(|id| workspace_path(id))
            .collect();
        let windows: Vec<_> = self.windows.keys().map(|id| window_path(id)).collect();
        for output in self.outputs.values_mut() {
            output.current_workspace.retain(|p| workspaces.contains(p));
            output.workspaces.retain(|p| workspaces.contains(p));
        }
        for workspace in self.workspaces.values_mut() {
            workspace.output.retain(|p| outputs.contains(p));
            workspace.active_window.retain(|p| windows.contains(p));
            workspace.windows.retain(|p| windows.contains(p));
        }
        for window in self.windows.values_mut() {
            window.workspace.retain(|p| workspaces.contains(p));
            window.output.retain(|p| outputs.contains(p));
        }
        self.manager.outputs = outputs;
        self.manager.workspaces = workspaces;
        self.manager.windows = windows;
        self.manager
            .focused_output
            .retain(|p| self.manager.outputs.contains(p));
        self.manager
            .focused_workspace
            .retain(|p| self.manager.workspaces.contains(p));
        self.manager
            .focused_window
            .retain(|p| self.manager.windows.contains(p));
    }
}
