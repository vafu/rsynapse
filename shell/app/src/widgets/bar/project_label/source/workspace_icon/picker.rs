use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use serde::Deserialize;
use tokio::{process::Command, time::timeout};

use super::{WorkspaceIconContext, non_empty};

const PICK_ICON_TIMEOUT: Duration = Duration::from_millis(800);
const MAX_QUERY_STRINGS: usize = 12;
const MAX_PICK_ICON_CANDIDATES: usize = 5;
const MIN_PICK_ICON_INPUTS: usize = 2;
const MIN_PICK_ICON_SCORE: f64 = 0.72;

type PickIconCache = HashMap<String, Vec<WorkspaceIconCandidate>>;

static PICK_ICON_CACHE: OnceLock<Mutex<PickIconCache>> = OnceLock::new();

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::widgets::bar::project_label) struct WorkspaceIconCandidate {
    pub(in crate::widgets::bar::project_label) icon: String,
    pub(in crate::widgets::bar::project_label) glyph: Option<String>,
    pub(in crate::widgets::bar::project_label) score_millis: u16,
}

#[derive(Debug, Deserialize)]
struct PickIconCandidateJson {
    icon: String,
    glyph: Option<String>,
    score: Option<f64>,
}

pub(super) async fn pick_icon_candidates(
    context: &WorkspaceIconContext,
) -> Vec<WorkspaceIconCandidate> {
    let Some(cache_key) = picker_cache_key_for_context(context) else {
        return Vec::new();
    };
    if let Some(candidates) = cached_pick_icon(&cache_key) {
        return candidates;
    }

    let mut command = Command::new(pick_icon_executable());
    for string in picker_command_strings(context) {
        command.args(["--string", string.as_str()]);
    }
    let top = MAX_PICK_ICON_CANDIDATES.to_string();
    command.args(["--family", "nerd", "--top", top.as_str(), "--json"]);

    let candidates = timeout(PICK_ICON_TIMEOUT, command.output())
        .await
        .ok()
        .and_then(Result::ok)
        .and_then(|output| output.status.success().then_some(output))
        .map(|output| parse_pick_icon_output(&output.stdout))
        .unwrap_or_default();
    cache_pick_icon(cache_key, candidates.clone());
    candidates
}

fn picker_command_strings(context: &WorkspaceIconContext) -> Vec<String> {
    context
        .strings
        .iter()
        .take(MAX_QUERY_STRINGS)
        .cloned()
        .collect()
}

#[cfg(test)]
pub(in crate::widgets::bar::project_label::source) fn picker_strings_for_context(
    context: &WorkspaceIconContext,
) -> &[String] {
    &context.strings
}

pub(in crate::widgets::bar::project_label::source) fn picker_input_for_context(
    context: &WorkspaceIconContext,
) -> String {
    picker_command_strings(context).join("\n")
}

pub(in crate::widgets::bar::project_label::source) fn picker_cache_key_for_context(
    context: &WorkspaceIconContext,
) -> Option<String> {
    let strings = picker_command_strings(context);
    (strings.len() >= MIN_PICK_ICON_INPUTS).then(|| strings.join("\u{1f}"))
}

fn cached_pick_icon(key: &str) -> Option<Vec<WorkspaceIconCandidate>> {
    pick_icon_cache().lock().ok()?.get(key).cloned()
}

fn cache_pick_icon(key: String, candidates: Vec<WorkspaceIconCandidate>) {
    if let Ok(mut cache) = pick_icon_cache().lock() {
        cache.insert(key, candidates);
    }
}

fn pick_icon_cache() -> &'static Mutex<PickIconCache> {
    PICK_ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(in crate::widgets::bar::project_label::source) fn parse_pick_icon_output(
    output: &[u8],
) -> Vec<WorkspaceIconCandidate> {
    serde_json::from_slice::<Vec<PickIconCandidateJson>>(output)
        .unwrap_or_default()
        .into_iter()
        .filter_map(pick_icon_candidate)
        .take(MAX_PICK_ICON_CANDIDATES)
        .collect()
}

fn pick_icon_candidate(candidate: PickIconCandidateJson) -> Option<WorkspaceIconCandidate> {
    let icon = non_empty(candidate.icon)?;
    let score = candidate.score.unwrap_or_default();
    (score >= MIN_PICK_ICON_SCORE).then(|| WorkspaceIconCandidate {
        icon,
        glyph: candidate.glyph.and_then(non_empty),
        score_millis: score_millis(score),
    })
}

fn score_millis(score: f64) -> u16 {
    (score.clamp(0.0, 1.0) * 1000.0).round() as u16
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
