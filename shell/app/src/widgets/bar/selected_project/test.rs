use super::git::GitStatus;
use super::{GitPart, ProjectView, SelectedWorkspaceView, git_loading, git_visible, project_view};
use crate::widgets::bar::project::ProjectDetails;

fn wrap(project: ProjectView) -> SelectedWorkspaceView {
    let visible = project.visible;
    SelectedWorkspaceView {
        visible,
        workspace_id: Some(7),
        name: None,
        project: visible.then_some(project),
    }
}

fn named(project: ProjectView, name: &str) -> SelectedWorkspaceView {
    SelectedWorkspaceView {
        visible: true,
        workspace_id: Some(7),
        name: Some(name.to_owned()),
        project: Some(project),
    }
}

#[test]
fn selected_project_displays_project_metadata() {
    let view = project_view(split_project(), None);

    assert!(view.visible);
    assert_eq!(view.title, "platform/taskexecution");
    assert_eq!(view.branch.as_deref(), Some("codex/android-core-isol"));
}

#[test]
fn selected_project_hides_without_project_metadata() {
    let view = project_view(ProjectDetails::default(), None);

    assert!(!view.visible);
    assert_eq!(view.title, "");
    assert_eq!(view.branch, None);
}

#[test]
fn selected_project_uses_root_cwd_name_without_relative_cwd() {
    let view = project_view(
        ProjectDetails {
            has_project: true,
            cwd_label: Some("uiq-worktree".to_owned()),
            branch: Some("vafu/coroutines/rescue-scheduler".to_owned()),
            ..ProjectDetails::default()
        },
        None,
    );

    assert!(view.visible);
    assert_eq!(view.title, "uiq-worktree");
    assert_eq!(
        view.branch.as_deref(),
        Some("vafu/coroutines/rescue-scheduler")
    );
}

#[test]
fn workspace_prefers_name_over_project_title() {
    let view = named(project_view(split_project(), None), "coro-uiq");

    assert!(super::visible(&view));
    assert_eq!(super::title_label(&view), "coro-uiq");
}

#[test]
fn workspace_falls_back_to_project_title_without_name() {
    let view = wrap(project_view(split_project(), None));

    assert_eq!(super::title_label(&view), "platform/taskexecution");
}

#[test]
fn workspace_shows_name_without_project() {
    let view = SelectedWorkspaceView {
        visible: true,
        workspace_id: Some(7),
        name: Some("noble-owl".to_owned()),
        project: None,
    };

    assert!(super::visible(&view));
    assert_eq!(super::title_label(&view), "noble-owl");
    assert!(!super::branch_visible(&view));
    assert!(!super::git_loading(&view));
}

#[test]
fn nameless_workspace_shows_empty_and_has_no_git_spinner() {
    let view = SelectedWorkspaceView {
        visible: true,
        workspace_id: Some(7),
        ..SelectedWorkspaceView::default()
    };
    assert_eq!(super::title_label(&view), "empty");
    assert!(!super::git_loading(&view));
}

#[test]
fn workspace_uses_workspace_icon_without_project() {
    let plain = SelectedWorkspaceView {
        visible: true,
        workspace_id: Some(7),
        name: Some("noble-owl".to_owned()),
        project: None,
    };
    let project = wrap(project_view(split_project(), None));

    assert_eq!(
        super::icon(&plain).glyph(),
        crate::widgets::nerd_icon::NerdIcon::workspace().glyph()
    );
    assert_eq!(
        super::icon(&project).glyph(),
        crate::widgets::nerd_icon::NerdIcon::folder().glyph()
    );
}

#[test]
fn selected_project_spins_while_git_resolves() {
    let loading = wrap(project_view(split_project(), None));

    assert!(super::visible(&loading));
    assert!(git_loading(&loading));
}

#[test]
fn selected_project_stops_spinning_once_git_resolves() {
    let resolved = wrap(project_view(split_project(), Some(GitStatus::default())));
    let empty = wrap(project_view(ProjectDetails::default(), None));

    assert!(!git_loading(&resolved));
    assert!(!git_loading(&empty));
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
    let view = project_view(
        ProjectDetails {
            has_project: true,
            cwd_label: Some("rsynapse".to_owned()),
            branch: Some("vafu/rsynapse/disk-widget".to_owned()),
            ..ProjectDetails::default()
        },
        None,
    );

    assert_eq!(view.branch.as_deref(), Some("disk-widget"));
}

#[test]
fn selected_project_keeps_vafu_branch_when_worktree_does_not_match_cwd() {
    let view = project_view(
        ProjectDetails {
            has_project: true,
            cwd_label: Some("rsynapse".to_owned()),
            branch: Some("vafu/other/disk-widget".to_owned()),
            ..ProjectDetails::default()
        },
        None,
    );

    assert_eq!(view.branch.as_deref(), Some("vafu/other/disk-widget"));
}

#[test]
fn selected_project_keeps_non_vafu_branch_name() {
    let view = project_view(
        ProjectDetails {
            has_project: true,
            cwd_label: Some("rsynapse".to_owned()),
            branch: Some("main".to_owned()),
            ..ProjectDetails::default()
        },
        None,
    );

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

#[test]
fn git_markers_combine_dirty_ahead_behind_and_merge() {
    let view = wrap(project_view(
        split_project(),
        Some(GitStatus {
            staged: 1,
            unstaged: 0,
            untracked: 2,
            ahead: 2,
            behind: 1,
            merging: true,
            ..GitStatus::default()
        }),
    ));

    assert!(super::git_visible(&view));
    assert!(super::git_part_visible(&view, GitPart::Dirty));
    assert_eq!(
        super::git_part_icon(GitPart::Dirty),
        crate::widgets::nerd_icon::cod::COD_DIFF_MODIFIED
    );
    assert!(super::git_part_visible(&view, GitPart::Untracked));
    assert_eq!(
        super::git_part_icon(GitPart::Untracked),
        crate::widgets::nerd_icon::cod::COD_DIFF_ADDED
    );
    assert_eq!(super::git_part_icon(GitPart::Ahead), "⇡");
    assert!(super::git_part_visible(&view, GitPart::Merging));
    assert!(super::classes(&view).contains(&"git-dirty"));
    assert!(super::classes(&view).contains(&"git-merging"));
    let tooltip = super::tooltip(&view);
    assert!(tooltip.contains("1 staged, 0 unstaged, 2 untracked"));
    assert!(tooltip.contains("ahead 2, behind 1"));
}

#[test]
fn git_markers_show_unstaged_and_rebase_icons() {
    let view = wrap(project_view(
        split_project(),
        Some(GitStatus {
            unstaged: 3,
            rebasing: true,
            ..GitStatus::default()
        }),
    ));

    assert!(super::git_visible(&view));
    assert!(super::git_part_visible(&view, GitPart::Dirty));
    assert_eq!(
        super::git_part_icon(GitPart::Dirty),
        crate::widgets::nerd_icon::cod::COD_DIFF_MODIFIED
    );
    assert_eq!(
        super::git_part_icon(GitPart::Rebasing),
        crate::widgets::nerd_icon::cod::COD_SYNC
    );
    assert!(super::classes(&view).contains(&"git-merging"));
}

#[test]
fn git_segment_hides_for_clean_tree() {
    let view = wrap(project_view(split_project(), Some(GitStatus::default())));

    assert!(!super::git_visible(&view));
    assert!(!super::git_part_visible(&view, GitPart::Dirty));
    assert!(!super::classes(&view).contains(&"git-dirty"));
    let tooltip = super::tooltip(&view);
    assert!(!tooltip.contains("staged"));
}

#[test]
fn git_segment_hides_without_repository() {
    let view = wrap(project_view(split_project(), None));

    assert!(!git_visible(&view));
    assert!(!super::git_part_visible(&view, GitPart::Dirty));
}
