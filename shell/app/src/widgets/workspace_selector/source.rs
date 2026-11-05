//! Reactive state for the focused workspace's project/name picker.
use shell_core::source::{self, Observable, rx::Observable as _};
use shell_rx_macros::combine_latest;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct View {
    pub workspace_id: Option<String>,
    pub workspace_name: Option<String>,
    pub current_project_id: Option<String>,
    pub current_project_name: Option<String>,
    pub projects: Vec<Project>,
}

pub(super) fn view() -> Observable<View> {
    source::switch_map(source::workspace::focused_workspace(), move |workspace| {
        let Some(workspace) = workspace else {
            return source::once(View::default());
        };

        let projects = source::proj::projects()
            .map(|projects| {
                projects
                    .into_iter()
                    .map(|project| Project {
                        id: project.id,
                        name: project.name,
                        path: project.cwd,
                    })
                    .collect::<Vec<_>>()
            })
            .distinct_until_changed()
            .box_it();
        let id_for_name = workspace.id();
        let id = workspace.id();
        let name = source::switch_map(id_for_name, |id| {
            source::workspace_name::workspace_display_name(Some(id))
        });
        let project = source::workspace_project::workspace_project(workspace);
        combine_latest!(
            id,
            name,
            project,
            projects => |(id, name, project, projects)| View {
                workspace_id: Some(id),
                workspace_name: name,
                current_project_id: project.project_id,
                current_project_name: project.project.as_ref().map(|project| project.name.clone()),
                projects,
            }
        )
        .box_it()
    })
    .distinct_until_changed()
    .box_it()
}
