use crate::widgets::material_icon;
use shell_core::source::{self, Observable, rx::Observable as _};

use super::{
    niri::{self, NiriWorkspace},
    project::{ProjectDetails, project_details},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct SelectedProjectView {
    pub(super) visible: bool,
    pub(super) title: String,
    pub(super) branch: Option<String>,
}

pub(super) fn selected_project_status(
    output_name: Option<String>,
) -> Observable<SelectedProjectView> {
    source::switch_map(niri::current_workspace(output_name), |workspace| {
        workspace
            .map(selected_workspace_project_status)
            .unwrap_or_else(|| source::once(SelectedProjectView::default()))
    })
    .distinct_until_changed()
    .box_it()
}

fn selected_workspace_project_status(workspace: NiriWorkspace) -> Observable<SelectedProjectView> {
    project_details(workspace)
        .map(selected_project_view)
        .distinct_until_changed()
        .box_it()
}

fn selected_project_view(project: ProjectDetails) -> SelectedProjectView {
    if !project.has_project {
        return SelectedProjectView::default();
    }

    let title = project
        .cwd_label
        .as_deref()
        .and_then(non_empty)
        .map(str::to_owned)
        .unwrap_or_default();
    let branch = optional_text(project.branch).filter(|branch| distinct_from(branch, &title));
    let visible = non_empty(&title).is_some();

    SelectedProjectView {
        visible,
        title,
        branch,
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

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

pub(super) fn visible(view: &SelectedProjectView) -> bool {
    view.visible
}

pub(super) fn icon_name(_view: &SelectedProjectView) -> String {
    material_icon::icon_name("folder")
}

pub(super) fn title_label(view: &SelectedProjectView) -> &str {
    view.title.as_str()
}

pub(super) fn branch_visible(view: &SelectedProjectView) -> bool {
    view.branch.as_deref().and_then(non_empty).is_some()
}

pub(super) fn branch_label(view: &SelectedProjectView) -> &str {
    view.branch.as_deref().unwrap_or_default()
}

pub(super) fn first_separator_visible(view: &SelectedProjectView) -> bool {
    branch_visible(view)
}

pub(super) fn branch_icon_name() -> String {
    material_icon::icon_name("account_tree")
}

pub(super) fn tooltip(view: &SelectedProjectView) -> String {
    let mut lines = vec![format!("cwd: {}", view.title)];
    if let Some(branch) = view.branch.as_deref().and_then(non_empty) {
        lines.push(format!("branch: {branch}"));
    }
    lines.join("\n")
}

pub(super) fn classes(_view: &SelectedProjectView) -> &'static [&'static str] {
    &["bar-item", super::BACKGROUND_BLUR_CLASS, "selected-project"]
}

#[cfg(test)]
mod test;
