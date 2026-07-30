use std::collections::BTreeSet;

use super::{IconCandidate, IconCandidateSource, IconChoice, IconEvidence, IconEvidenceKind};

const ALIAS_SCORE: u16 = 1000;

pub(super) fn alias_candidates(evidence: &[IconEvidence]) -> Vec<IconCandidate> {
    let mut seen = BTreeSet::new();
    evidence
        .iter()
        .filter_map(alias_for_evidence)
        .map(|choice| IconCandidate::new(choice, ALIAS_SCORE, IconCandidateSource::Alias))
        .filter(|candidate| seen.insert(candidate.identity()))
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
    let choice = match value.as_str() {
        _ if value.contains("firefox") => {
            IconChoice::new("nf-fa-firefox".to_owned(), Some("".to_owned()))
        }
        _ if value.contains("chrome") || value.contains("chromium") => {
            IconChoice::new("nf-md-google_chrome".to_owned(), Some("󰊯".to_owned()))
        }
        _ if value.contains("slack") => {
            IconChoice::new("nf-dev-slack".to_owned(), Some("".to_owned()))
        }
        _ if value.contains("neovim") || value.contains("nvim") => {
            IconChoice::new("nf-custom-neovim".to_owned(), Some("".to_owned()))
        }
        _ if value.contains("ghostty") || value.contains("terminal") || value.contains("term") => {
            IconChoice::new("nf-dev-terminal".to_owned(), Some("".to_owned()))
        }
        _ => None,
    }?;
    Some(choice)
}

fn normalize(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(".desktop")
        .replace(['_', '-', '.'], " ")
        .to_ascii_lowercase()
}
