use rsynapse_plugin::{
    ActionHint, LauncherActivity, Plugin, PluginResponse, PluginScope, PluginSpec, QueryContext,
    QueryMode, ResultItem,
};

const PLUGIN_ID: &str = "audio";

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn spec(&self) -> PluginSpec {
        PluginSpec {
            id: PLUGIN_ID,
            name: "Audio Output",
            aliases: &["audio", "sound", "sink", "output"],
            scope: PluginScope::DirectOnly,
        }
    }

    fn query(&self, context: &QueryContext<'_>) -> PluginResponse {
        if context.mode != QueryMode::Direct {
            return PluginResponse::default();
        }

        let mut sinks = match data_sources::audio::sinks_blocking() {
            Ok(sinks) => sinks,
            Err(error) => {
                eprintln!("[audio-plugin] {error}");
                return PluginResponse::default();
            }
        };
        sinks.sort_by(|left, right| {
            right
                .is_default
                .cmp(&left.is_default)
                .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
                .then_with(|| left.name.cmp(&right.name))
        });

        let query = context.query.trim().to_lowercase();
        let words = query.split_whitespace().collect::<Vec<_>>();
        let results = sinks
            .iter()
            .filter(|sink| {
                let text = format!("{} {}", sink.title, sink.name).to_lowercase();
                words.iter().all(|word| text.contains(word))
            })
            .enumerate()
            .map(|(index, sink)| ResultItem {
                id: format!("audio:{}", sink.id),
                title: sink.title.clone(),
                description: sink.subtitle.clone(),
                icon: "audio-speakers-symbolic".to_owned(),
                data: sink.id.to_string(),
                score: 1_000.0 - index as f64,
                activity: LauncherActivity::None,
                current: sink.is_default,
                keep_open: false,
            })
            .collect();

        let subtitle = sinks
            .iter()
            .find(|sink| sink.is_default)
            .map(|sink| sink.title.clone())
            .unwrap_or_else(|| "No default output".to_owned());

        PluginResponse {
            summary: subtitle,
            actions: vec![ActionHint::Apply],
            results,
        }
    }

    fn execute(&self, item: &ResultItem) -> Option<Result<(), String>> {
        let id = item.id.strip_prefix("audio:")?;
        let id = id.parse::<u32>().ok()?;
        Some(data_sources::audio::set_default_sink_blocking(id))
    }
}

pub fn plugin() -> Box<dyn Plugin> {
    Box::new(AudioPlugin)
}
