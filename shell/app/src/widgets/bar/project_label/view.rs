use shell_core::gtk::{self, prelude::*};

use super::{
    input::ProjectLabelInput,
    source::{ProjectLabelVm, WorkspaceIconCandidate},
};
use crate::widgets::{bar::WorkspaceNode, material_icon};

use super::super::PANEL_ICON_SIZE;

const ICON_CANDIDATE_COUNT: usize = 5;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BarIconRender {
    icon: String,
    glyph: Option<String>,
}

pub(super) trait BarIconBoxExt {
    fn set_bar_icon(&self, icon: BarIconRender);
}

impl BarIconBoxExt for gtk::Box {
    fn set_bar_icon(&self, icon: BarIconRender) {
        let key = format!(
            "{}:{}",
            icon.icon,
            icon.glyph.as_deref().unwrap_or_default()
        );
        if self.widget_name().as_str() == key {
            return;
        }
        self.set_widget_name(&key);
        while let Some(child) = self.first_child() {
            self.remove(&child);
        }

        if let Some(glyph) = icon.glyph.and_then(non_empty_owned) {
            let label = gtk::Label::new(Some(&glyph));
            label.set_css_classes(&["bar-indicator-icon", "nerdicon"]);
            label.set_halign(gtk::Align::Center);
            label.set_valign(gtk::Align::Center);
            label.set_width_chars(1);
            self.append(&label);
            return;
        }

        let image = gtk::Image::new();
        image.set_css_classes(&["bar-indicator-icon", "materialicon"]);
        image.set_pixel_size(PANEL_ICON_SIZE);
        image.set_halign(gtk::Align::Center);
        image.set_valign(gtk::Align::Center);
        image.set_icon_name(Some(material_icon::icon_name(&icon.icon).as_str()));
        self.append(&image);
    }
}

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

pub(super) fn project_icon(model: &ProjectLabelVm) -> String {
    non_empty_text(&model.project_icon)
        .unwrap_or("workspaces")
        .to_owned()
}

pub(super) fn project_icon_render(model: &ProjectLabelVm) -> BarIconRender {
    BarIconRender {
        icon: project_icon(model),
        glyph: model.project_icon_glyph.clone(),
    }
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
    let icon = icon_label(project_icon(model), model.project_icon_glyph.as_deref());
    let override_line = model
        .project_icon_overridden
        .then_some("\noverride: locus")
        .unwrap_or_default();
    let candidate_lines = icon_candidate_lines(model);
    match (
        non_empty_text(&model.project_icon_input),
        candidate_lines.is_empty(),
    ) {
        (Some(input), false) => {
            format!(
                "{title}\nicon: {icon}{override_line}\npick-icon input:\n{input}\ncandidates:\n{candidate_lines}"
            )
        }
        (Some(input), true) => {
            format!("{title}\nicon: {icon}{override_line}\npick-icon input:\n{input}")
        }
        (None, false) => {
            format!("{title}\nicon: {icon}{override_line}\ncandidates:\n{candidate_lines}")
        }
        (None, true) => format!("{title}\nicon: {icon}{override_line}"),
    }
}

pub(super) fn icon_candidate_lines(model: &ProjectLabelVm) -> String {
    model
        .project_icon_candidates
        .iter()
        .map(|candidate| {
            format!(
                "{} {}",
                icon_label(candidate.icon.clone(), candidate.glyph.as_deref()),
                score_label(candidate.score_millis)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
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

pub(super) fn auto_icon_tooltip(model: &ProjectLabelVm) -> String {
    format!(
        "automatic · current {}",
        icon_label(project_icon(model), model.project_icon_glyph.as_deref())
    )
}

pub(super) fn icon_candidate_visible(model: &ProjectLabelVm, index: usize) -> bool {
    index < ICON_CANDIDATE_COUNT && model.project_icon_candidates.get(index).is_some()
}

pub(super) fn icon_candidate_button_classes(
    model: &ProjectLabelVm,
    index: usize,
) -> Vec<&'static str> {
    let mut classes = vec!["flat", "workspace-icon-choice"];
    if icon_candidate(model, index).is_some_and(|candidate| {
        candidate.icon == project_icon(model) && candidate.glyph == model.project_icon_glyph
    }) {
        classes.push("selected");
    }
    classes
}

pub(super) fn icon_candidate_tooltip(model: &ProjectLabelVm, index: usize) -> String {
    icon_candidate(model, index)
        .map(|candidate| {
            format!(
                "{} · {}",
                icon_label(candidate.icon.clone(), candidate.glyph.as_deref()),
                score_label(candidate.score_millis)
            )
        })
        .unwrap_or_default()
}

pub(super) fn icon_candidate_render(model: &ProjectLabelVm, index: usize) -> BarIconRender {
    icon_candidate(model, index)
        .map(|candidate| BarIconRender {
            icon: candidate.icon.clone(),
            glyph: candidate.glyph.clone(),
        })
        .unwrap_or_else(|| BarIconRender {
            icon: "workspaces".to_owned(),
            glyph: None,
        })
}

pub(super) fn icon_candidate(
    model: &ProjectLabelVm,
    index: usize,
) -> Option<&WorkspaceIconCandidate> {
    model.project_icon_candidates.get(index)
}

pub(super) fn score_label(score_millis: u16) -> String {
    format!("{:.3}", f32::from(score_millis) / 1000.0)
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

fn non_empty_owned(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn icon_label(icon: String, glyph: Option<&str>) -> String {
    match glyph.and_then(non_empty_text) {
        Some(glyph) => format!("{icon} {glyph}"),
        None => icon,
    }
}

pub(super) fn workspace_badge_label(sort_index: u32) -> String {
    sort_index.to_string()
}
