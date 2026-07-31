mod aliases;
mod model;
mod override_store;
mod picker;

#[cfg(test)]
mod test;

use std::collections::BTreeSet;

use shell_core::source::{self, Observable, rx::Observable as _};

#[cfg(test)]
pub(in crate::widgets::bar) use self::picker::{
    parse_pick_icon_output, picker_cache_key_for_request, picker_strings_for_request,
};
use self::{aliases::alias_candidates, picker::pick_icon_candidates};
pub(in crate::widgets::bar) use self::{
    model::{
        IconCandidate, IconCandidateSource, IconChoice, IconEvidence, IconEvidenceKind, IconPolicy,
        IconRequest, IconResolution,
    },
    override_store::{
        clear_workspace_icon_override, set_workspace_icon_override, workspace_icon_override_source,
    },
    picker::picker_input_for_request,
};

pub(in crate::widgets::bar) fn resolve_icon(request: IconRequest) -> Observable<IconResolution> {
    let key = request.key();
    source::shared_by_key("rsynapse.icon-resolution", key, move || {
        let request = request.clone();
        source::from_task(move |sender| {
            let request = request.clone();
            async move {
                let initial = resolution_for_request(&request, Vec::new());
                if sender.send(Ok(initial.clone())).await.is_err() {
                    return;
                }

                let picker_candidates = pick_icon_candidates(&request).await;
                let resolved = resolution_for_request(&request, picker_candidates);
                if resolved != initial {
                    let _ = sender.send(Ok(resolved)).await;
                }
            }
        })
        .distinct_until_changed()
        .box_it()
    })
}

fn resolution_for_request(
    request: &IconRequest,
    picker_candidates: Vec<IconCandidate>,
) -> IconResolution {
    let candidates = merge_candidates(alias_candidates(request.evidence()), picker_candidates);
    let selected = request
        .override_icon()
        .cloned()
        .or_else(|| candidates.first().map(IconChoice::from))
        .unwrap_or_else(|| request.fallback().clone());
    IconResolution {
        selected,
        candidates,
        picker_input: picker_input_for_request(request),
        overridden: request.override_icon().is_some(),
    }
}

fn merge_candidates(
    alias_candidates: Vec<IconCandidate>,
    picker_candidates: Vec<IconCandidate>,
) -> Vec<IconCandidate> {
    let mut seen = BTreeSet::new();
    let mut candidates = alias_candidates
        .into_iter()
        .chain(picker_candidates)
        .filter(|candidate| seen.insert(candidate.identity()))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .score_millis
            .cmp(&left.score_millis)
            .then_with(|| {
                candidate_source_rank(left.source).cmp(&candidate_source_rank(right.source))
            })
            .then_with(|| left.glyph.cmp(&right.glyph))
    });
    candidates.truncate(5);
    candidates
}

fn candidate_source_rank(source: IconCandidateSource) -> u8 {
    match source {
        IconCandidateSource::Alias => 0,
        IconCandidateSource::Picker => 1,
    }
}

#[cfg(test)]
pub(super) fn resolve_icon_for_test(
    request: &IconRequest,
    picker_candidates: Vec<IconCandidate>,
) -> IconResolution {
    resolution_for_request(request, picker_candidates)
}
