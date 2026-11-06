use std::process::Command;

use rsynapse_plugin::{
    Hint, Plugin, PluginGroup, PluginResponse, PluginView, QueryContext, ResultItem,
};
use serde_json::Value;

const GROUP_ID: &str = "audio";

#[derive(Clone, Debug, Eq, PartialEq)]
struct Sink {
    id: u32,
    name: String,
    title: String,
    subtitle: String,
    is_default: bool,
}

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

        let objects = match dump_objects() {
            Ok(objects) => objects,
            Err(error) => {
                eprintln!("[audio-plugin] {error}");
                return PluginResponse::default();
            }
        };
        let (mut sinks, default_name) = snapshot(&objects);
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
                current: sink.is_default,
                keep_open: false,
            })
            .collect();

        let subtitle = default_name
            .as_deref()
            .and_then(|name| sinks.iter().find(|sink| sink.name == name))
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
        Some(
            Command::new("wpctl")
                .arg("set-default")
                .arg(id)
                .status()
                .map_err(|error| format!("wpctl set-default {id}: {error}"))
                .and_then(|status| {
                    status
                        .success()
                        .then_some(())
                        .ok_or_else(|| format!("wpctl set-default {id} exited with {status}"))
                }),
        )
    }
}

fn dump_objects() -> Result<Vec<Value>, String> {
    let output = Command::new("pw-dump")
        .output()
        .map_err(|error| format!("run pw-dump: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "pw-dump exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice::<Vec<Value>>(&output.stdout)
        .map_err(|error| format!("parse pw-dump output: {error}"))
}

fn snapshot(objects: &[Value]) -> (Vec<Sink>, Option<String>) {
    let default_name = default_audio_sink_name(objects);
    let sinks = objects
        .iter()
        .filter_map(|object| sink(object, default_name.as_deref()))
        .collect();
    (sinks, default_name)
}

fn default_audio_sink_name(objects: &[Value]) -> Option<String> {
    objects
        .iter()
        .filter(|object| {
            object.get("type").and_then(Value::as_str) == Some("PipeWire:Interface:Metadata")
        })
        .flat_map(|object| {
            object
                .get("metadata")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find(|entry| entry.get("key").and_then(Value::as_str) == Some("default.audio.sink"))
        .and_then(|entry| entry.pointer("/value/name").and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

fn sink(object: &Value, default_sink: Option<&str>) -> Option<Sink> {
    if object.get("type").and_then(Value::as_str) != Some("PipeWire:Interface:Node") {
        return None;
    }
    let props = object.pointer("/info/props")?;
    if props.get("media.class").and_then(Value::as_str) != Some("Audio/Sink") {
        return None;
    }
    let id = object.get("id").and_then(Value::as_u64)?.try_into().ok()?;
    let name = props.get("node.name").and_then(Value::as_str)?;
    let title = props
        .get("node.description")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(name);
    if title.is_empty() {
        return None;
    }
    let volume = sink_volume(object).unwrap_or(0.0);
    let muted = sink_muted(object);
    let is_default = default_sink == Some(name);
    Some(Sink {
        id,
        name: name.to_owned(),
        title: title.to_owned(),
        subtitle: route_subtitle(volume, muted, is_default),
        is_default,
    })
}

fn sink_volume(object: &Value) -> Option<f64> {
    let props = object
        .pointer("/info/params/Props")
        .and_then(Value::as_array)?
        .first()?;
    props
        .get("channelVolumes")
        .and_then(Value::as_array)
        .and_then(|volumes| channel_volumes_to_ui_volume(volumes))
        .or_else(|| props.get("volume").and_then(Value::as_f64))
}

fn channel_volumes_to_ui_volume(volumes: &[Value]) -> Option<f64> {
    if volumes.is_empty() {
        return None;
    }
    let mut sum = 0.0;
    let mut count = 0usize;
    for volume in volumes.iter().filter_map(Value::as_f64) {
        sum += volume.cbrt();
        count += 1;
    }
    (count > 0).then(|| sum / count as f64)
}

fn sink_muted(object: &Value) -> bool {
    object
        .pointer("/info/params/Props")
        .and_then(Value::as_array)
        .and_then(|props| props.first())
        .and_then(|props| props.get("mute"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn route_subtitle(volume: f64, muted: bool, is_default: bool) -> String {
    let volume = format!("{}%", format_percent(volume));
    match (is_default, muted) {
        (true, true) => format!("Default output · {volume} · muted"),
        (true, false) => format!("Default output · {volume}"),
        (false, true) => format!("{volume} · muted"),
        (false, false) => volume,
    }
}

fn format_percent(value: f64) -> u8 {
    (value * 100.0).round().clamp(0.0, 150.0) as u8
}

pub fn plugin() -> Box<dyn Plugin> {
    Box::new(AudioPlugin)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn objects() -> Vec<Value> {
        serde_json::json!([
            {
                "id": 40,
                "type": "PipeWire:Interface:Metadata",
                "metadata": [
                    {
                        "key": "default.audio.sink",
                        "value": { "name": "alsa_output.default" }
                    }
                ]
            },
            {
                "id": 44,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "props": {
                        "media.class": "Audio/Sink",
                        "node.description": "Speakers",
                        "node.name": "alsa_output.default"
                    },
                    "params": { "Props": [{ "volume": 0.5, "mute": true }] }
                }
            },
            {
                "id": 45,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "props": {
                        "media.class": "Audio/Sink",
                        "node.description": "Headphones",
                        "node.name": "alsa_output.usb"
                    },
                    "params": { "Props": [{ "volume": 0.25, "mute": false }] }
                }
            }
        ])
        .as_array()
        .unwrap()
        .clone()
    }

    #[test]
    fn parses_default_and_other_sinks() {
        let (sinks, default) = snapshot(&objects());
        assert_eq!(default.as_deref(), Some("alsa_output.default"));
        assert_eq!(sinks.len(), 2);
        let speakers = sinks.iter().find(|sink| sink.id == 44).unwrap();
        assert!(speakers.is_default);
        assert_eq!(speakers.title, "Speakers");
        assert_eq!(speakers.subtitle, "Default output · 50% · muted");
        let headphones = sinks.iter().find(|sink| sink.id == 45).unwrap();
        assert!(!headphones.is_default);
        assert_eq!(headphones.subtitle, "25%");
    }
}
