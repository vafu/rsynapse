use super::source::ProjectLabelVm;
use crate::widgets::{bar::WorkspaceNode, nerd_icon::NerdIcon};

pub(super) fn project_group_classes(vm: &ProjectLabelVm, selected: bool) -> Vec<&'static str> {
    let mut classes = vec!["bar-indicator"];

    if selected || vm.active {
        classes.push("selected-workspace");
    }
    if selected && !vm.active {
        classes.push("inactive-selected-workspace");
    }
    if vm.urgent || vm.agent.has_attention {
        classes.push("has-attention");
    }
    if vm.agent.has_working {
        classes.push("has-working");
    }
    if workspace_agent_unseen_visible(vm) {
        classes.push("has-unseen");
    }
    classes
}

pub(super) fn workspace_visible(vm: &ProjectLabelVm, selected: bool) -> bool {
    selected || !vm.empty
}

pub(super) fn project_icon_render(model: &ProjectLabelVm) -> NerdIcon {
    NerdIcon::new(model.project_icon_glyph.clone())
}

// Build status icon rendering is paused while the status is moved to a new surface.
// pub(super) fn workspace_build_indicator_state(state: WorkspaceBuildState) -> BuildIndicatorState {
//     match state {
//         WorkspaceBuildState::None => BuildIndicatorState::None,
//         WorkspaceBuildState::Running => BuildIndicatorState::Running,
//         WorkspaceBuildState::Failed => BuildIndicatorState::Failed,
//         WorkspaceBuildState::Finished => BuildIndicatorState::Finished,
//     }
// }

pub(super) fn project_primary(model: &ProjectLabelVm, _workspace: &WorkspaceNode) -> String {
    model
        .project_name
        .as_deref()
        .and_then(non_empty_text)
        .map(str::to_owned)
        .unwrap_or_else(|| workspace_title(&model.workspace_name, model.index))
}

pub(super) fn project_secondary(model: &ProjectLabelVm) -> Option<String> {
    model
        .project_branch
        .as_deref()
        .and_then(non_empty_text)
        .map(str::to_owned)
}

pub(super) fn project_tooltip(model: &ProjectLabelVm, workspace: &WorkspaceNode) -> String {
    let primary = project_primary(model, workspace);
    let title = match project_secondary(model) {
        Some(secondary) => format!("{primary} · {secondary}"),
        None => primary,
    };
    let override_line = model
        .project_icon_overridden
        .then_some("\noverride: locus")
        .unwrap_or_default();
    format!("{title}{override_line}")
}

pub(super) fn workspace_agent_unseen_visible(model: &ProjectLabelVm) -> bool {
    model.agent.has_unseen
}

pub(super) fn workspace_title(workspace_name: &str, index: u32) -> String {
    optional_text(Some(workspace_name))
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Workspace {}", index))
}

pub(super) fn optional_text(value: Option<&str>) -> Option<&str> {
    non_empty_text(value?)
}

pub(super) fn non_empty_text(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

pub(super) fn workspace_badge_label(sort_index: u32) -> String {
    sort_index.to_string()
}
