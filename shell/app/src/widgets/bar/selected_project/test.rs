use super::selected_project_view;
use crate::widgets::bar::project::ProjectDetails;

#[test]
fn selected_project_displays_project_metadata() {
    let view = selected_project_view(split_project());

    assert!(view.visible);
    assert_eq!(view.title, "platform/taskexecution");
    assert_eq!(view.branch.as_deref(), Some("codex/android-core-isol"));
}

#[test]
fn selected_project_hides_without_project_metadata() {
    let view = selected_project_view(ProjectDetails::default());

    assert!(!view.visible);
    assert_eq!(view.title, "");
    assert_eq!(view.branch, None);
}

#[test]
fn selected_project_uses_root_cwd_name_without_relative_cwd() {
    let view = selected_project_view(ProjectDetails {
        has_project: true,
        cwd_label: Some("uiq-worktree".to_owned()),
        branch: Some("vafu/coroutines/rescue-scheduler".to_owned()),
        ..ProjectDetails::default()
    });

    assert!(view.visible);
    assert_eq!(view.title, "uiq-worktree");
    assert_eq!(
        view.branch.as_deref(),
        Some("vafu/coroutines/rescue-scheduler")
    );
}

#[test]
fn selected_project_exposes_branch_for_clipboard() {
    let view = split_project();

    assert_eq!(
        super::branch_for_clipboard(view.branch.as_deref()),
        Some("codex/android-core-isol")
    );
}

#[test]
fn selected_project_shows_only_feature_for_vafu_worktree_branch() {
    let view = selected_project_view(ProjectDetails {
        has_project: true,
        cwd_label: Some("rsynapse".to_owned()),
        branch: Some("vafu/rsynapse/disk-widget".to_owned()),
        ..ProjectDetails::default()
    });

    assert_eq!(view.branch.as_deref(), Some("disk-widget"));
}

#[test]
fn selected_project_keeps_vafu_branch_when_worktree_does_not_match_cwd() {
    let view = selected_project_view(ProjectDetails {
        has_project: true,
        cwd_label: Some("rsynapse".to_owned()),
        branch: Some("vafu/other/disk-widget".to_owned()),
        ..ProjectDetails::default()
    });

    assert_eq!(view.branch.as_deref(), Some("vafu/other/disk-widget"));
}

#[test]
fn selected_project_keeps_non_vafu_branch_name() {
    let view = selected_project_view(ProjectDetails {
        has_project: true,
        cwd_label: Some("rsynapse".to_owned()),
        branch: Some("main".to_owned()),
        ..ProjectDetails::default()
    });

    assert_eq!(view.branch.as_deref(), Some("main"));
}

fn split_project() -> ProjectDetails {
    ProjectDetails {
        has_project: true,
        display_main: Some("android".to_owned()),
        display_secondary: Some("core-isol".to_owned()),
        branch: Some("codex/android-core-isol".to_owned()),
        cwd_label: Some("platform/taskexecution".to_owned()),
        ..ProjectDetails::default()
    }
}
