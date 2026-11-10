use std::collections::BTreeSet;

use crate::widgets::nerd_icon::{dev, fa, md, seti};

use super::{IconCandidate, IconCandidateSource, IconChoice, IconEvidence, IconEvidenceKind};

const ALIAS_SCORE: u16 = 1000;

pub(super) fn alias_candidates(evidence: &[IconEvidence]) -> Vec<IconCandidate> {
    let mut seen = BTreeSet::new();
    evidence
        .iter()
        .filter_map(alias_for_evidence)
        .map(|choice| IconCandidate::new(choice, ALIAS_SCORE, IconCandidateSource::Alias))
        .filter(|candidate| seen.insert(candidate.identity().to_owned()))
        .collect()
}

fn alias_for_evidence(evidence: &IconEvidence) -> Option<IconChoice> {
    match evidence.kind {
        IconEvidenceKind::AppId | IconEvidenceKind::DesktopName | IconEvidenceKind::DesktopIcon => {
            app_alias(&evidence.value)
        }
        IconEvidenceKind::ProjectName
        | IconEvidenceKind::ProjectDisplayMain
        | IconEvidenceKind::ProjectDisplaySecondary
        | IconEvidenceKind::ProjectBranch
        | IconEvidenceKind::ProjectCwd => None,
    }
}

fn app_alias(value: &str) -> Option<IconChoice> {
    let value = normalize(value);
    let glyph = if value.contains("firefox") {
        fa::FA_FIREFOX
    } else if value.contains("chrome") || value.contains("chromium") {
        md::MD_GOOGLE_CHROME
    } else if value.contains("slack") {
        dev::DEV_SLACK
    } else if value.contains("neovim") || value.contains("nvim") {
        seti::CUSTOM_NEOVIM
    } else if value.contains("ghostty") || value.contains("terminal") || value.contains("term") {
        dev::DEV_TERMINAL
    } else {
        return None;
    };
    IconChoice::new(glyph.to_owned())
}

fn normalize(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(".desktop")
        .replace(['_', '-', '.'], " ")
        .to_ascii_lowercase()
}
