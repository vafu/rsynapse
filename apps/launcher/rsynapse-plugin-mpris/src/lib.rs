//! Launcher plugin for choosing which MPRIS player the shell controls.
use data_sources::mpris;
use rsynapse_plugin::{
    ActionHint, LauncherActivity, Plugin, PluginResponse, PluginScope, PluginSpec, QueryContext,
    QueryMode, ResultItem,
};

const PLUGIN_ID: &str = "mpris";
/// Result id that clears the explicit selection and returns to auto-choice.
const AUTOMATIC_ID: &str = "mpris:automatic";

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
                let identity = mpris::player_identity_blocking(&player);
                matches(&format!("{identity} {player}")).then_some((player, identity))
            })
            .enumerate()
            .map(|(index, (player, identity))| {
                let current = selected.as_deref() == Some(player.as_str());
                ResultItem {
                    id: format!("mpris:{player}"),
                    title: identity,
                    description: player.clone(),
                    icon: "media-playback-start-symbolic".to_owned(),
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
