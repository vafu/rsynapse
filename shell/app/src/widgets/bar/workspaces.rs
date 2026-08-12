use shell_core::source::{self, Observable, rx::Observable as _};
use shell_rx_macros::combine_latest;

use super::WindowNode;
use super::niri::{self, NiriWorkspace};
use super::window_source::{WindowSnapshot, window_snapshots};
use super::workspace_sync::{self, WorkspaceSyncMode};

pub(super) type WorkspaceNode = WorkspaceEntry;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceEntry {
    pub(super) workspace: NiriWorkspace,
    index: u32,
    output_path: Option<String>,
}

pub(super) fn workspaces(
    output_name: Option<String>,
    primary_output_name: Option<String>,
) -> Observable<Vec<WorkspaceNode>> {
    let output_path = output_name.as_deref().map(niri::output_path_for_name);
    let primary_output_path = primary_output_name
        .as_deref()
        .map(niri::output_path_for_name);
    source::switch_map_list(niri::workspaces(), workspace_entry)
        .combine_latest(workspace_sync::mode(), move |workspaces, mode| {
            let output_path = effective_workspace_output_path(
                output_path.as_deref(),
                primary_output_path.as_deref(),
                mode,
            );
            filter_workspaces_for_output(workspaces, output_path)
        })
        .distinct_until_changed()
        .box_it()
}

fn workspace_entry(workspace: NiriWorkspace) -> Observable<WorkspaceEntry> {
    combine_latest!(
        workspace.index().map(u32::from),
        workspace.output_path_key()
            => move |(index, output_path)| WorkspaceEntry {
                workspace: workspace.clone(),
                index,
                output_path,
            },
    )
    .distinct_until_changed()
    .box_it()
}

fn selected_workspace_id(output_name: Option<String>) -> Observable<Option<u64>> {
    niri::current_workspace(output_name)
        .map(|workspace| workspace.and_then(|workspace| workspace.path_id()))
        .distinct_until_changed()
        .box_it()
}

pub(super) fn selected_workspace_windows(
    output_name: Option<String>,
) -> Observable<Vec<WindowNode>> {
    selected_workspace_id(output_name)
        .combine_latest(window_snapshots(), |selected_workspace_id, windows| {
            let _span = tracing::trace_span!(
                "bar.selected_workspace_windows",
                selected_workspace_id,
                input_windows = windows.len()
            )
            .entered();
            let windows = selected_workspace_sorted_windows(selected_workspace_id, windows)
                .into_iter()
                .map(|window| window.window)
                .collect::<Vec<_>>();

            tracing::trace!(output_windows = windows.len(), "selected workspace windows");
            windows
        })
        .distinct_until_changed()
        .box_it()
}

fn selected_workspace_sorted_windows(
    selected_workspace_id: Option<u64>,
    mut windows: Vec<WindowSnapshot>,
) -> Vec<WindowSnapshot> {
    windows.retain(|window| window.workspace_id == selected_workspace_id);
    windows.sort_by(|left, right| {
        (left.column, left.row, left.id)
            .cmp(&(right.column, right.row, right.id))
            .then_with(|| left.window.path_key().cmp(right.window.path_key()))
    });
    windows
}

fn filter_workspaces_for_output(
    mut workspaces: Vec<WorkspaceEntry>,
    output_path: Option<&str>,
) -> Vec<WorkspaceEntry> {
    let Some(output_path) = output_path else {
        sort_workspaces(&mut workspaces);
        return workspaces;
    };

    let mut filtered: Vec<_> = workspaces
        .iter()
        .filter(|workspace| {
            workspace_matches_output(workspace.output_path.as_deref(), Some(output_path))
        })
        .cloned()
        .collect();

    if filtered.is_empty() && !workspaces.is_empty() {
        eprintln!(
            "[bar] no workspaces matched niri output {output_path}; showing unfiltered workspaces"
        );
        sort_workspaces(&mut workspaces);
        workspaces
    } else {
        sort_workspaces(&mut filtered);
        filtered
    }
}

fn effective_workspace_output_path<'a>(
    output_path: Option<&'a str>,
    primary_output_path: Option<&'a str>,
    mode: WorkspaceSyncMode,
) -> Option<&'a str> {
    if mode.mirrors_workspace_rail()
        && primary_output_path.is_some()
        && output_path != primary_output_path
    {
        primary_output_path
    } else {
        output_path
    }
}

fn workspace_matches_output(
    workspace_output_path: Option<&str>,
    filter_path: Option<&str>,
) -> bool {
    filter_path.is_none_or(|filter_path| workspace_output_path == Some(filter_path))
}

fn sort_workspaces(workspaces: &mut [WorkspaceEntry]) {
    workspaces.sort_by(|left, right| {
        left.index
            .cmp(&right.index)
            .then_with(|| left.workspace.path_key().cmp(right.workspace.path_key()))
    });
}

#[cfg(test)]
mod tests {
    use super::{
        effective_workspace_output_path, filter_workspaces_for_output,
        selected_workspace_sorted_windows, workspace_matches_output,
    };
    use crate::widgets::bar::{
        niri::{NiriWindow, NiriWorkspace},
        window_source::WindowSnapshot,
        workspace_sync::WorkspaceSyncMode,
    };
    use shell_core::source::dbus::ObjectModel;
    use zbus::zvariant::OwnedObjectPath;

    #[test]
    fn output_filter_matches_only_same_output_when_set() {
        assert!(workspace_matches_output(
            Some("/org/rsynapse/Niri/Outputs/x6544502D31"),
            Some("/org/rsynapse/Niri/Outputs/x6544502D31")
        ));
        assert!(!workspace_matches_output(
            Some("/org/rsynapse/Niri/Outputs/x48444D492D412D31"),
            Some("/org/rsynapse/Niri/Outputs/x6544502D31")
        ));
        assert!(!workspace_matches_output(
            None,
            Some("/org/rsynapse/Niri/Outputs/x6544502D31")
        ));
    }

    #[test]
    fn output_filter_allows_all_workspaces_without_monitor_name() {
        assert!(workspace_matches_output(
            Some("/org/rsynapse/Niri/Outputs/x6544502D31"),
            None
        ));
        assert!(workspace_matches_output(None, None));
    }

    #[test]
    fn output_filter_falls_back_to_all_workspaces_when_connector_misses() {
        let workspaces = vec![workspace(1, Some("/org/rsynapse/Niri/Outputs/x44502D32"))];

        assert_eq!(
            filter_workspaces_for_output(
                workspaces.clone(),
                Some("/org/rsynapse/Niri/Outputs/x4D49534D41544348")
            ),
            workspaces
        );
    }

    #[test]
    fn mirror_modes_use_primary_output_for_secondary_rails() {
        assert_eq!(
            effective_workspace_output_path(
                Some("secondary"),
                Some("primary"),
                WorkspaceSyncMode::Off,
            ),
            Some("secondary")
        );
        assert_eq!(
            effective_workspace_output_path(
                Some("secondary"),
                Some("primary"),
                WorkspaceSyncMode::MirrorBar,
            ),
            Some("primary")
        );
        assert_eq!(
            effective_workspace_output_path(
                Some("secondary"),
                Some("primary"),
                WorkspaceSyncMode::AgentSidecar,
            ),
            Some("primary")
        );
        assert_eq!(
            effective_workspace_output_path(
                Some("primary"),
                Some("primary"),
                WorkspaceSyncMode::MirrorBar
            ),
            Some("primary")
        );
    }

    #[test]
    fn selected_workspace_windows_are_inline_in_niri_layout_order() {
        let selected_workspace_id = Some(7);
        let windows = vec![
            window_snapshot(30, selected_workspace_id, 2, 0),
            window_snapshot(20, selected_workspace_id, 1, 1),
            window_snapshot(40, Some(8), 1, 0),
            window_snapshot(10, selected_workspace_id, 1, 0),
        ];

        let windows = selected_workspace_sorted_windows(selected_workspace_id, windows);

        assert_eq!(
            window_paths(&windows),
            vec![window_path(10), window_path(20), window_path(30)]
        );
    }

    #[test]
    fn inline_windows_use_id_to_order_unknown_positions() {
        let selected_workspace_id = Some(7);
        let windows = vec![
            window_snapshot(20, selected_workspace_id, u64::MAX, u64::MAX),
            window_snapshot(10, selected_workspace_id, u64::MAX, u64::MAX),
        ];

        let windows = selected_workspace_sorted_windows(selected_workspace_id, windows);

        assert_eq!(
            window_paths(&windows),
            vec![window_path(10), window_path(20)]
        );
    }

    fn workspace(index: u32, output_path: Option<&str>) -> super::WorkspaceEntry {
        super::WorkspaceEntry {
            workspace: niri_workspace(index),
            index,
            output_path: output_path.map(str::to_owned),
        }
    }

    fn window_snapshot(
        id: u64,
        workspace_id: Option<u64>,
        column: u64,
        row: u64,
    ) -> WindowSnapshot {
        WindowSnapshot {
            window: NiriWindow::at(OwnedObjectPath::try_from(window_path(id)).unwrap()),
            workspace_id,
            column,
            row,
            id,
            app_id: None,
        }
    }

    fn window_paths(windows: &[WindowSnapshot]) -> Vec<String> {
        windows
            .iter()
            .map(|window| window.window.path_key().to_owned())
            .collect()
    }

    fn window_path(id: u64) -> String {
        format!("/org/rsynapse/Niri/Windows/window_{id}")
    }

    fn niri_workspace(index: u32) -> NiriWorkspace {
        NiriWorkspace::at(
            OwnedObjectPath::try_from(format!("/org/rsynapse/Niri/Workspaces/workspace_{index}"))
                .unwrap(),
        )
    }
}
