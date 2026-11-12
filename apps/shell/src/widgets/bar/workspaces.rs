use data_sources::{self, Observable, rx::Observable as _};
use shell_rx_macros::combine_latest;

mod visibility;

use std::collections::HashMap;

use super::WindowNode;
use super::window_source::{WindowSnapshot, window_snapshots};
use super::workspace::{self, WorkspaceObject};

pub(super) type WorkspaceNode = WorkspaceRow;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceRow {
    pub(super) workspace: WorkspaceObject,
    pub(super) edge_placeholder: bool,
}

impl WorkspaceRow {
    pub(super) fn identity(&self) -> String {
        self.workspace.path_key().to_owned()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WorkspaceEntry {
    pub(super) workspace: WorkspaceObject,
    index: u32,
    output_path: Option<String>,
    has_windows: bool,
    named: bool,
    pub(super) edge_placeholder: bool,
}

pub(super) fn workspaces(output_name: Option<String>) -> Observable<Vec<WorkspaceNode>> {
    let output_path = output_name.as_deref().map(workspace::output_path_for_name);
    let order = match output_name {
        Some(output_name) => workspace::output_workspaces(output_name).map(Some).box_it(),
        None => data_sources::once(None).box_it(),
    };
    data_sources::switch_map_list(workspace::workspaces(), workspace_entry)
        .combine_latest(order, |workspaces, order| (workspaces, order))
        .map(move |workspaces| {
            let (workspaces, order) = workspaces;
            let mut workspaces = filter_workspaces_for_output(
                workspaces,
                output_path.as_deref(),
                order.as_deref(),
            );
            visibility::mark_edge_placeholders(&mut workspaces);
            workspaces
                .into_iter()
                .map(|workspace| WorkspaceRow {
                    workspace: workspace.workspace,
                    edge_placeholder: workspace.edge_placeholder,
                })
                .collect()
        })
        .distinct_until_changed()
        .box_it()
}

fn workspace_entry(workspace: WorkspaceObject) -> Observable<WorkspaceEntry> {
    combine_latest!(
        workspace.index(),
        workspace.output_path_key(),
        workspace.has_windows(),
        workspace.name()
            => move |(index, output_path, has_windows, name)| WorkspaceEntry {
                workspace: workspace.clone(),
                index,
                output_path,
                has_windows,
                named: name.is_some(),
                edge_placeholder: false,
            },
    )
    .distinct_until_changed()
    .box_it()
}

fn selected_workspace_id(output_name: Option<String>) -> Observable<Option<String>> {
    data_sources::switch_map(workspace::current_workspace(output_name), |workspace| {
        workspace
            .map(|w| w.id().map(Some).box_it())
            .unwrap_or_else(|| data_sources::once(None))
    })
    .distinct_until_changed()
    .box_it()
}

pub(super) fn selected_workspace_windows(
    output_name: Option<String>,
) -> Observable<Vec<WindowNode>> {
    let trace_output = output_name.clone();
    selected_workspace_id(output_name)
        .combine_latest(window_snapshots(), move |selected_workspace_id, windows| {
            let input_windows = windows.len();
            let _span = tracing::trace_span!(
                "bar.selected_workspace_windows",
                selected_workspace_id,
                input_windows
            )
            .entered();
            let selected_for_log = selected_workspace_id.clone();
            let windows = selected_workspace_sorted_windows(selected_workspace_id, windows)
                .into_iter()
                .map(|window| window.window)
                .collect::<Vec<_>>();
            let listed = windows
                .iter()
                .map(|window| window.path_key().to_owned())
                .collect::<Vec<_>>();
            tracing::trace!(
                output = ?trace_output,
                selected = ?selected_for_log,
                input = input_windows,
                listed = ?listed,
                output_windows = windows.len(),
                "selected workspace windows"
            );
            windows
        })
        .distinct_until_changed()
        .box_it()
}

fn selected_workspace_sorted_windows(
    selected_workspace_id: Option<String>,
    mut windows: Vec<WindowSnapshot>,
) -> Vec<WindowSnapshot> {
    windows.retain(|window| window.workspace_id == selected_workspace_id);
    windows.sort_by(|left, right| {
        (left.column, left.row, &left.id)
            .cmp(&(right.column, right.row, &right.id))
            .then_with(|| left.window.path_key().cmp(right.window.path_key()))
    });
    windows
}

fn filter_workspaces_for_output(
    mut workspaces: Vec<WorkspaceEntry>,
    output_path: Option<&str>,
    order: Option<&[WorkspaceObject]>,
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

    // During hotplug a monitor and its projected output can appear/disappear
    // in either order. Never render other outputs' workspaces as a fallback.
    sort_workspaces_by_output_order(&mut filtered, order);
    filtered
}

fn sort_workspaces_by_output_order(
    workspaces: &mut [WorkspaceEntry],
    order: Option<&[WorkspaceObject]>,
) {
    let Some(order) = order else {
        sort_workspaces(workspaces);
        return;
    };
    let positions = order
        .iter()
        .enumerate()
        .map(|(index, workspace)| (workspace.path_key(), index))
        .collect::<HashMap<_, _>>();
    workspaces.sort_by(|left, right| {
        position_of(left, &positions)
            .cmp(&position_of(right, &positions))
            .then_with(|| left.index.cmp(&right.index))
            .then_with(|| left.workspace.path_key().cmp(right.workspace.path_key()))
    });
}

fn position_of(
    workspace: &WorkspaceEntry,
    positions: &HashMap<&str, usize>,
) -> usize {
    positions
        .get(workspace.workspace.path_key())
        .copied()
        .unwrap_or(usize::MAX)
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
        WorkspaceRow, filter_workspaces_for_output, selected_workspace_sorted_windows,
        workspace_matches_output,
    };
    use crate::widgets::bar::{
        window_source::WindowSnapshot,
        workspace::{WindowObject, WorkspaceObject},
    };
    use data_sources::dbus::ObjectModel;
    use data_sources::zvariant::OwnedObjectPath;

    #[test]
    fn output_filter_matches_only_same_output_when_set() {
        assert!(workspace_matches_output(
            Some("/org/rsynapse/Workspace/Outputs/x6544502D31"),
            Some("/org/rsynapse/Workspace/Outputs/x6544502D31")
        ));
        assert!(!workspace_matches_output(
            Some("/org/rsynapse/Workspace/Outputs/x48444D492D412D31"),
            Some("/org/rsynapse/Workspace/Outputs/x6544502D31")
        ));
        assert!(!workspace_matches_output(
            None,
            Some("/org/rsynapse/Workspace/Outputs/x6544502D31")
        ));
    }

    #[test]
    fn output_filter_allows_all_workspaces_without_monitor_name() {
        assert!(workspace_matches_output(
            Some("/org/rsynapse/Workspace/Outputs/x6544502D31"),
            None
        ));
        assert!(workspace_matches_output(None, None));
    }

    #[test]
    fn missing_output_does_not_expose_another_outputs_workspace_targets() {
        let workspaces = vec![workspace(
            1,
            Some("/org/rsynapse/Workspace/Outputs/x44502D32"),
        )];

        assert_eq!(
            filter_workspaces_for_output(
                workspaces.clone(),
                Some("/org/rsynapse/Workspace/Outputs/x4D49534D41544348"),
                None,
            ),
            Vec::new()
        );
    }

    #[test]
    fn output_order_wins_over_stale_workspace_indexes() {
        let first = workspace(2, Some("/org/rsynapse/Workspace/Outputs/x44502D32"));
        let second = workspace(1, Some("/org/rsynapse/Workspace/Outputs/x44502D32"));
        let order = vec![second.workspace.clone(), first.workspace.clone()];

        let filtered = filter_workspaces_for_output(
            vec![first, second],
            Some("/org/rsynapse/Workspace/Outputs/x44502D32"),
            Some(&order),
        );

        assert_eq!(
            filtered
                .iter()
                .map(|workspace| workspace.workspace.path_key().to_owned())
                .collect::<Vec<_>>(),
            order
                .iter()
                .map(|workspace| workspace.path_key().to_owned())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn output_order_places_unlisted_workspaces_last() {
        let listed = workspace(2, Some("/org/rsynapse/Workspace/Outputs/x44502D32"));
        let unlisted = workspace(1, Some("/org/rsynapse/Workspace/Outputs/x44502D32"));
        let order = vec![listed.workspace.clone()];

        let filtered = filter_workspaces_for_output(
            vec![unlisted, listed],
            Some("/org/rsynapse/Workspace/Outputs/x44502D32"),
            Some(&order),
        );

        assert_eq!(filtered[0].workspace.path_key(), order[0].path_key());
    }

    #[test]
    fn selected_workspace_windows_are_inline_in_scrolling_layout_order() {
        let selected_workspace_id = Some(7);
        let windows = vec![
            window_snapshot(30, selected_workspace_id, 2, 0),
            window_snapshot(20, selected_workspace_id, 1, 1),
            window_snapshot(40, Some(8), 1, 0),
            window_snapshot(10, selected_workspace_id, 1, 0),
        ];

        let windows = selected_workspace_sorted_windows(
            selected_workspace_id.map(|id| id.to_string()),
            windows,
        );

        assert_eq!(
            window_paths(&windows),
            vec![window_path(10), window_path(20), window_path(30)]
        );
    }

    #[test]
    fn workspace_identity_survives_reordering() {
        let first = WorkspaceRow {
            workspace: workspace(1, Some("/org/rsynapse/Workspace/Outputs/x44502D32")).workspace,
            edge_placeholder: false,
        };
        let second = WorkspaceRow {
            workspace: workspace(2, Some("/org/rsynapse/Workspace/Outputs/x44502D32")).workspace,
            edge_placeholder: false,
        };
        let first_identity = first.identity();
        let second_identity = second.identity();

        let mut reordered = vec![second, first];
        reordered.sort_by(|left, right| {
            left.workspace
                .path_key()
                .cmp(right.workspace.path_key())
        });

        assert_eq!(reordered[0].identity(), first_identity);
        assert_eq!(reordered[1].identity(), second_identity);
    }

    #[test]
    fn inline_windows_use_id_to_order_unknown_positions() {
        let selected_workspace_id = Some(7);
        let windows = vec![
            window_snapshot(20, selected_workspace_id, u64::MAX, u64::MAX),
            window_snapshot(10, selected_workspace_id, u64::MAX, u64::MAX),
        ];

        let windows = selected_workspace_sorted_windows(
            selected_workspace_id.map(|id| id.to_string()),
            windows,
        );

        assert_eq!(
            window_paths(&windows),
            vec![window_path(10), window_path(20)]
        );
    }

    fn workspace(index: u32, output_path: Option<&str>) -> super::WorkspaceEntry {
        super::WorkspaceEntry {
            workspace: workspace_object(index),
            index,
            output_path: output_path.map(str::to_owned),
            has_windows: true,
            named: false,
            edge_placeholder: false,
        }
    }

    fn window_snapshot(
        id: u64,
        workspace_id: Option<u64>,
        column: u64,
        row: u64,
    ) -> WindowSnapshot {
        WindowSnapshot {
            window: WindowObject::at(OwnedObjectPath::try_from(window_path(id)).unwrap()),
            workspace_id: workspace_id.map(|id| id.to_string()),
            column,
            row,
            id: id.to_string(),
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
        workspace_dbus_api::window_path(&id.to_string()).to_string()
    }

    fn workspace_object(index: u32) -> WorkspaceObject {
        WorkspaceObject::at(workspace_dbus_api::workspace_path(&index.to_string()))
    }
}
