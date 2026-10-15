use shell_core::source::{self, Observable, rx::Observable as _};
use shell_rx_macros::combine_latest;

use super::WindowNode;
use super::window_source::{WindowSnapshot, window_snapshots};
use super::workspace::{self, WorkspaceObject};

pub(super) type WorkspaceNode = WorkspaceEntry;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceEntry {
    pub(super) workspace: WorkspaceObject,
    index: u32,
    output_path: Option<String>,
}

pub(super) fn workspaces(output_name: Option<String>) -> Observable<Vec<WorkspaceNode>> {
    let output_path = output_name.as_deref().map(workspace::output_path_for_name);
    source::switch_map_list(workspace::workspaces(), workspace_entry)
        .map(move |workspaces| filter_workspaces_for_output(workspaces, output_path.as_deref()))
        .distinct_until_changed()
        .box_it()
}

fn workspace_entry(workspace: WorkspaceObject) -> Observable<WorkspaceEntry> {
    combine_latest!(
        workspace.index(),
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

fn selected_workspace_id(output_name: Option<String>) -> Observable<Option<String>> {
    source::switch_map(workspace::current_workspace(output_name), |workspace| {
        workspace
            .map(|w| w.id().map(Some).box_it())
            .unwrap_or_else(|| source::once(None))
    })
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
            "[bar] no workspaces matched output {output_path}; showing unfiltered workspaces"
        );
        sort_workspaces(&mut workspaces);
        workspaces
    } else {
        sort_workspaces(&mut filtered);
        filtered
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
        filter_workspaces_for_output, selected_workspace_sorted_windows, workspace_matches_output,
    };
    use crate::widgets::bar::{
        window_source::WindowSnapshot,
        workspace::{WindowObject, WorkspaceObject},
    };
    use shell_core::source::dbus::ObjectModel;
    use zbus::zvariant::OwnedObjectPath;

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
    fn output_filter_falls_back_to_all_workspaces_when_connector_misses() {
        let workspaces = vec![workspace(
            1,
            Some("/org/rsynapse/Workspace/Outputs/x44502D32"),
        )];

        assert_eq!(
            filter_workspaces_for_output(
                workspaces.clone(),
                Some("/org/rsynapse/Workspace/Outputs/x4D49534D41544348")
            ),
            workspaces
        );
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
