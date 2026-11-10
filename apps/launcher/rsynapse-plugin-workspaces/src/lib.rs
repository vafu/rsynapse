//! Workspace/project selector exposed as a launcher plugin.
use data_sources::{workspace_launcher, workspace_policy};
use rsynapse_plugin::{
    Hint, KeyEvent, KeyResponse, LauncherActivity, Plugin, PluginGroup, PluginResponse, PluginView,
    QueryContext, ResultItem,
};

const PROJECTS_GROUP: &str = "projects";
const WORKSPACES_GROUP: &str = "workspaces";

struct WorkspacesPlugin;

impl WorkspacesPlugin {
    fn project_view(snapshot: &workspace_launcher::WorkspaceLauncherSnapshot) -> PluginView {
        let title = snapshot
            .current_name
            .clone()
            .unwrap_or_else(|| "Workspace".into());
        let subtitle = match snapshot.current_project() {
            Some(project) => format!("Current project · {}", project.name),
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

    fn workspace_results(
        snapshot: &workspace_launcher::WorkspaceLauncherSnapshot,
        words: &[&str],
        current_output: Option<&str>,
    ) -> Vec<ResultItem> {
        let projects = snapshot
            .projects
            .iter()
            .map(|project| (project.id.clone(), project))
            .collect::<std::collections::HashMap<_, _>>();
        let mut workspaces = snapshot.workspaces.clone();
        workspaces.retain(|item| {
            workspace_policy::visible(item)
                && workspace_policy::matches(item, words, current_output)
        });
        workspaces.sort_by_key(|item| {
            workspace_policy::sort_key(
                item,
                &snapshot.mru,
                snapshot.focused_workspace_path.as_deref(),
            )
        });
        workspaces
            .iter()
            .enumerate()
            .map(|(position, item)| {
                let project = snapshot
                    .workspace_projects
                    .get(&item.id)
                    .and_then(|project_id| projects.get(project_id).copied());
                let (title, icon) = workspace_policy::title_and_icon(item, project);
                ResultItem {
                    id: format!("workspace:{}", item.id),
                    title,
                    description: item.output.clone(),
                    icon,
                    data: format!("{}\u{1f}{}", item.output, item.index),
                    score: 1_000.0 - position as f64,
                    activity: LauncherActivity::None,
                    current: workspace_policy::is_current(
                        item,
                        snapshot.focused_workspace_path.as_deref(),
                    ),
                    keep_open: false,
                }
            })
            .collect()
    }

    fn project_results(
        snapshot: &workspace_launcher::WorkspaceLauncherSnapshot,
        query: &str,
    ) -> Vec<ResultItem> {
        let words = query.to_lowercase();
        let words = words.split_whitespace().collect::<Vec<_>>();
        let mut results = snapshot
            .projects
            .iter()
            .filter(|project| {
                let text = format!("{} {}", project.name, project.cwd).to_lowercase();
                words.iter().all(|word| text.contains(word))
            })
            .enumerate()
            .map(|(index, project)| ResultItem {
                id: format!("project:{}", project.id),
                title: project.name.clone(),
                description: project.cwd.clone(),
                icon: if project.icon.is_empty() {
                    "folder-symbolic".into()
                } else {
                    project.icon.clone()
                },
                data: project.cwd.clone(),
                score: 1_000.0 - index as f64,
                activity: LauncherActivity::None,
                current: snapshot.current_project_id.as_deref() == Some(project.id.as_str()),
                keep_open: false,
            })
            .collect::<Vec<_>>();

        if query.is_empty() {
            if snapshot.current_project_id.is_some() {
                results.push(ResultItem {
                    id: "unassign".into(),
                    title: "Unassign project".into(),
                    description: "Remove the project association from this workspace".into(),
                    icon: "list-remove-symbolic".into(),
                    data: String::new(),
                    score: -1.0,
                    activity: LauncherActivity::None,
                    current: false,
                    keep_open: false,
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
                activity: LauncherActivity::None,
                current: false,
                keep_open: false,
            });
        }
        results
    }
}

impl Plugin for WorkspacesPlugin {
    fn name(&self) -> &'static str {
        "Workspaces"
    }

    fn groups(&self) -> Vec<PluginGroup> {
        vec![
            PluginGroup {
                id: PROJECTS_GROUP.into(),
                title: "Projects".into(),
                description: "Assign a project or name this workspace".into(),
                aliases: vec!["project".into(), "proj".into()],
                plugin: String::new(),
            },
            PluginGroup {
                id: WORKSPACES_GROUP.into(),
                title: "Workspace".into(),
                description: "Focus a workspace on any output".into(),
                aliases: vec!["workspace".into(), "ws".into()],
                plugin: String::new(),
            },
        ]
    }

    fn execute(&self, item: &ResultItem) -> Option<Result<(), String>> {
        if item.id == "rename" {
            return Some(workspace_launcher::snapshot().and_then(|snapshot| {
                let workspace = snapshot
                    .focused_workspace
                    .ok_or_else(|| "no focused workspace".to_owned())?;
                workspace_launcher::set_workspace_name(&workspace, &item.data)
            }));
        }
        if item.id == "unassign" {
            return Some(workspace_launcher::snapshot().and_then(|snapshot| {
                let workspace = snapshot
                    .focused_workspace
                    .ok_or_else(|| "no focused workspace".to_owned())?;
                workspace_launcher::unassign_workspace_project(&workspace)
            }));
        }
        if let Some(path) = item.id.strip_prefix("project:") {
            let _ = path;
            return Some(workspace_launcher::snapshot().and_then(|snapshot| {
                let workspace = snapshot
                    .focused_workspace
                    .ok_or_else(|| "no focused workspace".to_owned())?;
                workspace_launcher::bind_workspace_project(&workspace, &item.data)
            }));
        }
        if item.id.starts_with("workspace:") {
            let (output, index) = item.data.split_once('\u{1f}').unwrap_or_default();
            return Some(
                index
                    .parse::<u32>()
                    .map_err(|error| error.to_string())
                    .and_then(|index| workspace_launcher::focus_workspace(output, index)),
            );
        }
        Some(Err(format!("Unknown workspace item '{}'", item.id)))
    }

    fn query(&self, context: &QueryContext<'_>) -> PluginResponse {
        if !matches!(context.group, Some(PROJECTS_GROUP | WORKSPACES_GROUP)) {
            return PluginResponse::default();
        }
        let snapshot = match workspace_launcher::snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                eprintln!("[workspaces-plugin] {error}");
                return PluginResponse::default();
            }
        };

        if context.group == Some(WORKSPACES_GROUP) {
            let query = context.query.trim().to_lowercase();
            let words = query.split_whitespace().collect::<Vec<_>>();
            let current_output = context
                .params
                .iter()
                .any(|param| {
                    matches!(
                        param.as_str(),
                        "current-output" | "current-monitor" | "focused-output"
                    )
                })
                .then_some(snapshot.focused_output.as_deref())
                .flatten();
            return PluginResponse {
                view: Some(Self::workspace_view()),
                results: Self::workspace_results(&snapshot, &words, current_output),
            };
        }

        if snapshot.focused_workspace.is_none() {
            return PluginResponse::default();
        }
        PluginResponse {
            view: Some(Self::project_view(&snapshot)),
            results: Self::project_results(&snapshot, context.query.trim()),
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
        match workspace_launcher::remove_project(id) {
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
