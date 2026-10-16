use std::collections::BTreeMap;

use super::WorkspaceEntry;

/// Only empty anonymous edges are placeholder candidates. The row separately
/// keeps the selected workspace and project-associated rows visible.
pub(super) fn mark_edge_placeholders(workspaces: &mut [WorkspaceEntry]) {
    let mut edges = BTreeMap::<String, (u32, u32)>::new();
    for workspace in workspaces.iter() {
        if let Some(output) = &workspace.output_path {
            let edge = edges
                .entry(output.clone())
                .or_insert((workspace.index, workspace.index));
            edge.0 = edge.0.min(workspace.index);
            edge.1 = edge.1.max(workspace.index);
        }
    }
    for workspace in workspaces {
        workspace.edge_placeholder = !workspace.has_windows
            && !workspace.named
            && workspace
                .output_path
                .as_ref()
                .and_then(|output| edges.get(output))
                .is_some_and(|&(first, last)| workspace.index == first || workspace.index == last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::bar::workspace::WorkspaceObject;
    use shell_core::source::dbus::ObjectModel;

    fn entry(id: &str, output: &str, index: u32, has_windows: bool, named: bool) -> WorkspaceEntry {
        WorkspaceEntry {
            workspace: WorkspaceObject::at(workspace_dbus_api::workspace_path(id)),
            index,
            output_path: Some(output.into()),
            has_windows,
            named,
            edge_placeholder: false,
        }
    }

    #[test]
    fn only_anonymous_empty_edges_are_placeholders_per_output() {
        // Unsorted input, gaps in indices and independent monitor inventories.
        let mut rows = vec![
            entry("interior", "A", 3, false, false),
            entry("tail", "A", 8, false, false),
            entry("other-named", "B", 1, false, true),
            entry("head", "A", 1, false, false),
            entry("other-occupied", "B", 4, true, false),
        ];
        mark_edge_placeholders(&mut rows);
        assert_eq!(
            rows.iter()
                .map(|row| row.edge_placeholder)
                .collect::<Vec<_>>(),
            [false, true, false, true, false]
        );

        // The same empty interior becomes an edge after its neighbors disappear.
        rows.retain(|row| {
            row.workspace.path_key() == workspace_dbus_api::workspace_path("interior").as_str()
        });
        mark_edge_placeholders(&mut rows);
        assert!(rows[0].edge_placeholder);
    }

    #[test]
    fn missing_output_is_not_evidence_of_a_placeholder() {
        let mut row = entry("pending", "A", 1, false, false);
        row.output_path = None;
        mark_edge_placeholders(std::slice::from_mut(&mut row));
        assert!(!row.edge_placeholder);
    }
}
