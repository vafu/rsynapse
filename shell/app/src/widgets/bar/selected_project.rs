use shell_core::source::{self, Observable, rx::Observable as _};
use shell_rx_macros::combine_latest;

use crate::widgets::nerd_icon::NerdIcon;

use super::{
    niri::{self, NiriWorkspace},
    project::{ProjectDetails, project_details},
};

mod git;
mod rename;
mod workspace;
pub(super) use rename::WorkspaceEditor;

use git::{GitStatus, git_status};
use workspace::workspace_display_name;

/// Workspace-level view: always carries the workspace display name when
/// known, plus project details only when the workspace has a project.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct SelectedWorkspaceView {
    pub(super) workspace_id: Option<u64>,
    pub(super) visible: bool,
    pub(super) name: Option<String>,
    pub(super) project: Option<ProjectView>,
}

/// Project-level view: title, branch, and git metadata for a linked project.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ProjectView {
    pub(super) visible: bool,
    pub(super) title: String,
    pub(super) branch: Option<String>,
    pub(super) branch_full: Option<String>,
    pub(super) git: Option<GitStatus>,
}

pub(super) fn selected_project_status(
    output_name: Option<String>,
) -> Observable<SelectedWorkspaceView> {
    source::switch_map(niri::current_workspace(output_name), |workspace| {
        let Some(selected) = workspace else {
            return source::once(SelectedWorkspaceView::default());
        };
        let workspace_id = selected.path_id();
        combine_latest!(
            workspace_display_name(selected.path_id()),
            workspace_project_status(selected)
            => move |(name, project)| {
                let project = project.visible.then_some(project);
                SelectedWorkspaceView {
                    workspace_id,
                    visible: true,
                    name,
                    project,
                }
            },
        )
        .distinct_until_changed()
        .box_it()
    })
    .distinct_until_changed()
    .box_it()
}

fn workspace_project_status(workspace: NiriWorkspace) -> Observable<ProjectView> {
    source::switch_map(project_details(workspace), |project| {
        match project.path.clone() {
            // Paint the locus-derived view immediately; git metadata
            // resolves off-thread and fills in when ready instead of
            // holding the title and branch hostage.
            Some(path) => {
                let immediate = project_view(project.clone(), None);
                git_status(path)
                    .map(move |git| project_view(project.clone(), Some(git)))
                    .start_with(vec![immediate])
                    .distinct_until_changed()
                    .box_it()
            }
            None => source::once(project_view(project, None)),
        }
    })
    .distinct_until_changed()
    .box_it()
}

fn project_view(project: ProjectDetails, git: Option<GitStatus>) -> ProjectView {
    if !project.has_project {
        return ProjectView::default();
    }

    let title = project
        .cwd_label
        .as_deref()
        .and_then(non_empty)
        .map(str::to_owned)
        .unwrap_or_default();
    let branch_full = optional_text(project.branch);
    let branch = branch_full
        .clone()
        .map(|branch| display_branch(branch, &title))
        .filter(|branch| distinct_from(branch, &title));
    let visible = non_empty(&title).is_some();

    ProjectView {
        visible,
        title,
        branch,
        branch_full,
        git,
    }
}

fn distinct_from(value: &str, other: &str) -> bool {
    value.trim() != other.trim()
}

fn optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

fn display_branch(branch: String, cwd: &str) -> String {
    branch
        .strip_prefix("vafu/")
        .and_then(|rest| rest.split_once('/'))
        .filter(|(worktree, _)| *worktree == cwd)
        .map(|(_, feature)| feature.to_owned())
        .unwrap_or(branch)
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

pub(super) fn visible(view: &SelectedWorkspaceView) -> bool {
    view.visible
}

pub(super) fn icon(view: &SelectedWorkspaceView) -> NerdIcon {
    if view.project.is_some() {
        NerdIcon::folder()
    } else {
        NerdIcon::workspace()
    }
}

pub(super) fn title_label(view: &SelectedWorkspaceView) -> &str {
    view.name.as_deref().and_then(non_empty).unwrap_or_else(|| {
        view.project
            .as_ref()
            .map(|project| project.title.as_str())
            .unwrap_or("empty")
    })
}

pub(super) fn branch_visible(view: &SelectedWorkspaceView) -> bool {
    view.project
        .as_ref()
        .and_then(|project| project.branch.as_deref())
        .and_then(non_empty)
        .is_some()
}

pub(super) fn branch_label(view: &SelectedWorkspaceView) -> &str {
    view.project
        .as_ref()
        .and_then(|project| project.branch.as_deref())
        .unwrap_or_default()
}

pub(super) fn branch_for_clipboard(branch: Option<&str>) -> Option<&str> {
    branch.and_then(non_empty)
}

pub(super) fn first_separator_visible(view: &SelectedWorkspaceView) -> bool {
    branch_visible(view)
}

pub(super) fn branch_icon() -> NerdIcon {
    NerdIcon::new(crate::widgets::nerd_icon::cod::COD_GITHUB)
}

pub(super) fn tooltip(view: &SelectedWorkspaceView) -> String {
    let mut lines = Vec::new();
    if let Some(name) = view.name.as_deref().and_then(non_empty) {
        lines.push(format!("workspace: {name}"));
    }
    if let Some(project) = view.project.as_ref() {
        if let Some(title) = non_empty(project.title.as_str()) {
            lines.push(format!("cwd: {title}"));
        }
        if let Some(branch) = project.branch.as_deref().and_then(non_empty) {
            lines.push(format!("branch: {branch}"));
        }
        if let Some(git) = project.git.as_ref() {
            if git.has_changes() || git.stashes > 0 {
                lines.push(git_summary(git));
            }
        }
    }
    lines.join("\n")
}

pub(super) fn classes(view: &SelectedWorkspaceView) -> Vec<&'static str> {
    let mut classes = vec!["bar-item", "selected-project"];
    if let Some(git) = view
        .project
        .as_ref()
        .and_then(|project| project.git.as_ref())
    {
        if git.staged > 0 || git.unstaged > 0 || git.untracked > 0 {
            classes.push("git-dirty");
        }
        if git.merging || git.rebasing {
            classes.push("git-merging");
        }
    }
    classes
}

#[allow(dead_code)]
pub(super) fn git_visible(view: &SelectedWorkspaceView) -> bool {
    view.project
        .as_ref()
        .and_then(|project| project.git.as_ref())
        .is_some_and(GitStatus::has_changes)
}

/// Shows a spinner while a project is known but its git metadata has not
/// resolved yet. Project-less workspaces never spin: their git stays `None`
/// by design, and the workspace name carries the widget.
pub(super) fn git_loading(view: &SelectedWorkspaceView) -> bool {
    view.project
        .as_ref()
        .is_some_and(|project| project.git.is_none())
}

/// One fixed slot of the git status cluster. Each slot renders a single
/// icon widget: nerd glyphs and bar text never share a label because the
/// icon font's vertical metrics differ.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GitPart {
    Dirty,
    Untracked,
    Ahead,
    Behind,
    Merging,
    Rebasing,
}

pub(super) fn git_part_visible(view: &SelectedWorkspaceView, part: GitPart) -> bool {
    view.project
        .as_ref()
        .and_then(|project| project.git.as_ref())
        .is_some_and(|git| match part {
            GitPart::Dirty => git.staged > 0 || git.unstaged > 0,
            GitPart::Untracked => git.untracked > 0,
            GitPart::Ahead => git.ahead > 0,
            GitPart::Behind => git.behind > 0,
            GitPart::Merging => git.merging,
            GitPart::Rebasing => git.rebasing,
        })
}

pub(super) fn git_part_icon(part: GitPart) -> &'static str {
    use crate::widgets::nerd_icon::cod;

    match part {
        GitPart::Dirty => cod::COD_DIFF_MODIFIED,
        GitPart::Untracked => cod::COD_DIFF_ADDED,
        GitPart::Ahead => "⇡",
        GitPart::Behind => "⇣",
        GitPart::Merging => cod::COD_GIT_MERGE,
        GitPart::Rebasing => cod::COD_SYNC,
    }
}

fn git_summary(git: &GitStatus) -> String {
    let mut lines = vec![format!(
        "git: {} staged, {} unstaged, {} untracked",
        git.staged, git.unstaged, git.untracked
    )];
    if git.ahead > 0 || git.behind > 0 {
        lines.push(format!(
            "upstream: ahead {}, behind {}",
            git.ahead, git.behind
        ));
    }
    if git.merging {
        lines.push("merging".to_owned());
    }
    if git.rebasing {
        lines.push("rebasing".to_owned());
    }
    if git.stashes > 0 {
        lines.push(format!("stashes: {}", git.stashes));
    }
    lines.join("\n")
}

#[cfg(test)]
mod test;
