//! Blocking workspace/project snapshot and actions for the launcher plugin.
use std::collections::HashMap;

use locus::{RelationEndpoint, RelationRecord, keys};
use proj_model::ProjectInfo;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

use workspace_dbus_api::{
    BUS_NAME as WORKSPACE_SERVICE, CONTROL_INTERFACE, MANAGER_INTERFACE, OUTPUT_INTERFACE,
    ROOT_PATH as WORKSPACE_ROOT, WORKSPACE_INTERFACE,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkspaceLauncherSnapshot {
    pub projects: Vec<ProjectInfo>,
    pub workspaces: Vec<workspace_policy::WorkspaceItem>,
    pub workspace_projects: HashMap<String, String>,
    pub focused_workspace: Option<String>,
    pub focused_workspace_path: Option<String>,
    pub focused_output: Option<String>,
    pub mru: Vec<String>,
    pub current_project_id: Option<String>,
    pub current_name: Option<String>,
}

impl WorkspaceLauncherSnapshot {
    pub fn current_project(&self) -> Option<&ProjectInfo> {
        let id = self.current_project_id.as_deref()?;
        self.projects.iter().find(|project| project.id == id)
    }
}

pub fn snapshot() -> Result<WorkspaceLauncherSnapshot, String> {
    let connection = Connection::session().map_err(|error| format!("session bus: {error}"))?;
    let manager = manager_proxy(&connection)?;
    let focused: Vec<OwnedObjectPath> = manager
        .get_property("FocusedWorkspace")
        .map_err(|error| format!("FocusedWorkspace: {error}"))?;
    let focused_workspace_path = focused.first().map(|path| path.as_str().to_owned());
    let focused_workspace = focused_workspace_path
        .as_deref()
        .and_then(|path| workspace_id(&connection, path));
    let focused_output = focused_output_name(&connection)?;
    let mru = manager
        .get_property::<Vec<OwnedObjectPath>>("WorkspaceMru")
        .unwrap_or_default()
        .into_iter()
        .map(|path| path.as_str().to_owned())
        .collect();
    let names_records = relations(&connection, workspace_policy::NAME_RELATION);
    let project_records = relations(&connection, workspace_policy::PROJECT_RELATION);
    let projects = projects(&connection);
    let current_name = focused_workspace.as_deref().and_then(|workspace| {
        workspace_policy::name_of(relation(
            &names_records,
            workspace,
            workspace_policy::NAME_RELATION,
        )?)
    });
    let current_project_id = focused_workspace.as_deref().and_then(|workspace| {
        workspace_policy::project_id_of(relation(
            &project_records,
            workspace,
            workspace_policy::PROJECT_RELATION,
        )?)
    });
    Ok(WorkspaceLauncherSnapshot {
        projects,
        workspaces: workspaces(&connection, &manager, &names_records)?,
        workspace_projects: workspace_policy::workspace_projects(&project_records),
        focused_workspace,
        focused_workspace_path,
        focused_output,
        mru,
        current_project_id,
        current_name,
    })
}

pub fn focus_workspace(output: &str, index: u32) -> Result<(), String> {
    let connection = Connection::session().map_err(|error| format!("session bus: {error}"))?;
    let manager = manager_proxy(&connection)?;
    let generation: u64 = manager
        .get_property("Generation")
        .map_err(|error| format!("Generation: {error}"))?;
    let control = Proxy::new(
        &connection,
        WORKSPACE_SERVICE,
        WORKSPACE_ROOT,
        CONTROL_INTERFACE,
    )
    .map_err(|error| format!("workspace control: {error}"))?;
    let _: String = control
        .call("SelectWorkspace", &(generation, output, index))
        .map_err(|error| format!("SelectWorkspace: {error}"))?;
    Ok(())
}

pub fn set_workspace_name(workspace: &str, name: &str) -> Result<(), String> {
    let connection = Connection::session().map_err(|error| format!("session bus: {error}"))?;
    let proxy = locus::RelationsProxyBlocking::new(&connection)
        .map_err(|error| format!("locus proxy: {error}"))?;
    let _: locus::RelationState = proxy
        .set_one_with_persistence(
            RelationEndpoint::stable_key(keys::WORKSPACE_ID, workspace),
            workspace_policy::NAME_RELATION,
            RelationEndpoint::stable_key(workspace_policy::NAME_KIND, name),
            HashMap::from([("source".to_owned(), "manual".to_owned())]),
            true,
        )
        .map_err(|error| format!("set workspace name: {error}"))?;
    Ok(())
}

pub fn bind_workspace_project(workspace: &str, path: &str) -> Result<(), String> {
    let connection = Connection::session().map_err(|error| format!("session bus: {error}"))?;
    let proxy = steward_api::AssociationsProxyBlocking::new(&connection)
        .map_err(|error| format!("steward proxy: {error}"))?;
    let _: ProjectInfo = proxy
        .bind_workspace_project(workspace, path)
        .map_err(|error| format!("bind workspace project: {error}"))?;
    Ok(())
}

pub fn unassign_workspace_project(workspace: &str) -> Result<(), String> {
    let connection = Connection::session().map_err(|error| format!("session bus: {error}"))?;
    let proxy = steward_api::AssociationsProxyBlocking::new(&connection)
        .map_err(|error| format!("steward proxy: {error}"))?;
    let _: () = proxy
        .unassign_workspace_project(workspace)
        .map_err(|error| format!("unassign workspace project: {error}"))?;
    Ok(())
}

pub fn remove_project(id: &str) -> Result<(), String> {
    let connection = Connection::session().map_err(|error| format!("session bus: {error}"))?;
    let proxy = proj_model::api::ManagerProxyBlocking::new(&connection)
        .map_err(|error| format!("projd proxy: {error}"))?;
    let _: bool = proxy
        .remove_project(id)
        .map_err(|error| format!("remove project: {error}"))?;
    Ok(())
}

fn manager_proxy(connection: &Connection) -> Result<Proxy<'static>, String> {
    Proxy::new(
        connection,
        WORKSPACE_SERVICE,
        WORKSPACE_ROOT,
        MANAGER_INTERFACE,
    )
    .map_err(|error| format!("workspace manager: {error}"))
}

fn projects(connection: &Connection) -> Vec<ProjectInfo> {
    let Ok(proxy) = proj_model::api::ManagerProxyBlocking::new(connection) else {
        return Vec::new();
    };
    let Ok(mut projects) = proxy.list_projects() else {
        return Vec::new();
    };
    projects.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.cwd.cmp(&right.cwd))
    });
    projects
}

fn relation<'a>(
    records: &'a [RelationRecord],
    workspace: &str,
    relation: &str,
) -> Option<&'a RelationRecord> {
    let subject = RelationEndpoint::stable_key(keys::WORKSPACE_ID, workspace);
    records
        .iter()
        .find(|record| record.subject == subject && record.relation == relation)
}

fn relations(connection: &Connection, relation: &str) -> Vec<RelationRecord> {
    let Ok(proxy) = locus::RelationsProxyBlocking::new(connection) else {
        return Vec::new();
    };
    proxy.list(relation).unwrap_or_default()
}

fn workspaces(
    connection: &Connection,
    manager: &Proxy<'_>,
    names_records: &[RelationRecord],
) -> Result<Vec<workspace_policy::WorkspaceItem>, String> {
    let paths: Vec<OwnedObjectPath> = manager
        .get_property("Workspaces")
        .map_err(|error| format!("Workspaces: {error}"))?;
    let names = workspace_policy::workspace_names(names_records);
    Ok(paths
        .iter()
        .filter_map(|path| {
            let workspace = Proxy::new(
                connection,
                WORKSPACE_SERVICE,
                path.as_str(),
                WORKSPACE_INTERFACE,
            )
            .ok()?;
            let id = workspace.get_property::<String>("Id").ok()?;
            if id.is_empty() {
                return None;
            }
            let name = names.get(&id).cloned().unwrap_or_else(|| {
                workspace
                    .get_property::<Vec<String>>("Name")
                    .unwrap_or_default()
                    .into_iter()
                    .next()
                    .unwrap_or_default()
            });
            let index = workspace.get_property::<u32>("Index").unwrap_or_default();
            let focused = workspace
                .get_property::<bool>("Focused")
                .unwrap_or_default();
            let windows = workspace
                .get_property::<Vec<OwnedObjectPath>>("Windows")
                .unwrap_or_default();
            let output_paths = workspace
                .get_property::<Vec<OwnedObjectPath>>("Output")
                .unwrap_or_default();
            let output = output_paths.first().and_then(|output| {
                let output = Proxy::new(
                    connection,
                    WORKSPACE_SERVICE,
                    output.as_str(),
                    OUTPUT_INTERFACE,
                )
                .ok()?;
                if !output.get_property::<bool>("Enabled").unwrap_or(false) {
                    return None;
                }
                let name = output.get_property::<String>("Name").ok()?;
                (!name.is_empty()).then_some(name)
            })?;
            Some(workspace_policy::WorkspaceItem {
                id,
                name,
                index,
                focused,
                has_windows: !windows.is_empty(),
                output,
            })
        })
        .collect())
}

fn focused_output_name(connection: &Connection) -> Result<Option<String>, String> {
    let manager = manager_proxy(connection)?;
    let focused: Vec<OwnedObjectPath> = manager
        .get_property("FocusedOutput")
        .map_err(|error| format!("FocusedOutput: {error}"))?;
    let Some(path) = focused.first() else {
        return Ok(None);
    };
    let output = Proxy::new(
        connection,
        WORKSPACE_SERVICE,
        path.as_str(),
        OUTPUT_INTERFACE,
    )
    .map_err(|error| format!("output proxy: {error}"))?;
    Ok(output
        .get_property::<String>("Name")
        .ok()
        .filter(|name| !name.is_empty()))
}

fn workspace_id(connection: &Connection, path: &str) -> Option<String> {
    let workspace = Proxy::new(connection, WORKSPACE_SERVICE, path, WORKSPACE_INTERFACE).ok()?;
    let id = workspace.get_property::<String>("Id").ok()?;
    (!id.is_empty()).then_some(id)
}
