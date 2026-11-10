//! Icon picker process invocation.
use std::{path::PathBuf, time::Duration};

use serde::Deserialize;
use tokio::{process::Command, time::timeout};

const PICK_ICON_TIMEOUT: Duration = Duration::from_millis(800);
const MAX_PICK_ICON_CANDIDATES: usize = 5;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IconPickerCandidate {
    pub glyph: String,
    pub score_millis: u16,
}

#[derive(Debug, Deserialize)]
struct PickIconCandidateJson {
    glyph: Option<String>,
    score: Option<f64>,
}

pub async fn pick(strings: Vec<String>, min_score_millis: u16) -> Vec<IconPickerCandidate> {
    let mut command = Command::new(pick_icon_executable());
    for string in strings {
        command.args(["--string", string.as_str()]);
    }
    let top = MAX_PICK_ICON_CANDIDATES.to_string();
    command.args(["--family", "nerd", "--top", top.as_str(), "--json"]);

    timeout(PICK_ICON_TIMEOUT, command.output())
        .await
        .ok()
        .and_then(Result::ok)
        .and_then(|output| output.status.success().then_some(output))
        .map(|output| parse(&output.stdout, min_score_millis))
        .unwrap_or_default()
}

pub fn parse(output: &[u8], min_score_millis: u16) -> Vec<IconPickerCandidate> {
    serde_json::from_slice::<Vec<PickIconCandidateJson>>(output)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|candidate| {
            let glyph = non_empty(candidate.glyph.unwrap_or_default())?;
            let score_millis = score_millis(candidate.score.unwrap_or_default());
            (score_millis >= min_score_millis).then_some(IconPickerCandidate {
                glyph,
                score_millis,
            })
        })
        .take(MAX_PICK_ICON_CANDIDATES)
        .collect()
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
