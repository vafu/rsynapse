use shell_core::source::{self, rx::Observable as _, Observable};

pub(super) use crate::widgets::bar::icon_resolver::{
    clear_workspace_icon_override, set_workspace_icon_override,
};
pub(in crate::widgets::bar::project_label) use crate::widgets::bar::icon_resolver::{
    IconCandidate as WorkspaceIconCandidate, IconChoice as WorkspaceIconChoice,
};
use crate::widgets::bar::{
    icon_resolver::{
        resolve_icon, workspace_icon_override_source, IconChoice, IconEvidence, IconEvidenceKind,
        IconPolicy, IconRequest, IconResolution,
    },
    niri::NiriWorkspace,
    project::{project_details, ProjectDetails},
    window_source::{window_snapshots, WindowSnapshot},
};

#[cfg(test)]
use crate::widgets::bar::icon_resolver::parse_pick_icon_output as parse_pick_icon_output_with_score;
#[cfg(test)]
use crate::widgets::bar::icon_resolver::{
    picker_cache_key_for_request, picker_input_for_request, picker_strings_for_request,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceIcon {
    pub(super) glyph: String,
    pub(super) empty: bool,
    pub(super) picker_input: String,
    pub(super) candidates: Vec<WorkspaceIconCandidate>,
    pub(super) overridden: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceIconContext {
    empty: bool,
    request: IconRequest,
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
    let empty = context.empty;
    resolve_icon(context.request)
        .map(move |resolution| workspace_icon_from_resolution(resolution, empty))
        .distinct_until_changed()
        .box_it()
}

fn workspace_icon_from_resolution(resolution: IconResolution, empty: bool) -> WorkspaceIcon {
    let selected = resolution.selected;
    WorkspaceIcon {
        glyph: selected.glyph,
        empty,
        picker_input: resolution.picker_input,
        candidates: resolution.candidates,
        overridden: resolution.overridden,
    }
}

fn workspace_icon_context(
    workspace_id: Option<u64>,
    mut windows: Vec<WindowSnapshot>,
    project: ProjectDetails,
    override_icon: Option<IconChoice>,
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
    override_icon: Option<IconChoice>,
) -> WorkspaceIconContext {
    let has_project = project.has_project;
    let project_evidence = project_icon_evidence(project);
    let has_project_evidence = !project_evidence.is_empty();
    let (evidence, policy) = if has_project_evidence {
        (project_evidence, IconPolicy::workspace_project())
    } else {
        (app_icon_evidence(app_ids), IconPolicy::workspace_apps())
    };
    let empty = !has_project && evidence.is_empty();
    let request = IconRequest::new(
        "workspace-icon",
        IconChoice::workspace_fallback(),
        policy,
        evidence,
    )
    .with_override(override_icon);

    WorkspaceIconContext { empty, request }
}

fn project_icon_evidence(project: ProjectDetails) -> Vec<IconEvidence> {
    let mut evidence = Vec::new();
    push_optional(&mut evidence, IconEvidenceKind::ProjectName, project.name);
    push_optional(
        &mut evidence,
        IconEvidenceKind::ProjectDisplayMain,
        project.display_main,
    );
    push_optional(
        &mut evidence,
        IconEvidenceKind::ProjectDisplaySecondary,
        project.display_secondary,
    );
    push_optional(
        &mut evidence,
        IconEvidenceKind::ProjectCwd,
        project.cwd_label,
    );
    push_optional(
        &mut evidence,
        IconEvidenceKind::ProjectBranch,
        project.branch,
    );
    evidence
}

fn app_icon_evidence(mut app_ids: Vec<String>) -> Vec<IconEvidence> {
    app_ids.sort();
    app_ids
        .into_iter()
        .filter_map(|app_id| IconEvidence::new(IconEvidenceKind::AppId, app_id))
        .collect()
}

fn push_optional(evidence: &mut Vec<IconEvidence>, kind: IconEvidenceKind, value: Option<String>) {
    if let Some(evidence_value) = value.and_then(|value| IconEvidence::new(kind, value)) {
        evidence.push(evidence_value);
    }
}

#[cfg(test)]
pub(super) fn fallback_glyph_for_context(context: &WorkspaceIconContext) -> &str {
    context
        .request
        .override_icon()
        .unwrap_or_else(|| context.request.fallback())
        .glyph
        .as_str()
}

#[cfg(test)]
pub(super) fn with_icon_override_for_test(
    mut context: WorkspaceIconContext,
    glyph: &str,
) -> WorkspaceIconContext {
    context.request = context
        .request
        .clone()
        .with_override(Some(IconChoice::of(glyph)));
    context
}

#[cfg(test)]
pub(super) fn picker_strings_for_context(context: &WorkspaceIconContext) -> Vec<String> {
    picker_strings_for_request(&context.request)
}

#[cfg(test)]
pub(super) fn picker_input_for_context(context: &WorkspaceIconContext) -> String {
    picker_input_for_request(&context.request)
}

#[cfg(test)]
pub(super) fn picker_cache_key_for_context(context: &WorkspaceIconContext) -> Option<String> {
    picker_cache_key_for_request(&context.request)
}

#[cfg(test)]
pub(super) fn parse_pick_icon_output(output: &[u8]) -> Vec<WorkspaceIconCandidate> {
    parse_pick_icon_output_with_score(output, 720)
}
