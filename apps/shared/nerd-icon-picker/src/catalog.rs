use std::{ffi::OsString, path::PathBuf};

use gtk::gio;
use serde::Deserialize;

/// A named Nerd Font glyph that can be displayed or selected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NerdIcon {
    name: String,
    glyph: String,
}

impl NerdIcon {
    /// Construct an icon supplied by a picker consumer, such as a suggestion.
    pub fn specific(name: impl Into<String>, glyph: impl Into<String>) -> Option<Self> {
        let name = non_empty(name.into())?;
        let glyph = non_empty(glyph.into())?;
        Some(Self { name, glyph })
    }

    /// Return the searchable Nerd Font name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Return the glyph text rendered by a Nerd Font.
    pub fn glyph(&self) -> &str {
        self.glyph.as_str()
    }
}

#[derive(Debug, Deserialize)]
struct PickIconResult {
    icon: String,
    glyph: String,
}

pub(crate) async fn search_icons(query: &str, limit: usize) -> Result<Vec<NerdIcon>, String> {
    if query.trim().is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    let arguments = pick_icon_arguments(query, limit);
    let argv = arguments
        .iter()
        .map(OsString::as_os_str)
        .collect::<Vec<_>>();
    let process = gio::Subprocess::newv(
        &argv,
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE,
    )
    .map_err(|error| format!("failed to start pick-icon: {error}"))?;
    let (stdout, _) = process
        .communicate_utf8_future(None)
        .await
        .map_err(|error| format!("failed to query pick-icon: {error}"))?;
    if process.exit_status() != 0 {
        return Err(format!(
            "pick-icon exited with status {}",
            process.exit_status()
        ));
    }

    Ok(parse_pick_icon_output(
        stdout.as_deref().unwrap_or_default().as_bytes(),
        limit,
    ))
}

fn pick_icon_arguments(query: &str, limit: usize) -> Vec<OsString> {
    vec![
        pick_icon_executable().into_os_string(),
        "--family".into(),
        "nerd".into(),
        "--string".into(),
        query.into(),
        "--top".into(),
        limit.to_string().into(),
        "--json".into(),
    ]
}

fn parse_pick_icon_output(output: &[u8], limit: usize) -> Vec<NerdIcon> {
    serde_json::from_slice::<Vec<PickIconResult>>(output)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|result| {
            let name = non_empty(result.icon)?;
            let glyph = non_empty(result.glyph)?;
            Some(NerdIcon { name, glyph })
        })
        .take(limit)
        .collect()
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

#[cfg(test)]
mod test;
