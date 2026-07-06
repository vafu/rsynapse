use shell_core::source::{self, Observable, rx::Observable as _};

use super::niri::{self, NiriWindow, NiriWorkspace};
use super::window_source::window_snapshots;

pub(super) type WorkspaceNode = WorkspaceEntry;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceEntry {
    pub(super) workspace: NiriWorkspace,
    index: u32,
}

pub(super) fn workspaces() -> Observable<Vec<WorkspaceNode>> {
    source::switch_map_list(niri::workspaces(), workspace_entry)
        .map(|mut workspaces| {
            workspaces.sort_by(|left, right| {
                left.index
                    .cmp(&right.index)
                    .then_with(|| left.workspace.path_key().cmp(right.workspace.path_key()))
            });
            workspaces
        })
        .distinct_until_changed()
        .box_it()
}

fn workspace_entry(workspace: NiriWorkspace) -> Observable<WorkspaceEntry> {
    workspace
        .index()
        .map(move |index| WorkspaceEntry {
            workspace: workspace.clone(),
            index: u32::from(index),
        })
        .distinct_until_changed()
        .box_it()
}

fn selected_workspace() -> Observable<NiriWorkspace> {
    niri::focused_workspace()
        .filter_map(|workspace| workspace)
        .distinct_until_changed()
        .box_it()
}

pub(super) fn selected_workspace_windows() -> Observable<Vec<NiriWindow>> {
    selected_workspace()
        .switch_map(|workspace| workspace.id())
        .combine_latest(window_snapshots(), |selected_workspace_id, mut windows| {
            let _span = tracing::trace_span!(
                "bar.selected_workspace_windows",
                selected_workspace_id,
                input_windows = windows.len()
            )
            .entered();
            windows.retain(|window| window.workspace_id == Some(selected_workspace_id));
            windows.sort_by(|left, right| {
                (left.column, left.row, left.id)
                    .cmp(&(right.column, right.row, right.id))
                    .then_with(|| left.window.path_key().cmp(right.window.path_key()))
            });

            tracing::trace!(output_windows = windows.len(), "selected workspace windows");
            windows.into_iter().map(|window| window.window).collect()
        })
        .distinct_until_changed()
        .box_it()
}
