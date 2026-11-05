//! Workspace/project selector exposed as a launcher plugin.
use std::collections::HashMap;

use locus::{RelationEndpoint, RelationRecord, keys};
use proj_model::ProjectInfo;
use rsynapse_plugin::{
    Hint, Key, KeyEvent, KeyOutcome, Plugin, PluginGroup, PluginResponse, PluginView, QueryContext,
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
const PROJECT_RELATION: &str = "org.rsynapse.workspace.project";
const NAME_RELATION: &str = "org.rsynapse.workspace.name";
const NAME_KIND: &str = "org.rsynapse.workspace.name";
const STEWARD_SERVICE: &str = "org.rsynapse.Steward";
const STEWARD_PATH: &str = "/org/rsynapse/Steward";
const STEWARD_ASSOCIATIONS: &str = "org.rsynapse.Steward.Associations1";

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
        let Ok(proxy) = Proxy::new(
            connection,
            proj_model::BUS_NAME,
            proj_model::ROOT_PATH,
            proj_model::MANAGER_INTERFACE,
        ) else {
            return Vec::new();
        };
        let Ok(mut projects) = proxy.call::<_, _, Vec<ProjectInfo>>("ListProjects", &()) else {
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
        let Ok(proxy) = Proxy::new(
            connection,
            locus::BUS_NAME,
            locus::OBJECT_PATH,
            locus::RELATIONS_INTERFACE,
        ) else {
            return Vec::new();
        };
        proxy
            .call::<_, _, Vec<RelationRecord>>("List", &(relation,))
            .unwrap_or_default()
    }

    fn current_project_id(connection: &Connection, workspace: &str) -> Option<String> {
        let records = Self::relations(connection, PROJECT_RELATION);
        let record = Self::relation(&records, workspace, PROJECT_RELATION)?;
        record
            .metadata
            .get("project-id")
            .cloned()
            .filter(|id| !id.is_empty())
    }

    fn current_name(connection: &Connection, workspace: &str) -> Option<String> {
        let records = Self::relations(connection, NAME_RELATION);
        let record = Self::relation(&records, workspace, NAME_RELATION)?;
        match &record.target {
            RelationEndpoint::StableKey { kind, id } if kind == NAME_KIND => {
                let id = id.trim();
                (!id.is_empty()).then(|| id.to_owned())
            }
            _ => None,
        }
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

    fn workspaces(connection: &Connection) -> Vec<(String, String, u32, bool, String)> {
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
        let names = Self::workspace_names(connection);
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
                // Hide anonymous empty non-focused workspaces. Named, occupied,
                // and the focused workspace remain available.
                if windows.is_empty() && name.is_empty() && !focused {
                    return None;
                }
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
                Some((id, name, index, focused, output))
            })
            .collect()
    }

    fn workspace_names(connection: &Connection) -> HashMap<String, String> {
        Self::relations(connection, NAME_RELATION)
            .into_iter()
            .filter_map(|record| {
                if record.metadata.get("source").map(String::as_str) != Some("manual") {
                    return None;
                }
                let RelationEndpoint::StableKey { kind, id } = record.subject else {
                    return None;
                };
                if kind != keys::WORKSPACE_ID {
                    return None;
                }
                let RelationEndpoint::StableKey { kind, id: name } = record.target else {
                    return None;
                };
                if kind != NAME_KIND {
                    return None;
                }
                let name = name.trim().to_owned();
                (!name.is_empty()).then_some((id, name))
            })
            .collect()
    }

    fn workspace_projects(connection: &Connection) -> HashMap<String, String> {
        Self::relations(connection, PROJECT_RELATION)
            .into_iter()
            .filter_map(|record| {
                let RelationEndpoint::StableKey { kind, id } = record.subject else {
                    return None;
                };
                if kind != keys::WORKSPACE_ID {
                    return None;
                }
                let project_id = record.metadata.get("project-id")?.clone();
                (!project_id.is_empty()).then_some((id, project_id))
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
                Some(format!("Current project · {name}"))
            }
            None => Some("No project assigned".into()),
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
            subtitle: Some("Focus a workspace on any output".into()),
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
        let proxy = Proxy::new(
            connection,
            locus::BUS_NAME,
            locus::OBJECT_PATH,
            locus::RELATIONS_INTERFACE,
        )
        .map_err(|error| error.to_string())?;
        let _: locus::RelationState = proxy
            .call(
                "SetOneWithPersistence",
                &(
                    RelationEndpoint::stable_key(keys::WORKSPACE_ID, workspace),
                    NAME_RELATION,
                    RelationEndpoint::stable_key(NAME_KIND, name),
                    HashMap::from([("source".to_owned(), "manual".to_owned())]),
                    true,
                ),
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn bind(connection: &Connection, workspace: &str, path: &str) -> Result<(), String> {
        let proxy = Proxy::new(
            connection,
            STEWARD_SERVICE,
            STEWARD_PATH,
            STEWARD_ASSOCIATIONS,
        )
        .map_err(|error| error.to_string())?;
        let _: proj_model::ProjectInfo = proxy
            .call("BindWorkspaceProject", &(workspace, path))
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn unassign(connection: &Connection, workspace: &str) -> Result<(), String> {
        let proxy = Proxy::new(
            connection,
            STEWARD_SERVICE,
            STEWARD_PATH,
            STEWARD_ASSOCIATIONS,
        )
        .map_err(|error| error.to_string())?;
        let _: () = proxy
            .call("UnassignWorkspaceProject", &(workspace,))
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn remove_project(connection: &Connection, id: &str) -> Result<(), String> {
        let proxy = Proxy::new(
            connection,
            proj_model::BUS_NAME,
            proj_model::ROOT_PATH,
            proj_model::MANAGER_INTERFACE,
        )
        .map_err(|error| error.to_string())?;
        let _: bool = proxy
            .call("RemoveProject", &(id,))
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
                description: Some("Assign a project or name this workspace".into()),
                aliases: vec!["project".into(), "proj".into()],
            },
            PluginGroup {
                id: "workspaces".into(),
                title: "Workspace".into(),
                description: Some("Focus a workspace on any output".into()),
                aliases: vec!["workspace".into(), "ws".into()],
            },
        ]
    }

    fn execute(&self, item: &ResultItem) -> Option<Result<(), String>> {
        let connection = Self::connection()?;
        let (workspace, _) = Self::focused_workspace(&connection)?;
        let result = if item.id == "rename" {
            let name = item.data.as_deref().unwrap_or_default();
            Self::set_name(&connection, &workspace, name)
        } else if let Some(workspace) = item.id.strip_prefix("workspace:") {
            let _ = workspace;
            let data = item.data.as_deref().unwrap_or_default();
            let (output, index) = data.split_once('\u{1f}').unwrap_or_default();
            index
                .parse::<u32>()
                .map_err(|error| error.to_string())
                .and_then(|index| Self::focus_workspace(&connection, output, index))
        } else if item.id == "unassign" {
            Self::unassign(&connection, &workspace)
        } else if let Some(id) = item.id.strip_prefix("project:") {
            let _ = id;
            let path = item.data.as_deref().unwrap_or_default();
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
            let workspace_projects = Self::workspace_projects(&connection);
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
            workspaces.sort_by_key(|(id, _, _, _, _)| {
                let path = workspace_dbus_api::workspace_path(id).as_str().to_owned();
                if focused_path.as_deref() == Some(path.as_str()) {
                    0
                } else {
                    mru.iter()
                        .position(|ranked| ranked == &path)
                        .map(|rank| rank + 1)
                        .unwrap_or(usize::MAX)
                }
            });
            let results = workspaces
                .into_iter()
                .filter(|(_, name, index, _, output)| {
                    if current_output
                        .as_deref()
                        .is_some_and(|current| current != output)
                    {
                        return false;
                    }
                    let text = format!("{name} {index} {output}").to_lowercase();
                    words.iter().all(|word| text.contains(word))
                })
                .enumerate()
                .map(|(position, (id, name, index, _focused, output))| {
                    let path = workspace_dbus_api::workspace_path(&id);
                    let is_current = focused_path.as_deref() == Some(path.as_str());
                    let project = workspace_projects
                        .get(&id)
                        .and_then(|project_id| projects_by_id.get(project_id));
                    let (title, icon) = match project {
                        Some(project) => (
                            project.name.clone(),
                            if project.icon.is_empty() {
                                "folder-symbolic".into()
                            } else {
                                project.icon.clone()
                            },
                        ),
                        None => (
                            if name.is_empty() {
                                format!("Workspace {index}")
                            } else {
                                name
                            },
                            "focus-windows-symbolic".into(),
                        ),
                    };
                    ResultItem {
                        id: format!("workspace:{id}"),
                        title,
                        description: Some(output.clone()),
                        icon: Some(icon),
                        data: Some(format!("{output}\u{1f}{index}")),
                        score: 1_000.0 - position as f64,
                        current: is_current,
                    }
                })
                .collect();
            return PluginResponse {
                view: Some(Self::workspace_view()),
                results,
            };
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
                description: Some(project.cwd.clone()),
                icon: Some(if project.icon.is_empty() {
                    "folder-symbolic".into()
                } else {
                    project.icon
                }),
                data: Some(project.cwd),
                score: 1_000.0 - index as f64,
                current: current_project.as_deref() == Some(project.id.as_str()),
            })
            .collect::<Vec<_>>();

        if query.is_empty() {
            if current_project.is_some() {
                results.push(ResultItem {
                    id: "unassign".into(),
                    title: "Unassign project".into(),
                    description: Some("Remove the project association from this workspace".into()),
                    icon: Some("list-remove-symbolic".into()),
                    data: None,
                    score: -1.0,
                    current: false,
                });
            }
        } else {
            results.push(ResultItem {
                id: "rename".into(),
                title: format!("Rename to “{query}”"),
                description: Some("Set the workspace name".into()),
                icon: Some("document-edit-symbolic".into()),
                data: Some(query.to_owned()),
                score: -1.0,
                current: false,
            });
        }

        PluginResponse {
            view: Some(view),
            results,
        }
    }

    fn handle_key(&self, event: &KeyEvent, context: &QueryContext<'_>) -> KeyOutcome {
        if event.key != Key::Delete {
            return KeyOutcome::Ignored;
        }
        let Some(selected) = context.selected else {
            return KeyOutcome::Ignored;
        };
        let Some(id) = selected.strip_prefix("project:") else {
            return KeyOutcome::Ignored;
        };
        let Some(connection) = Self::connection() else {
            return KeyOutcome::Handled {
                refresh: false,
                close: false,
                message: Some("Launcher services are unavailable".into()),
            };
        };
        match Self::remove_project(&connection, id) {
            Ok(()) => KeyOutcome::Handled {
                refresh: true,
                close: false,
                message: Some("Removed project".into()),
            },
            Err(error) => KeyOutcome::Handled {
                refresh: false,
                close: false,
                message: Some(error),
            },
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _rsynapse_init() -> *mut dyn Plugin {
    Box::into_raw(Box::new(WorkspacesPlugin))
}
