use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use super::{IconCandidate, IconCandidateSource, IconChoice, IconRequest};

const MAX_QUERY_STRINGS: usize = 12;

type PickIconCache = HashMap<String, Vec<IconCandidate>>;

static PICK_ICON_CACHE: OnceLock<Mutex<PickIconCache>> = OnceLock::new();

pub(super) async fn pick_icon_candidates(request: &IconRequest) -> Vec<IconCandidate> {
    let Some(cache_key) = request.picker_cache_key() else {
        return Vec::new();
    };
    if let Some(candidates) = cached_pick_icon(&cache_key) {
        return candidates;
    }

    let candidates = data_sources::icon_picker::pick(
        picker_command_strings(request),
        request.min_picker_score_millis(),
    )
    .await
    .into_iter()
    .filter_map(|candidate| {
        IconChoice::new(candidate.glyph).map(|choice| {
            IconCandidate::new(choice, candidate.score_millis, IconCandidateSource::Picker)
        })
    })
    .collect::<Vec<_>>();
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
    data_sources::icon_picker::parse(output, min_score_millis)
        .into_iter()
        .filter_map(|candidate| {
            IconChoice::new(candidate.glyph).map(|choice| {
                IconCandidate::new(choice, candidate.score_millis, IconCandidateSource::Picker)
            })
        })
        .collect()
}
