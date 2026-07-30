use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use serde::Deserialize;
use tokio::{process::Command, time::timeout};

use super::{IconCandidate, IconCandidateSource, IconChoice, IconRequest};

const PICK_ICON_TIMEOUT: Duration = Duration::from_millis(800);
const MAX_QUERY_STRINGS: usize = 12;
const MAX_PICK_ICON_CANDIDATES: usize = 5;

type PickIconCache = HashMap<String, Vec<IconCandidate>>;

static PICK_ICON_CACHE: OnceLock<Mutex<PickIconCache>> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct PickIconCandidateJson {
    icon: String,
    glyph: Option<String>,
    score: Option<f64>,
}

pub(super) async fn pick_icon_candidates(request: &IconRequest) -> Vec<IconCandidate> {
    let Some(cache_key) = request.picker_cache_key() else {
        return Vec::new();
    };
    if let Some(candidates) = cached_pick_icon(&cache_key) {
        return candidates;
    }

    let mut command = Command::new(pick_icon_executable());
    for string in picker_command_strings(request) {
        command.args(["--string", string.as_str()]);
    }
    let top = MAX_PICK_ICON_CANDIDATES.to_string();
    command.args(["--family", "nerd", "--top", top.as_str(), "--json"]);

    let candidates = timeout(PICK_ICON_TIMEOUT, command.output())
        .await
        .ok()
        .and_then(Result::ok)
        .and_then(|output| output.status.success().then_some(output))
        .map(|output| parse_pick_icon_output(&output.stdout, request.min_picker_score_millis()))
        .unwrap_or_default();
    cache_pick_icon(cache_key, candidates.clone());
    candidates
}

#[cfg(test)]
pub(in crate::widgets::bar) fn picker_strings_for_request(request: &IconRequest) -> Vec<String> {
    picker_command_strings(request)
}

pub(in crate::widgets::bar) fn picker_input_for_request(request: &IconRequest) -> String {
    request.picker_input()
}

#[cfg(test)]
pub(in crate::widgets::bar) fn picker_cache_key_for_request(
    request: &IconRequest,
) -> Option<String> {
    request.picker_cache_key()
}

fn picker_command_strings(request: &IconRequest) -> Vec<String> {
    request
        .picker_strings()
        .into_iter()
        .take(MAX_QUERY_STRINGS)
        .collect()
}

fn cached_pick_icon(key: &str) -> Option<Vec<IconCandidate>> {
    pick_icon_cache().lock().ok()?.get(key).cloned()
}

fn cache_pick_icon(key: String, candidates: Vec<IconCandidate>) {
    if let Ok(mut cache) = pick_icon_cache().lock() {
        cache.insert(key, candidates);
    }
}

fn pick_icon_cache() -> &'static Mutex<PickIconCache> {
    PICK_ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(in crate::widgets::bar) fn parse_pick_icon_output(
    output: &[u8],
    min_score_millis: u16,
) -> Vec<IconCandidate> {
    serde_json::from_slice::<Vec<PickIconCandidateJson>>(output)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|candidate| pick_icon_candidate(candidate, min_score_millis))
        .take(MAX_PICK_ICON_CANDIDATES)
        .collect()
}

fn pick_icon_candidate(
    candidate: PickIconCandidateJson,
    min_score_millis: u16,
) -> Option<IconCandidate> {
    let icon = non_empty(candidate.icon)?;
    let score_millis = score_millis(candidate.score.unwrap_or_default());
    if score_millis < min_score_millis {
        return None;
    }
    let choice = IconChoice::new(icon, candidate.glyph.and_then(non_empty))?;
    Some(IconCandidate::new(
        choice,
        score_millis,
        IconCandidateSource::Picker,
    ))
}

fn score_millis(score: f64) -> u16 {
    (score.clamp(0.0, 1.0) * 1000.0).round() as u16
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
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
