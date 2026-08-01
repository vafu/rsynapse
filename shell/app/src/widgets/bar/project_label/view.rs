use shell_core::gtk::{self, prelude::*};

use super::{
    input::ProjectLabelInput,
    source::{ProjectLabelVm, WorkspaceIconCandidate},
};
use crate::widgets::{bar::WorkspaceNode, nerd_icon::NerdIcon};

const ICON_CANDIDATE_COUNT: usize = 5;

pub(super) fn connect_icon_candidate_button(
    button: &gtk::Button,
    input_sender: relm4::Sender<ProjectLabelInput>,
    index: usize,
) {
    let button = button.clone();
    let button_for_signal = button.clone();
    button.connect_clicked(move |_| {
        close_button_popover(&button_for_signal);
        input_sender.emit(ProjectLabelInput::SetIconOverride(index));
    });
}

pub(super) fn close_button_popover(button: &gtk::Button) {
    if let Some(popover) = button
        .ancestor(gtk::Popover::static_type())
        .and_then(|widget| widget.downcast::<gtk::Popover>().ok())
    {
        popover.popdown();
    }
}

pub(super) fn project_group_classes(vm: &ProjectLabelVm, selected: bool) -> Vec<&'static str> {
    let mut classes = vec!["bar-indicator"];

    if selected {
        classes.push("selected-workspace");
    }
    if vm.active {
        classes.push("current-workspace");
    } else if selected {
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

pub(super) fn auto_icon_visible(model: &ProjectLabelVm) -> bool {
    model.project_icon_overridden
}

pub(super) fn auto_icon_button_classes(model: &ProjectLabelVm) -> Vec<&'static str> {
    let mut classes = vec!["flat", "workspace-icon-choice"];
    if !model.project_icon_overridden {
        classes.push("selected");
    }
    classes
}

pub(super) fn icon_candidate_visible(model: &ProjectLabelVm, index: usize) -> bool {
    index < ICON_CANDIDATE_COUNT && model.project_icon_candidates.get(index).is_some()
}

pub(super) fn icon_candidate_button_classes(
    model: &ProjectLabelVm,
    index: usize,
) -> Vec<&'static str> {
    let mut classes = vec!["flat", "workspace-icon-choice"];
    if icon_candidate(model, index)
        .is_some_and(|candidate| candidate.glyph == model.project_icon_glyph)
    {
        classes.push("selected");
    }
    classes
}

pub(super) fn icon_candidate_render(model: &ProjectLabelVm, index: usize) -> NerdIcon {
    icon_candidate(model, index)
        .map(|candidate| NerdIcon::new(candidate.glyph.clone()))
        .unwrap_or_else(NerdIcon::workspace)
}

pub(super) fn icon_candidate(
    model: &ProjectLabelVm,
    index: usize,
) -> Option<&WorkspaceIconCandidate> {
    model.project_icon_candidates.get(index)
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
