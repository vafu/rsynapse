use shell_core::source::{self, Observable, dbus, rx::Observable as _};
use shell_rx_macros::combine_latest;

use super::{
    ProjectView, SelectedWorkspaceView, git::git_status, project_view,
    workspace::workspace_display_name,
};
use crate::widgets::bar::{
    project::project_details,
    workspace::{WorkspaceObject, current_workspace},
};

pub(in crate::widgets::bar) fn selected_project_status(
    output: Option<String>,
) -> Observable<SelectedWorkspaceView> {
    selected_project_context(output)
        .map(Option::unwrap_or_default)
        .distinct_until_changed()
        .box_it()
}

/// Shared title/association graph, without any window inventory dependency.
/// None means unresolved/unavailable, not a successfully unassociated workspace.
pub(in crate::widgets::bar) fn selected_project_context(
    output: Option<String>,
) -> Observable<Option<SelectedWorkspaceView>> {
    source::shared_by_key(
        "rsynapse.selected-project",
        format!("{output:?}"),
        move || {
            // project::send_project currently maps ServiceUnknown to no association.
            // Gate that producer's absence here so it cannot enable global GitHub
            // fallback. Other producer errors do not emit a no-association value.
            let owners: Observable<(Option<String>, Option<String>)> = combine_latest!(
                dbus::service_owner(dbus::Bus::Session, workspace_dbus_api::BUS_NAME),
                dbus::service_owner(dbus::Bus::Session, locus::BUS_NAME)
                => |(workspace, locus)| (workspace, locus),
            )
            .box_it();
            let output = output.clone();
            source::switch_map(owners, move |(workspace_owner, locus_owner)| {
                if workspace_owner.is_none() || locus_owner.is_none() {
                    return source::once(None);
                }
                source::switch_map(current_workspace(output.clone()), |workspace| {
                    let Some(selected) = workspace else {
                        return source::once(None);
                    };
                    let names = source::switch_map(selected.id(), |id| {
                        workspace_display_name(Some(id.clone()))
                            .map(move |name| (id.clone(), name))
                            .box_it()
                    });
                    combine_latest!(
                        names,
                        workspace_project_status(selected)
                        => |((id, name), project)| Some(SelectedWorkspaceView {
                            workspace_id: Some(id),
                            visible: true,
                            name,
                            project: project.visible.then_some(project),
                        }),
                    )
                    // Clear the old workspace while the new association loads.
                    .start_with(vec![None])
                    .box_it()
                })
                .start_with(vec![None])
                .box_it()
            })
            .start_with(vec![None])
            .distinct_until_changed()
            .box_it()
        },
    )
}

fn workspace_project_status(workspace: WorkspaceObject) -> Observable<ProjectView> {
    source::switch_map(project_details(workspace), |project| {
        match project.path.clone() {
            // Paint the association immediately, then fill in Git metadata.
            Some(path) => {
                let immediate = project_view(project.clone(), None);
                git_status(path)
                    .map(move |git| project_view(project.clone(), Some(git)))
                    .start_with(vec![immediate])
                    .distinct_until_changed()
                    .box_it()
            }
            None => source::once(project_view(project, None)),
        }
    })
    .distinct_until_changed()
    .box_it()
}
