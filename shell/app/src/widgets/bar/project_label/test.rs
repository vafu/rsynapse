use super::{ProjectLabelVm, workspace_visible};

#[test]
fn workspace_visible_hides_empty_unselected_edge_placeholder() {
    assert!(!workspace_visible(
        &ProjectLabelVm {
            empty: true,
            ..ProjectLabelVm::default()
        },
        false,
        true
    ));
}

#[test]
fn workspace_visible_keeps_empty_selected_workspace() {
    assert!(workspace_visible(
        &ProjectLabelVm {
            empty: true,
            ..ProjectLabelVm::default()
        },
        true,
        true
    ));
}

#[test]
fn workspace_visible_keeps_non_empty_unselected_workspace() {
    assert!(workspace_visible(
        &ProjectLabelVm {
            empty: false,
            ..ProjectLabelVm::default()
        },
        false,
        true
    ));
}

#[test]
fn workspace_visible_keeps_empty_interior_workspace() {
    assert!(workspace_visible(
        &ProjectLabelVm {
            empty: true,
            ..ProjectLabelVm::default()
        },
        false,
        false,
    ));
}
