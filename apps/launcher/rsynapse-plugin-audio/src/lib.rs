use rsynapse_plugin::{
    Hint, LauncherActivity, Plugin, PluginGroup, PluginResponse, PluginView, QueryContext,
    ResultItem,
};

const GROUP_ID: &str = "audio";

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn name(&self) -> &'static str {
        "Audio"
    }

    fn groups(&self) -> Vec<PluginGroup> {
        vec![PluginGroup {
            id: GROUP_ID.to_owned(),
            title: "Audio Output".to_owned(),
            description: "Choose the default audio output".to_owned(),
            aliases: vec![
                "audio".to_owned(),
                "sound".to_owned(),
                "sink".to_owned(),
                "output".to_owned(),
            ],
            plugin: String::new(),
        }]
    }

    fn query(&self, context: &QueryContext<'_>) -> PluginResponse {
        if context.group != Some(GROUP_ID) {
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
            view: Some(PluginView {
                title: "Audio Output".to_owned(),
                subtitle,
                placeholder: "Search audio outputs".to_owned(),
                hints: vec![
                    Hint {
                        key: "↑↓".to_owned(),
                        label: "choose".to_owned(),
                    },
                    Hint {
                        key: "Enter".to_owned(),
                        label: "apply".to_owned(),
                    },
                    Hint {
                        key: "Esc".to_owned(),
                        label: "close".to_owned(),
                    },
                ],
            }),
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
