use std::collections::BTreeSet;

use shell_core::source::{self, Observable, rx::Observable as _};

#[path = "workspace_icon/icon_choice.rs"]
mod icon_choice;
#[path = "workspace_icon/icon_override.rs"]
mod icon_override;
#[path = "workspace_icon/picker.rs"]
mod picker;

pub(in crate::widgets::bar::project_label) use self::icon_choice::WorkspaceIconChoice;
use self::icon_override::workspace_icon_override_source;
pub(super) use self::icon_override::{clear_workspace_icon_override, set_workspace_icon_override};
pub(in crate::widgets::bar::project_label) use self::picker::WorkspaceIconCandidate;
use self::picker::pick_icon_candidates;
#[cfg(test)]
pub(super) use self::picker::{
    parse_pick_icon_output, picker_cache_key_for_context, picker_input_for_context,
    picker_strings_for_context,
};
use crate::widgets::bar::{
    niri::NiriWorkspace,
    project::{ProjectDetails, project_details},
    window_source::{WindowSnapshot, window_snapshots},
};

const FALLBACK_ICON: &str = "workspaces";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceIcon {
    pub(super) icon: String,
    pub(super) glyph: Option<String>,
    pub(super) empty: bool,
    pub(super) picker_input: String,
    pub(super) candidates: Vec<WorkspaceIconCandidate>,
    pub(super) overridden: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceIconContext {
    override_icon: Option<WorkspaceIconChoice>,
    project_icon: Option<WorkspaceIconChoice>,
    empty: bool,
    strings: Vec<String>,
}

pub(super) fn workspace_icon_source(workspace: NiriWorkspace) -> Observable<WorkspaceIcon> {
    let workspace_id = workspace.id().map(Some);
    let project = project_details(workspace.clone());
    let override_icon = workspace_icon_override_source(workspace.clone());
    let contexts = workspace_id
        .combine_latest(window_snapshots(), |workspace_id, windows| {
            (workspace_id, windows)
        })
        .combine_latest(project, |(workspace_id, windows), project| {
            (workspace_id, windows, project)
        })
        .combine_latest(
            override_icon,
            |(workspace_id, windows, project), override_icon| {
                workspace_icon_context(workspace_id, windows, project, override_icon)
            },
        )
        .distinct_until_changed()
        .box_it();

    source::switch_map(contexts, workspace_icon_for_context)
        .distinct_until_changed()
        .box_it()
}

fn workspace_icon_for_context(context: WorkspaceIconContext) -> Observable<WorkspaceIcon> {
    let key = context.key();
    source::shared_by_key("rsynapse.workspace-icon", key, move || {
        let context = context.clone();
        source::from_task(move |sender| {
            let context = context.clone();
            async move {
                let fallback = fallback_icon_choice_for_context(&context);
                let picker_input = picker::picker_input_for_context(&context);
                let initial = WorkspaceIcon {
                    icon: fallback.icon.clone(),
                    glyph: fallback.glyph.clone(),
                    empty: context.empty,
                    picker_input: picker_input.clone(),
                    candidates: Vec::new(),
                    overridden: context.override_icon.is_some() && context.project_icon.is_none(),
                };
                if sender.send(Ok(initial.clone())).await.is_err() {
                    return;
                }
                if context.project_icon.is_some() {
                    return;
                }

                let candidates = pick_icon_candidates(&context).await;
                let icon = context
                    .override_icon
                    .clone()
                    .or_else(|| candidates.first().map(WorkspaceIconChoice::from));
                let Some(icon) = icon else {
                    return;
                };
                let resolved = WorkspaceIcon {
                    icon: icon.icon,
                    glyph: icon.glyph,
                    empty: context.empty,
                    picker_input,
                    candidates,
                    overridden: context.override_icon.is_some(),
                };
                if resolved != initial {
                    let _ = sender.send(Ok(resolved)).await;
                }
            }
        })
        .distinct_until_changed()
        .box_it()
    })
}

fn workspace_icon_context(
    workspace_id: Option<u64>,
    mut windows: Vec<WindowSnapshot>,
    project: ProjectDetails,
    override_icon: Option<WorkspaceIconChoice>,
) -> WorkspaceIconContext {
    windows.retain(|window| window.workspace_id == workspace_id);
    windows.sort_by(|left, right| {
        (left.column, left.row, left.id)
            .cmp(&(right.column, right.row, right.id))
            .then_with(|| left.window.path_key().cmp(right.window.path_key()))
    });
    let app_ids = windows
        .into_iter()
        .filter_map(|window| window.app_id)
        .collect::<Vec<_>>();
    workspace_icon_context_from_parts_with_override(project, app_ids, override_icon)
}

#[cfg(test)]
pub(super) fn workspace_icon_context_from_parts(
    project: ProjectDetails,
    app_ids: Vec<String>,
) -> WorkspaceIconContext {
    workspace_icon_context_from_parts_with_override(project, app_ids, None)
}

fn workspace_icon_context_from_parts_with_override(
    project: ProjectDetails,
    app_ids: Vec<String>,
    override_icon: Option<WorkspaceIconChoice>,
) -> WorkspaceIconContext {
    let project_icon = project
        .icon
        .clone()
        .and_then(|icon| WorkspaceIconChoice::new(icon, project.icon_glyph.clone()));
    let project_strings = normalize_strings(
        [
            project.display_main,
            project.display_secondary,
            project.cwd_label,
            project.branch,
        ]
        .into_iter()
        .flatten()
        .collect(),
    );
    let mut app_strings = normalize_strings(app_ids);
    app_strings.sort();
    let strings = if project_strings.is_empty() {
        app_strings
    } else {
        project_strings
    };

    WorkspaceIconContext {
        override_icon,
        project_icon,
        empty: !project.has_project && strings.is_empty(),
        strings,
    }
}

fn normalize_strings(strings: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    strings
        .into_iter()
        .filter_map(non_empty)
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

#[cfg(test)]
pub(super) fn fallback_icon_for_context(context: &WorkspaceIconContext) -> &str {
    context
        .project_icon
        .as_ref()
        .map(|icon| icon.icon.as_str())
        .or_else(|| {
            context
                .override_icon
                .as_ref()
                .map(|icon| icon.icon.as_str())
        })
        .unwrap_or(FALLBACK_ICON)
}

fn fallback_icon_choice_for_context(context: &WorkspaceIconContext) -> WorkspaceIconChoice {
    context
        .project_icon
        .clone()
        .or_else(|| context.override_icon.clone())
        .unwrap_or_else(|| WorkspaceIconChoice::material(FALLBACK_ICON))
}

#[cfg(test)]
pub(super) fn with_icon_override_for_test(
    mut context: WorkspaceIconContext,
    icon: &str,
) -> WorkspaceIconContext {
    context.override_icon = Some(WorkspaceIconChoice::material(icon));
    context
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

impl WorkspaceIconContext {
    fn key(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}",
            self.empty,
            self.override_icon
                .as_ref()
                .map(|icon| icon.icon.as_str())
                .unwrap_or_default(),
            self.override_icon
                .as_ref()
                .and_then(|icon| icon.glyph.as_deref())
                .unwrap_or_default(),
            self.project_icon
                .as_ref()
                .map(|icon| icon.icon.as_str())
                .unwrap_or_default(),
            self.project_icon
                .as_ref()
                .and_then(|icon| icon.glyph.as_deref())
                .unwrap_or_default(),
            self.strings.join("\u{1f}")
        )
    }
}

impl From<&WorkspaceIconCandidate> for WorkspaceIconChoice {
    fn from(candidate: &WorkspaceIconCandidate) -> Self {
        Self {
            icon: candidate.icon.clone(),
            glyph: candidate.glyph.clone(),
        }
    }
}
