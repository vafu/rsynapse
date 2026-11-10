use data_sources::{self, Observable, dbus, rx::Observable as _};
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
///
/// The inner graph clears to None while a new workspace's association loads so
/// the project label never renders stale data. Consumers that must not flicker
/// for an unchanged project use [`selected_project_scope`] instead.
fn project_context(output: Option<String>) -> Observable<Option<SelectedWorkspaceView>> {
    // project::send_project currently maps ServiceUnknown to no association.
    // Gate that producer's absence here so it cannot enable global GitHub
    // fallback. Other producer errors do not emit a no-association value.
    let owners: Observable<(Option<String>, Option<String>)> = combine_latest!(
        dbus::service_owner(dbus::Bus::Session, workspace_dbus_api::BUS_NAME),
        dbus::service_owner(dbus::Bus::Session, locus::BUS_NAME)
        => |(workspace, locus)| (workspace, locus),
    )
    .box_it();
    data_sources::switch_map(owners, move |(workspace_owner, locus_owner)| {
        if workspace_owner.is_none() || locus_owner.is_none() {
            return data_sources::once(None);
        }
        data_sources::switch_map(current_workspace(output.clone()), |workspace| {
            let Some(selected) = workspace else {
                return data_sources::once(None);
            };
            let names = data_sources::switch_map(selected.id(), |id| {
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
}

pub(in crate::widgets::bar) fn selected_project_context(
    output: Option<String>,
) -> Observable<Option<SelectedWorkspaceView>> {
    data_sources::shared_by_key(
        "rsynapse.selected-project",
        format!("{output:?}"),
        move || project_context(output.clone()),
    )
}

/// Association projection that keeps the current value while the same project
/// reloads across a workspace switch.
///
/// `project_context` clears to None while loading, which is correct for surfaces
/// that show the workspace title but would reseed downstream lists (such as the
/// GitHub scope) even when the project did not change. This retains the previous
/// association unless the resolved project identity actually changes.
pub(in crate::widgets::bar) fn selected_project_scope(
    output: Option<String>,
) -> Observable<Option<SelectedWorkspaceView>> {
    data_sources::shared_by_key(
        "rsynapse.selected-project-scope",
        format!("{output:?}"),
        move || retain_association(project_context(output.clone())),
    )
}

/// Holds the current association while the source reloads to `None`.
///
/// Only applied to the scope projection: the project label intentionally clears
/// on reload, whereas GitHub must not reseed its lists for an unchanged project.
fn retain_association(
    source: Observable<Option<SelectedWorkspaceView>>,
) -> Observable<Option<SelectedWorkspaceView>> {
    source
        // A held `None` yields a duplicate `Some`, which the distinct filter
        // drops, so an intra-project reload emits nothing.
        .scan(
            Option::<SelectedWorkspaceView>::None,
            |current, next| match next {
                None => current,
                Some(view) => Some(view),
            },
        )
        .distinct_until_changed()
        .box_it()
}

fn workspace_project_status(workspace: WorkspaceObject) -> Observable<ProjectView> {
    data_sources::switch_map(project_details(workspace), |project| {
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
            None => data_sources::once(project_view(project, None)),
        }
    })
    .distinct_until_changed()
    .box_it()
}

#[cfg(test)]
mod test {
    use super::retain_association;
    use crate::widgets::bar::selected_project::{ProjectView, SelectedWorkspaceView};
    use data_sources::{StateSignal, rx::Observable as _};
    use futures_util::{Stream, StreamExt};
    use std::time::Duration;

    fn view(workspace: &str, project_id: &str) -> Option<SelectedWorkspaceView> {
        Some(SelectedWorkspaceView {
            workspace_id: Some(workspace.into()),
            visible: true,
            project: Some(ProjectView {
                project_id: Some(project_id.into()),
                visible: true,
                ..Default::default()
            }),
            ..Default::default()
        })
    }

    async fn next(
        stream: &mut (impl Stream<Item = Result<Option<SelectedWorkspaceView>, String>> + Unpin),
    ) -> Option<SelectedWorkspaceView> {
        tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("source did not update")
            .unwrap()
            .unwrap()
    }

    #[test]
    fn holds_previous_association_across_same_project_reload() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let source = StateSignal::new(view("a", "project-1"));
            let mut stream = retain_association(source.observable()).into_stream();
            assert_eq!(next(&mut stream).await, view("a", "project-1"));

            // Workspace switch clears the graph. That must not surface as None.
            source.set(None);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), stream.next())
                    .await
                    .is_err(),
                "a reload must not emit None"
            );

            // The same project on the new workspace re-emits its own view, but
            // never a None in between.
            source.set(view("b", "project-1"));
            assert_eq!(next(&mut stream).await, view("b", "project-1"));

            // A genuine project change is followed.
            source.set(None);
            source.set(view("c", "project-2"));
            assert_eq!(next(&mut stream).await, view("c", "project-2"));
        });
    }
}
