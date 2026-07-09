use shell_core::source::{self, Observable, rx::Observable as _};
use shell_rx_macros::combine_latest;

use super::niri::{self, NiriWindow, NiriWorkspace};
use super::window_source::window_snapshots;

pub(super) type WorkspaceNode = WorkspaceEntry;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceEntry {
    pub(super) workspace: NiriWorkspace,
    index: u32,
    output_path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WorkspaceSelectionEntry {
    workspace: NiriWorkspace,
    output_path: Option<String>,
    selected: bool,
}

pub(super) fn workspaces(output_name: Option<String>) -> Observable<Vec<WorkspaceNode>> {
    let output_path = output_name.as_deref().map(niri::output_path_for_name);
    source::switch_map_list(niri::workspaces(), workspace_entry)
        .map(move |workspaces| filter_workspaces_for_output(workspaces, output_path.as_deref()))
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

fn workspace_selection_entry(workspace: NiriWorkspace) -> Observable<WorkspaceSelectionEntry> {
    combine_latest!(
        workspace.output_path_key(),
        workspace.active()
            => move |(output_path, selected)| WorkspaceSelectionEntry {
                workspace: workspace.clone(),
                output_path,
                selected,
            },
    )
    .distinct_until_changed()
    .box_it()
}

fn selected_workspace(output_name: Option<String>) -> Observable<Option<NiriWorkspace>> {
    let Some(output_name) = output_name else {
        return niri::focused_workspace().distinct_until_changed().box_it();
    };

    let output_path = niri::output_path_for_name(&output_name);

    source::switch_map_list(niri::workspaces(), workspace_selection_entry)
        .map(move |workspaces| active_workspace_for_output(workspaces, output_path.as_str()))
        .distinct_until_changed()
        .box_it()
}

pub(super) fn selected_workspace_windows(
    output_name: Option<String>,
) -> Observable<Vec<NiriWindow>> {
    selected_workspace(output_name)
        .switch_map(|workspace| {
            workspace
                .map(|workspace| workspace.id().map(Some).box_it())
                .unwrap_or_else(|| source::once(None))
        })
        .combine_latest(window_snapshots(), |selected_workspace_id, mut windows| {
            let _span = tracing::trace_span!(
                "bar.selected_workspace_windows",
                selected_workspace_id,
                input_windows = windows.len()
            )
            .entered();
            windows.retain(|window| window.workspace_id == selected_workspace_id);
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

fn active_workspace_for_output(
    workspaces: Vec<WorkspaceSelectionEntry>,
    output_path: &str,
) -> Option<NiriWorkspace> {
    let active_workspace = workspaces.iter().find(|workspace| {
        workspace.selected
            && workspace_matches_output(workspace.output_path.as_deref(), Some(output_path))
    });

    active_workspace
        .or_else(|| {
            workspaces.iter().find(|workspace| {
                workspace.selected
                    && !workspaces.iter().any(|candidate| {
                        workspace_matches_output(
                            candidate.output_path.as_deref(),
                            Some(output_path),
                        )
                    })
            })
        })
        .map(|workspace| workspace.workspace.clone())
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
        active_workspace_for_output, filter_workspaces_for_output, workspace_matches_output,
    };
    use crate::widgets::bar::niri::NiriWorkspace;
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
    fn selected_workspace_falls_back_to_active_workspace_when_connector_misses() {
        let workspaces = vec![selection_workspace(
            1,
            Some("/org/rsynapse/Niri/Outputs/x44502D32"),
            true,
        )];

        assert_eq!(
            active_workspace_for_output(
                workspaces.clone(),
                "/org/rsynapse/Niri/Outputs/x4D49534D41544348"
            ),
            Some(workspaces[0].workspace.clone())
        );
    }

    fn workspace(index: u32, output_path: Option<&str>) -> super::WorkspaceEntry {
        super::WorkspaceEntry {
            workspace: niri_workspace(index),
            index,
            output_path: output_path.map(str::to_owned),
        }
    }

    fn selection_workspace(
        index: u32,
        output_path: Option<&str>,
        selected: bool,
    ) -> super::WorkspaceSelectionEntry {
        super::WorkspaceSelectionEntry {
            workspace: niri_workspace(index),
            output_path: output_path.map(str::to_owned),
            selected,
        }
    }

    fn niri_workspace(index: u32) -> NiriWorkspace {
        NiriWorkspace::at(
            OwnedObjectPath::try_from(format!("/org/rsynapse/Niri/Workspaces/workspace_{index}"))
                .unwrap(),
        )
    }
}
