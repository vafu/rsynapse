use shell_core::source::{self, Observable, rx::Observable as _};
use shell_rx_macros::combine_latest;

use crate::widgets::material_icon;

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
    pub(super) icon: Option<String>,
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
    let secondary = project
        .display_secondary
        .and_then(|value| distinct_from(&value, &title).then_some(value));
    let label = Some(match secondary.as_deref() {
        Some(secondary) => format!("{title} · {secondary}"),
        None => title.clone(),
    });
    let visible = non_empty(&title).is_some();

    SelectedProjectView {
        visible,
        title,
        label,
        branch: project.branch,
        icon: optional_text(project.icon),
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

pub(super) fn icon_name(view: &SelectedProjectView) -> String {
    material_icon::icon_name(
        view.icon
            .as_deref()
            .and_then(non_empty)
            .unwrap_or("account_tree"),
    )
}

pub(super) fn label(view: &SelectedProjectView) -> &str {
    view.label.as_deref().unwrap_or_default()
}

pub(super) fn tooltip(view: &SelectedProjectView) -> String {
    let mut lines = vec![view.title.clone()];
    if let Some(label) = view
        .label
        .as_deref()
        .and_then(non_empty)
        .filter(|label| *label != view.title.as_str())
    {
        lines.push(format!("display: {label}"));
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
