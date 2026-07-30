use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use serde::Deserialize;
use shell_core::source::{self, Observable, rx::Observable as _};
use tokio::{process::Command, time::timeout};

use crate::widgets::bar::{
    niri::NiriWorkspace,
    project::{ProjectDetails, project_details},
    window_source::{WindowSnapshot, window_snapshots},
};

const FALLBACK_ICON: &str = "workspaces";
const PICK_ICON_TIMEOUT: Duration = Duration::from_millis(800);
const MAX_QUERY_STRINGS: usize = 12;
const MIN_PICK_ICON_INPUTS: usize = 2;
const MIN_PICK_ICON_SCORE: f64 = 0.72;

type PickIconCache = HashMap<String, Option<String>>;

static PICK_ICON_CACHE: OnceLock<Mutex<PickIconCache>> = OnceLock::new();

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceIcon {
    pub(super) icon: String,
    pub(super) empty: bool,
    pub(super) picker_input: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorkspaceIconContext {
    project_icon: Option<String>,
    empty: bool,
    strings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PickIconCandidate {
    icon: String,
    score: Option<f64>,
}

pub(super) fn workspace_icon_source(workspace: NiriWorkspace) -> Observable<WorkspaceIcon> {
    let workspace_id = workspace.id().map(Some);
    let contexts = workspace_id
        .combine_latest(window_snapshots(), |workspace_id, windows| {
            (workspace_id, windows)
        })
        .combine_latest(
            project_details(workspace),
            |(workspace_id, windows), project| {
                workspace_icon_context(workspace_id, windows, project)
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
                let fallback = fallback_icon_for_context(&context).to_owned();
                let picker_input = picker_input_for_context(&context);
                let initial = WorkspaceIcon {
                    icon: fallback.clone(),
                    empty: context.empty,
                    picker_input: picker_input.clone(),
                };
                if sender.send(Ok(initial.clone())).await.is_err() {
                    return;
                }
                if context.project_icon.is_some() || fallback != FALLBACK_ICON {
                    return;
                }

                let Some(icon) = pick_icon(&context).await else {
                    return;
                };
                let resolved = WorkspaceIcon {
                    icon,
                    empty: context.empty,
                    picker_input,
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
    workspace_icon_context_from_parts(project, app_ids)
}

pub(super) fn workspace_icon_context_from_parts(
    project: ProjectDetails,
    app_ids: Vec<String>,
) -> WorkspaceIconContext {
    let project_icon = project.icon.clone().and_then(non_empty);
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

pub(super) fn fallback_icon_for_context(context: &WorkspaceIconContext) -> &str {
    context.project_icon.as_deref().unwrap_or(FALLBACK_ICON)
}

#[cfg(test)]
pub(super) fn picker_strings_for_context(context: &WorkspaceIconContext) -> &[String] {
    &context.strings
}

pub(super) fn picker_input_for_context(context: &WorkspaceIconContext) -> String {
    picker_command_strings(context).join("\n")
}

async fn pick_icon(context: &WorkspaceIconContext) -> Option<String> {
    let cache_key = picker_cache_key_for_context(context)?;
    if let Some(icon) = cached_pick_icon(&cache_key) {
        return icon;
    }

    let mut command = Command::new(pick_icon_executable());
    for string in picker_command_strings(context) {
        command.args(["--string", string.as_str()]);
    }
    command.args(["--top", "1", "--json"]);

    let icon = timeout(PICK_ICON_TIMEOUT, command.output())
        .await
        .ok()
        .and_then(Result::ok)
        .and_then(|output| output.status.success().then_some(output))
        .and_then(|output| parse_pick_icon_output(&output.stdout));
    cache_pick_icon(cache_key, icon.clone());
    icon
}

fn picker_command_strings(context: &WorkspaceIconContext) -> Vec<String> {
    context
        .strings
        .iter()
        .take(MAX_QUERY_STRINGS)
        .cloned()
        .collect()
}

pub(super) fn picker_cache_key_for_context(context: &WorkspaceIconContext) -> Option<String> {
    let strings = picker_command_strings(context);
    (strings.len() >= MIN_PICK_ICON_INPUTS).then(|| strings.join("\u{1f}"))
}

fn cached_pick_icon(key: &str) -> Option<Option<String>> {
    pick_icon_cache().lock().ok()?.get(key).cloned()
}

fn cache_pick_icon(key: String, icon: Option<String>) {
    if let Ok(mut cache) = pick_icon_cache().lock() {
        cache.insert(key, icon);
    }
}

fn pick_icon_cache() -> &'static Mutex<PickIconCache> {
    PICK_ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn parse_pick_icon_output(output: &[u8]) -> Option<String> {
    serde_json::from_slice::<Vec<PickIconCandidate>>(output)
        .ok()?
        .into_iter()
        .find_map(pick_icon_candidate)
}

fn pick_icon_candidate(candidate: PickIconCandidate) -> Option<String> {
    (candidate.score.unwrap_or_default() >= MIN_PICK_ICON_SCORE)
        .then_some(candidate.icon)
        .and_then(non_empty)
}

fn pick_icon_executable() -> PathBuf {
    if let Some(path) = std::env::var_os("RSYNAPSE_PICK_ICON") {
        return PathBuf::from(path);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let path = PathBuf::from(home).join(".cargo/bin/pick-icon");
        if path.exists() {
            return path;
        }
    }
    PathBuf::from("pick-icon")
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

impl WorkspaceIconContext {
    fn key(&self) -> String {
        format!(
            "{}:{}:{}",
            self.empty,
            self.project_icon.as_deref().unwrap_or_default(),
            self.strings.join("\u{1f}")
        )
    }
}
