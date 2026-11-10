use std::path::PathBuf;

use crate::widgets::nerd_icon::md;
use data_sources::mpris::MprisSnapshot;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) enum SoneLikeState {
    #[default]
    Hidden,
    Checking,
    Liked,
    NotLiked,
    Unavailable,
}

impl SoneLikeState {
    pub(super) fn visible(&self) -> bool {
        !matches!(self, Self::Hidden)
    }

    pub(super) fn liked(&self) -> Option<bool> {
        match self {
            Self::Liked => Some(true),
            Self::NotLiked => Some(false),
            _ => None,
        }
    }

    pub(super) fn can_toggle(&self) -> bool {
        self.liked().is_some()
    }

    pub(super) fn icon(&self) -> &'static str {
        if self.liked() == Some(true) {
            md::MD_HEART
        } else {
            md::MD_HEART_OUTLINE
        }
    }
}

pub(super) fn is_sone(player: &MprisSnapshot) -> bool {
    player.player.to_ascii_lowercase().contains("sone")
        || player.identity.eq_ignore_ascii_case("sone")
}

pub(super) fn track_number(track_id: &str) -> Option<String> {
    let number = track_id.rsplit('/').next()?;
    (!number.is_empty() && number.chars().all(|character| character.is_ascii_digit()))
        .then(|| number.to_owned())
}

pub(super) async fn status(track_id: &str) -> Result<bool, String> {
    let output = run(&["status", track_id]).await?;
    match output.as_str() {
        "liked" => Ok(true),
        "not liked" => Ok(false),
        other => Err(format!("unexpected sone-cli status output: {other}")),
    }
}

pub(super) async fn set_liked(track_id: &str, liked: bool) -> Result<(), String> {
    let action = if liked { "like" } else { "unlike" };
    let output = run(&[action, track_id]).await?;
    if output.is_empty() {
        Err(format!("sone-cli {action} returned no result"))
    } else {
        Ok(())
    }
}

async fn run(args: &[&str]) -> Result<String, String> {
    let program = cli_path();
    let output = data_sources::write::output(&program, args)
        .await
        .map_err(|error| format!("run sone-cli {program:?} failed: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "sone-cli {args:?} exited with {}: {}",
            output.status,
            stderr.trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn cli_path() -> PathBuf {
    if let Some(path) = std::env::var_os("SONE_CLI").map(PathBuf::from)
        && path.is_file()
    {
        return path;
    }
    if let Some(path) = repository_cli_path() {
        return path;
    }
    if let Some(home) = std::env::var_os("HOME") {
        let path = PathBuf::from(home).join("proj/sone-cli/sone-cli");
        if path.is_file() {
            return path;
        }
    }
    PathBuf::from("sone-cli")
}

fn repository_cli_path() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    executable
        .ancestors()
        .map(|directory| directory.join("scripts/sone-cli"))
        .find(|path| path.is_file())
}

#[cfg(test)]
mod test {
    use super::{MprisSnapshot, is_sone, track_number};

    fn player(bus: &str, identity: &str, track_id: &str) -> MprisSnapshot {
        MprisSnapshot {
            player: bus.to_owned(),
            identity: identity.to_owned(),
            track_id: track_id.to_owned(),
            ..MprisSnapshot::default()
        }
    }

    #[test]
    fn detects_sone_by_bus_name_or_identity() {
        assert!(is_sone(&player(
            "org.mpris.MediaPlayer2.io.github.lullabyX.sone",
            "SONE",
            "",
        )));
        assert!(is_sone(&player(
            "org.mpris.MediaPlayer2.player",
            "sone",
            ""
        )));
        assert!(!is_sone(&player(
            "org.mpris.MediaPlayer2.spotify",
            "Spotify",
            "",
        )));
    }

    #[test]
    fn extracts_numeric_sone_track_numbers() {
        assert_eq!(
            track_number("/org/mpris/MediaPlayer2/Track/105951346").as_deref(),
            Some("105951346")
        );
        assert_eq!(track_number("/org/mpris/MediaPlayer2/Track/tidal"), None);
        assert_eq!(track_number(""), None);
    }
}
