//! Workspace/project selector exposed as a launcher plugin.
use std::collections::HashMap;

use locus::{RelationEndpoint, RelationRecord, keys};
use proj_model::ProjectInfo;
use rsynapse_plugin::{
    Hint, KeyEvent, KeyResponse, Plugin, PluginGroup, PluginResponse, PluginView, QueryContext,
    ResultItem,
};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

const WORKSPACE_SERVICE: &str = workspace_dbus_api::BUS_NAME;
const WORKSPACE_ROOT: &str = workspace_dbus_api::ROOT_PATH;
const WORKSPACE_MANAGER: &str = workspace_dbus_api::MANAGER_INTERFACE;
const WORKSPACE_INTERFACE: &str = workspace_dbus_api::WORKSPACE_INTERFACE;
const OUTPUT_INTERFACE: &str = workspace_dbus_api::OUTPUT_INTERFACE;
const CONTROL_INTERFACE: &str = workspace_dbus_api::CONTROL_INTERFACE;

struct WorkspacesPlugin;

impl WorkspacesPlugin {
    fn connection() -> Option<Connection> {
        Connection::session().ok()
    }

    fn focused_workspace(connection: &Connection) -> Option<(String, String)> {
        let manager = Proxy::new(
            connection,
            WORKSPACE_SERVICE,
            WORKSPACE_ROOT,
            WORKSPACE_MANAGER,
        )
        .ok()?;
        let focused: Vec<OwnedObjectPath> = manager.get_property("FocusedWorkspace").ok()?;
        let path = focused.first()?;
        let workspace = Proxy::new(
            connection,
            WORKSPACE_SERVICE,
            path.as_str(),
            WORKSPACE_INTERFACE,
        )
        .ok()?;
        let id = workspace.get_property::<String>("Id").ok()?;
        (!id.is_empty()).then_some((id, path.as_str().to_owned()))
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

    fn current_project_id(connection: &Connection, workspace: &str) -> Option<String> {
        let records = Self::relations(connection, workspace_policy::PROJECT_RELATION);
        let record = Self::relation(&records, workspace, workspace_policy::PROJECT_RELATION)?;
        workspace_policy::project_id_of(record)
    }

    fn current_name(connection: &Connection, workspace: &str) -> Option<String> {
        let records = Self::relations(connection, workspace_policy::NAME_RELATION);
        workspace_policy::name_of(Self::relation(
            &records,
            workspace,
            workspace_policy::NAME_RELATION,
        )?)
    }

    fn focused_workspace_path(connection: &Connection) -> Option<String> {
        let manager = Proxy::new(
            connection,
            WORKSPACE_SERVICE,
            WORKSPACE_ROOT,
            WORKSPACE_MANAGER,
        )
        .ok()?;
        let focused: Vec<OwnedObjectPath> = manager.get_property("FocusedWorkspace").ok()?;
        focused.first().map(|path| path.as_str().to_owned())
    }

    fn workspace_mru(connection: &Connection) -> Vec<String> {
        let Ok(manager) = Proxy::new(
            connection,
            WORKSPACE_SERVICE,
            WORKSPACE_ROOT,
            WORKSPACE_MANAGER,
        ) else {
            return Vec::new();
        };
        manager
            .get_property::<Vec<OwnedObjectPath>>("WorkspaceMru")
            .unwrap_or_default()
            .into_iter()
            .map(|path| path.as_str().to_owned())
            .collect()
    }

    fn focused_output_name(connection: &Connection) -> Option<String> {
        let manager = Proxy::new(
            connection,
            WORKSPACE_SERVICE,
            WORKSPACE_ROOT,
            WORKSPACE_MANAGER,
        )
        .ok()?;
        let focused: Vec<OwnedObjectPath> = manager.get_property("FocusedOutput").ok()?;
        let path = focused.first()?;
        let output = Proxy::new(
            connection,
            WORKSPACE_SERVICE,
            path.as_str(),
            OUTPUT_INTERFACE,
        )
        .ok()?;
        output
            .get_property::<String>("Name")
            .ok()
            .filter(|name| !name.is_empty())
    }

    fn workspaces(connection: &Connection) -> Vec<workspace_policy::WorkspaceItem> {
        let Ok(manager) = Proxy::new(
            connection,
            WORKSPACE_SERVICE,
            WORKSPACE_ROOT,
            WORKSPACE_MANAGER,
        ) else {
            return Vec::new();
        };
        let Ok(paths) = manager.get_property::<Vec<OwnedObjectPath>>("Workspaces") else {
            return Vec::new();
        };
        let names = workspace_policy::workspace_names(&Self::relations(
            connection,
            workspace_policy::NAME_RELATION,
        ));
        paths
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
                    let enabled = output.get_property::<bool>("Enabled").unwrap_or(false);
                    if !enabled {
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
            .collect()
    }

    fn project_view(connection: &Connection, workspace: &str) -> PluginView {
        let title = Self::current_name(connection, workspace).unwrap_or_else(|| "Workspace".into());
        let subtitle = match Self::current_project_id(connection, workspace) {
            Some(project_id) => {
                let name = Self::projects(connection)
                    .into_iter()
                    .find(|project| project.id == project_id)
                    .map(|project| project.name)
                    .unwrap_or_else(|| "project".into());
                format!("Current project · {name}")
            }
            None => "No project assigned".to_owned(),
        };
        PluginView {
            title,
            subtitle,
            placeholder: "Search projects or type a workspace name".into(),
            hints: vec![
                Hint {
                    key: "↑↓".into(),
                    label: "choose".into(),
                },
                Hint {
                    key: "Enter".into(),
                    label: "apply".into(),
                },
                Hint {
                    key: "Del".into(),
                    label: "remove".into(),
                },
                Hint {
                    key: "Esc".into(),
                    label: "close".into(),
                },
            ],
        }
    }

    fn workspace_view() -> PluginView {
        PluginView {
            title: "Workspaces".into(),
            subtitle: "Focus a workspace on any output".into(),
            placeholder: "Search workspaces".into(),
            hints: vec![
                Hint {
                    key: "↑↓".into(),
                    label: "choose".into(),
                },
                Hint {
                    key: "Enter".into(),
                    label: "focus".into(),
                },
                Hint {
                    key: "Esc".into(),
                    label: "close".into(),
                },
            ],
        }
    }

    fn focus_workspace(connection: &Connection, output: &str, index: u32) -> Result<(), String> {
        let manager = Proxy::new(
            connection,
            WORKSPACE_SERVICE,
            WORKSPACE_ROOT,
            WORKSPACE_MANAGER,
        )
        .map_err(|error| error.to_string())?;
        let generation: u64 = manager
            .get_property("Generation")
            .map_err(|error| error.to_string())?;
        let control = Proxy::new(
            connection,
            WORKSPACE_SERVICE,
            WORKSPACE_ROOT,
            CONTROL_INTERFACE,
        )
        .map_err(|error| error.to_string())?;
        let _: String = control
            .call("SelectWorkspace", &(generation, output, index))
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn set_name(connection: &Connection, workspace: &str, name: &str) -> Result<(), String> {
        let proxy =
            locus::RelationsProxyBlocking::new(connection).map_err(|error| error.to_string())?;
        let _: locus::RelationState = proxy
            .set_one_with_persistence(
                RelationEndpoint::stable_key(keys::WORKSPACE_ID, workspace),
                workspace_policy::NAME_RELATION,
                RelationEndpoint::stable_key(workspace_policy::NAME_KIND, name),
                HashMap::from([("source".to_owned(), "manual".to_owned())]),
                true,
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn bind(connection: &Connection, workspace: &str, path: &str) -> Result<(), String> {
        let proxy = steward_api::AssociationsProxyBlocking::new(connection)
            .map_err(|error| error.to_string())?;
        let _: proj_model::ProjectInfo = proxy
            .bind_workspace_project(workspace, path)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn unassign(connection: &Connection, workspace: &str) -> Result<(), String> {
        let proxy = steward_api::AssociationsProxyBlocking::new(connection)
            .map_err(|error| error.to_string())?;
        let _: () = proxy
            .unassign_workspace_project(workspace)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn remove_project(connection: &Connection, id: &str) -> Result<(), String> {
        let proxy = proj_model::api::ManagerProxyBlocking::new(connection)
            .map_err(|error| error.to_string())?;
        let _: bool = proxy
            .remove_project(id)
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

impl Plugin for WorkspacesPlugin {
    fn name(&self) -> &'static str {
        "Workspaces"
    }

    fn groups(&self) -> Vec<PluginGroup> {
        vec![
            PluginGroup {
                id: "projects".into(),
                title: "Projects".into(),
                description: "Assign a project or name this workspace".into(),
                aliases: vec!["project".into(), "proj".into()],
                plugin: String::new(),
            },
            PluginGroup {
                id: "workspaces".into(),
                title: "Workspace".into(),
                description: "Focus a workspace on any output".into(),
                aliases: vec!["workspace".into(), "ws".into()],
                plugin: String::new(),
            },
        ]
    }

    fn execute(&self, item: &ResultItem) -> Option<Result<(), String>> {
        let connection = Self::connection()?;
        let (workspace, _) = Self::focused_workspace(&connection)?;
        let result = if item.id == "rename" {
            let name = item.data.as_str();
            Self::set_name(&connection, &workspace, name)
        } else if let Some(workspace) = item.id.strip_prefix("workspace:") {
            let _ = workspace;
            let data = item.data.as_str();
            let (output, index) = data.split_once('\u{1f}').unwrap_or_default();
            index
                .parse::<u32>()
                .map_err(|error| error.to_string())
                .and_then(|index| Self::focus_workspace(&connection, output, index))
        } else if item.id == "unassign" {
            Self::unassign(&connection, &workspace)
        } else if let Some(id) = item.id.strip_prefix("project:") {
            let _ = id;
            let path = item.data.as_str();
            Self::bind(&connection, &workspace, path)
        } else {
            Err(format!("Unknown workspace item '{}'", item.id))
        };
        Some(result)
    }

    fn query(&self, context: &QueryContext<'_>) -> PluginResponse {
        let Some(connection) = Self::connection() else {
            return PluginResponse::default();
        };
        if context.group == Some("workspaces") {
            let query = context.query.trim().to_lowercase();
            let words = query.split_whitespace().collect::<Vec<_>>();
            let projects_by_id = Self::projects(&connection)
                .into_iter()
                .map(|project| (project.id.clone(), project))
                .collect::<HashMap<_, _>>();
            let workspace_projects = workspace_policy::workspace_projects(&Self::relations(
                &connection,
                workspace_policy::PROJECT_RELATION,
            ));
            let current_output = context
                .params
                .iter()
                .any(|param| {
                    matches!(
                        param.as_str(),
                        "current-output" | "current-monitor" | "focused-output"
                    )
                })
                .then(|| Self::focused_output_name(&connection))
                .flatten();
            let mru = Self::workspace_mru(&connection);
            let focused_path = Self::focused_workspace_path(&connection);
            let mut workspaces = Self::workspaces(&connection);
            workspaces.retain(|item| {
                workspace_policy::visible(item)
                    && workspace_policy::matches(item, &words, current_output.as_deref())
            });
            workspaces.sort_by_key(|item| {
                workspace_policy::sort_key(item, &mru, focused_path.as_deref())
            });
            let results = workspaces
                .iter()
                .enumerate()
                .map(|(position, item)| {
                    let project = workspace_projects
                        .get(&item.id)
                        .and_then(|project_id| projects_by_id.get(project_id));
                    let (title, icon) = workspace_policy::title_and_icon(item, project);
                    ResultItem {
                        id: format!("workspace:{}", item.id),
                        title,
                        description: item.output.clone(),
                        icon: icon,
                        data: format!("{}\u{1f}{}", item.output, item.index),
                        score: 1_000.0 - position as f64,
                        current: workspace_policy::is_current(item, focused_path.as_deref()),
                    }
                })
                .collect();
            return PluginResponse {
                view: Some(Self::workspace_view()),
                results,
            };
        }

        if context.group != Some("projects") {
            return PluginResponse::default();
        }

        let Some((workspace, _)) = Self::focused_workspace(&connection) else {
            return PluginResponse::default();
        };
        let view = Self::project_view(&connection, &workspace);
        let current_project = Self::current_project_id(&connection, &workspace);
        let query = context.query.trim();
        let words = query.to_lowercase();
        let words = words.split_whitespace().collect::<Vec<_>>();

        let mut results = Self::projects(&connection)
            .into_iter()
            .filter(|project| {
                let text = format!("{} {}", project.name, project.cwd).to_lowercase();
                words.iter().all(|word| text.contains(word))
            })
            .enumerate()
            .map(|(index, project)| ResultItem {
                id: format!("project:{}", project.id),
                title: project.name,
                description: project.cwd.clone(),
                icon: if project.icon.is_empty() {
                    "folder-symbolic".into()
                } else {
                    project.icon
                },
                data: project.cwd,
                score: 1_000.0 - index as f64,
                current: current_project.as_deref() == Some(project.id.as_str()),
            })
            .collect::<Vec<_>>();

        if query.is_empty() {
            if current_project.is_some() {
                results.push(ResultItem {
                    id: "unassign".into(),
                    title: "Unassign project".into(),
                    description: "Remove the project association from this workspace".into(),
                    icon: "list-remove-symbolic".into(),
                    data: String::new(),
                    score: -1.0,
                    current: false,
                });
            }
        } else {
            results.push(ResultItem {
                id: "rename".into(),
                title: format!("Rename to “{query}”"),
                description: "Set the workspace name".into(),
                icon: "document-edit-symbolic".into(),
                data: query.to_owned(),
                score: -1.0,
                current: false,
            });
        }

        PluginResponse {
            view: Some(view),
            results,
        }
    }

    fn handle_key(&self, event: &KeyEvent, context: &QueryContext<'_>) -> KeyResponse {
        if event.key != "Delete" {
            return KeyResponse::default();
        }
        let Some(selected) = context.selected else {
            return KeyResponse::default();
        };
        let Some(id) = selected.strip_prefix("project:") else {
            return KeyResponse::default();
        };
        let Some(connection) = Self::connection() else {
            return KeyResponse {
                handled: true,
                refresh: false,
                close: false,
                message: "Launcher services are unavailable".into(),
            };
        };
        match Self::remove_project(&connection, id) {
            Ok(()) => KeyResponse {
                handled: true,
                refresh: true,
                close: false,
                message: "Removed project".into(),
            },
            Err(error) => KeyResponse {
                handled: true,
                refresh: false,
                close: false,
                message: error,
            },
        }
    }
}

pub fn plugin() -> Box<dyn Plugin> {
    Box::new(WorkspacesPlugin)
}
