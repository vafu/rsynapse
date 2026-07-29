use shell_core::source::{self, Observable, rx::Observable as _};
use shell_rx_macros::combine_latest;

use super::{
    niri::{self, NiriWorkspace},
    project::{ProjectDetails, project_details},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct SelectedProjectView {
    pub(super) visible: bool,
    pub(super) title: String,
    pub(super) label: Option<String>,
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
    combine_latest!(
        workspace.index().map(u32::from),
        workspace.name().map(|name| name.unwrap_or_default()),
        project_details(workspace.clone())
            => |(index, workspace_name, project)| {
                selected_project_view(index, &workspace_name, project)
            },
    )
    .distinct_until_changed()
    .box_it()
}

fn selected_project_view(
    index: u32,
    workspace_name: &str,
    project: ProjectDetails,
) -> SelectedProjectView {
    let title = project
        .display_main
        .as_deref()
        .and_then(non_empty)
        .map(str::to_owned)
        .unwrap_or_else(|| workspace_title(workspace_name, index));
    let label = project
        .display_secondary
        .and_then(|value| distinct_from(&value, &title).then_some(value));
    let visible = label.is_some();

    SelectedProjectView {
        visible,
        title,
        label,
        branch: project.branch,
    }
}

fn workspace_title(workspace_name: &str, index: u32) -> String {
    non_empty(workspace_name)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Workspace {index}"))
}

fn distinct_from(value: &str, other: &str) -> bool {
    value.trim() != other.trim()
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

pub(super) fn visible(view: &SelectedProjectView) -> bool {
    view.visible
}

pub(super) fn icon_name() -> &'static str {
    "account_tree"
}

pub(super) fn label(view: &SelectedProjectView) -> &str {
    view.label.as_deref().unwrap_or_default()
}

pub(super) fn tooltip(view: &SelectedProjectView) -> String {
    let mut lines = vec![view.title.clone()];
    if let Some(label) = view.label.as_deref().and_then(non_empty) {
        lines.push(format!("secondary: {label}"));
    }
    if let Some(branch) = view.branch.as_deref().and_then(non_empty) {
        lines.push(format!("branch: {branch}"));
    }
    lines.join("\n")
}

pub(super) fn classes(_view: &SelectedProjectView) -> &'static [&'static str] {
    &["barblock", super::BACKGROUND_BLUR_CLASS, "selected-project"]
}

#[cfg(test)]
mod test;
