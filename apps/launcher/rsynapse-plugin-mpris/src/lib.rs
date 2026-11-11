//! Launcher plugin for choosing which MPRIS player the shell controls.
use data_sources::mpris::{self, MprisSnapshot, PlaybackStatus};
use rsynapse_plugin::{
    ActionHint, LauncherActivity, Plugin, PluginResponse, PluginScope, PluginSpec, QueryContext,
    QueryMode, ResultItem,
};

const PLUGIN_ID: &str = "mpris";
/// Result id that clears the explicit selection and returns to auto-choice.
const AUTOMATIC_ID: &str = "mpris:automatic";
const PLAYER_ICON: &str = "media-playback-start-symbolic";

fn row_icon(snapshot: &MprisSnapshot) -> String {
    let art = snapshot.art_url.trim();
    if art.starts_with("file://") || art.starts_with('/') {
        return art.to_owned();
    }
    match snapshot.playback_status {
        PlaybackStatus::Playing => PLAYER_ICON.to_owned(),
        PlaybackStatus::Paused => "media-playback-pause-symbolic".to_owned(),
        PlaybackStatus::Stopped => "media-playback-stop-symbolic".to_owned(),
    }
}

fn media_description(snapshot: &MprisSnapshot) -> String {
    let track = match (snapshot.title.trim(), snapshot.artist.trim()) {
        ("", "") => String::new(),
        (title, "") => title.to_owned(),
        ("", artist) => artist.to_owned(),
        (title, artist) => format!("{artist} — {title}"),
    };
    if track.is_empty() {
        return match snapshot.playback_status {
            PlaybackStatus::Playing => "Playing".to_owned(),
            PlaybackStatus::Paused => "Paused".to_owned(),
            PlaybackStatus::Stopped => "Idle".to_owned(),
        };
    }
    track
}

pub struct MprisPlugin;

impl Plugin for MprisPlugin {
    fn spec(&self) -> PluginSpec {
        PluginSpec {
            id: PLUGIN_ID,
            name: "Media Player",
            aliases: &["mpris", "media", "player", "music"],
            scope: PluginScope::DirectOnly,
        }
    }

    fn query(&self, context: &QueryContext<'_>) -> PluginResponse {
        if context.mode != QueryMode::Direct {
            return PluginResponse::default();
        }

        let players = match mpris::players_blocking() {
            Ok(players) => players,
            Err(error) => {
                eprintln!("[mpris-plugin] {error}");
                return PluginResponse::default();
            }
        };
        let selected = mpris::selected_player_blocking().ok().flatten();

        let query = context.query.trim().to_lowercase();
        let words = query.split_whitespace().collect::<Vec<_>>();
        let matches = |text: &str| words.iter().all(|word| text.to_lowercase().contains(word));

        let mut rows = Vec::new();
        // Offer an automatic choice only once an explicit selection exists, so
        // the default list stays focused on the live players.
        if selected.is_some() && matches("automatic") {
            rows.push(ResultItem {
                id: AUTOMATIC_ID.to_owned(),
                title: "Automatic".to_owned(),
                description: "Follow the playing player".to_owned(),
                icon: "media-playback-start-symbolic".to_owned(),
                data: String::new(),
                current: false,
                keep_open: false,
                score: f64::MAX,
                activity: LauncherActivity::None,
            });
        }

        let mut player_rows = players
            .into_iter()
            .filter_map(|player| {
                let snapshot = mpris::player_snapshot_blocking(&player).ok();
                let identity = snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.identity.clone())
                    .unwrap_or_else(|| mpris::player_identity_blocking(&player));
                let visible = snapshot
                    .as_ref()
                    .map(|snapshot| {
                        format!("{identity} {player} {} {}", snapshot.title, snapshot.artist)
                    })
                    .unwrap_or_else(|| format!("{identity} {player}"));
                matches(&visible).then_some((player, identity, snapshot))
            })
            .enumerate()
            .map(|(index, (player, identity, snapshot))| {
                let current = selected.as_deref() == Some(player.as_str());
                let description = snapshot
                    .as_ref()
                    .map(media_description)
                    .unwrap_or_else(|| "No media".to_owned());
                let icon = snapshot
                    .as_ref()
                    .map(row_icon)
                    .unwrap_or_else(|| PLAYER_ICON.to_owned());
                ResultItem {
                    id: format!("mpris:{player}"),
                    title: identity,
                    description,
                    icon,
                    current,
                    data: player,
                    score: 1_000.0 - index as f64,
                    keep_open: false,
                    activity: LauncherActivity::None,
                }
            })
            .collect::<Vec<_>>();
        rows.append(&mut player_rows);

        let summary = match selected.as_deref() {
            Some(player) => mpris::player_identity_blocking(player),
            None => "Automatic (playing player)".to_owned(),
        };

        PluginResponse {
            summary,
            actions: vec![ActionHint::Apply],
            results: rows,
        }
    }

    fn execute(&self, item: &ResultItem) -> Option<Result<(), String>> {
        if item.id == AUTOMATIC_ID {
            return Some(mpris::clear_selected_player_blocking());
        }
        let player = item.id.strip_prefix("mpris:")?;
        Some(mpris::set_selected_player_blocking(player))
    }
}

pub fn plugin() -> Box<dyn Plugin> {
    Box::new(MprisPlugin)
}

#[cfg(test)]
mod test {
    use super::{media_description, row_icon};
    use data_sources::mpris::{MprisSnapshot, PlaybackStatus};

    fn snapshot() -> MprisSnapshot {
        MprisSnapshot {
            player: "org.mpris.MediaPlayer2.spotify".to_owned(),
            identity: "Spotify".to_owned(),
            playback_status: PlaybackStatus::Playing,
            title: "Song".to_owned(),
            artist: "Artist".to_owned(),
            ..MprisSnapshot::default()
        }
    }

    #[test]
    fn describes_playing_track() {
        assert_eq!(media_description(&snapshot()), "Artist — Song");
    }

    #[test]
    fn describes_paused_and_idle_players() {
        let mut paused = snapshot();
        paused.playback_status = PlaybackStatus::Paused;
        assert_eq!(media_description(&paused), "Artist — Song");

        let mut idle = snapshot();
        idle.playback_status = PlaybackStatus::Stopped;
        idle.title.clear();
        idle.artist.clear();
        assert_eq!(media_description(&idle), "Idle");
    }

    #[test]
    fn uses_local_artwork_only() {
        let mut player = snapshot();
        player.art_url = "https://example.com/art.png".to_owned();
        assert_eq!(row_icon(&player), "media-playback-start-symbolic");

        player.art_url = "file:///tmp/art.png".to_owned();
        assert_eq!(row_icon(&player), "file:///tmp/art.png");
    }
}
