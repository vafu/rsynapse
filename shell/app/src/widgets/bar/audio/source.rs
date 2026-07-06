use serde_json::Value;
use shell_core::source::{self, Observable, rx::Observable as _};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
};

use super::{AudioRouteView, AudioView};

const AUDIO_SOURCE_KEY: &str = "default-output";

#[derive(Clone, Debug, Eq, PartialEq)]
struct AudioSnapshot {
    status: AudioView,
    routes: Vec<AudioRouteView>,
}

pub(in crate::widgets::bar) fn audio_status() -> Observable<AudioView> {
    audio_snapshot()
        .map(|snapshot| snapshot.status)
        .distinct_until_changed()
        .box_it()
}

pub(in crate::widgets::bar) fn audio_routes() -> Observable<Vec<AudioRouteView>> {
    audio_snapshot()
        .map(|snapshot| snapshot.routes)
        .distinct_until_changed()
        .box_it()
}

pub(super) fn set_default_route(route_id: u32) {
    std::thread::spawn(move || {
        let _ = std::process::Command::new("wpctl")
            .arg("set-default")
            .arg(route_id.to_string())
            .status();
    });
}

fn audio_snapshot() -> Observable<AudioSnapshot> {
    source::shared_by_key("rsynapse.pipewire-audio", AUDIO_SOURCE_KEY, || {
        source::from_task(|sender| async move {
            let mut latest = None;
            let mut monitor = match spawn_pipewire_monitor() {
                Ok(monitor) => monitor,
                Err(error) => {
                    let _ = sender.send(Err(error)).await;
                    return;
                }
            };
            let Some(stdout) = monitor.stdout.take() else {
                let _ = sender
                    .send(Err("pw-dump monitor did not provide stdout".to_owned()))
                    .await;
                return;
            };

            let mut reader = BufReader::new(stdout).lines();
            let mut objects = Vec::new();
            let mut chunk = String::new();
            let mut in_chunk = false;
            let mut chunk_depth = 0i32;
            let mut initial_dump_seen = false;
            while let Ok(Some(line)) = reader.next_line().await {
                let trimmed = line.trim();
                if !in_chunk && trimmed == "[" {
                    in_chunk = true;
                    chunk_depth = 0;
                    chunk.clear();
                }

                if in_chunk {
                    chunk.push_str(&line);
                    chunk.push('\n');
                    chunk_depth += json_bracket_delta(&line);
                    if chunk_depth == 0 {
                        match parse_pipewire_chunk(&chunk) {
                            Ok(updates) if !initial_dump_seen => {
                                objects = updates;
                                emit_snapshot_if_changed(&sender, &mut latest, &objects).await;
                                initial_dump_seen = true;
                            }
                            Ok(updates) => {
                                apply_pipewire_updates(&mut objects, updates);
                                emit_snapshot_if_changed(&sender, &mut latest, &objects).await;
                            }
                            Err(error) => {
                                eprintln!(
                                    "[audio-source] failed to parse PipeWire monitor chunk: {error}"
                                );
                            }
                        }
                        in_chunk = false;
                    }
                }
            }
        })
        .distinct_until_changed()
        .box_it()
    })
}

async fn emit_snapshot_if_changed(
    sender: &async_channel::Sender<Result<AudioSnapshot, String>>,
    latest: &mut Option<AudioSnapshot>,
    objects: &[Value],
) {
    let snapshot = snapshot_from_pipewire_objects(objects);
    if latest.as_ref() != Some(&snapshot) {
        *latest = Some(snapshot.clone());
        let _ = sender.send(Ok(snapshot)).await;
    }
}

fn spawn_pipewire_monitor() -> Result<Child, String> {
    Command::new("pw-dump")
        .arg("-m")
        .arg("-N")
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("start pw-dump monitor failed: {error}"))
}

fn parse_pipewire_chunk(output: &str) -> Result<Vec<Value>, String> {
    serde_json::from_str::<Vec<Value>>(output)
        .map_err(|error| format!("parse pw-dump output failed: {error}"))
}

fn snapshot_from_pipewire_objects(objects: &[Value]) -> AudioSnapshot {
    let default_sink = default_audio_sink_name(objects);
    let mut routes = objects
        .iter()
        .filter_map(|object| pipewire_sink_route(object, default_sink.as_deref()))
        .collect::<Vec<_>>();
    if !routes.iter().any(|route| route.is_default)
        && let Some(first) = routes.first_mut()
    {
        first.is_default = true;
        first.subtitle = format!("Default output · {}", first.subtitle);
    }

    let default_route = routes
        .iter()
        .find(|route| route.is_default)
        .or_else(|| routes.first());
    let percent = default_route
        .and_then(|route| route.subtitle_percent())
        .unwrap_or(0);
    let muted = default_route.is_some_and(|route| route.subtitle.contains("muted"));
    let status = AudioView {
        visible: !routes.is_empty(),
        icon: audio_icon(percent, muted).to_owned(),
        tooltip: audio_tooltip(default_route, percent, muted),
        percent,
        muted,
    };
    AudioSnapshot { status, routes }
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

fn pipewire_sink_route(object: &Value, default_sink: Option<&str>) -> Option<AudioRouteView> {
    if !object_is_audio_sink(object) {
        return None;
    }

    let props = object.pointer("/info/props")?;
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

    Some(AudioRouteView {
        id,
        name: name.to_owned(),
        title: title.to_owned(),
        subtitle: route_subtitle(volume, muted, is_default),
        icon: "speaker".to_owned(),
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

fn apply_pipewire_updates(objects: &mut Vec<Value>, updates: Vec<Value>) {
    for update in updates {
        let Some(id) = object_id(&update) else {
            continue;
        };
        if update.get("info").is_some_and(Value::is_null) {
            objects.retain(|object| object_id(object) != Some(id));
            continue;
        }

        match objects
            .iter()
            .position(|object| object_id(object) == Some(id))
        {
            Some(index) => merge_json_value(&mut objects[index], update),
            None => objects.push(update),
        }
    }
}

fn merge_json_value(current: &mut Value, update: Value) {
    match (current, update) {
        (Value::Object(current), Value::Object(update)) => {
            for (key, update_value) in update {
                match current.get_mut(&key) {
                    Some(current_value) => merge_json_value(current_value, update_value),
                    None => {
                        current.insert(key, update_value);
                    }
                }
            }
        }
        (Value::Array(current), Value::Array(update)) if should_merge_array(current, &update) => {
            for (index, update_value) in update.into_iter().enumerate() {
                match current.get_mut(index) {
                    Some(current_value) => merge_json_value(current_value, update_value),
                    None => current.push(update_value),
                }
            }
        }
        (current, update) => *current = update,
    }
}

fn should_merge_array(current: &[Value], update: &[Value]) -> bool {
    !current.is_empty()
        && update.len() <= current.len()
        && update.iter().all(|value| matches!(value, Value::Object(_)))
        && current
            .iter()
            .take(update.len())
            .all(|value| matches!(value, Value::Object(_)))
}

fn object_is_audio_sink(object: &Value) -> bool {
    object.get("type").and_then(Value::as_str) == Some("PipeWire:Interface:Node")
        && object
            .pointer("/info/props/media.class")
            .and_then(Value::as_str)
            == Some("Audio/Sink")
}

fn object_id(object: &Value) -> Option<u64> {
    object.get("id").and_then(Value::as_u64)
}

fn json_bracket_delta(line: &str) -> i32 {
    let mut delta = 0;
    let mut in_string = false;
    let mut escaped = false;
    for character in line.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '[' | '{' if !in_string => delta += 1,
            ']' | '}' if !in_string => delta -= 1,
            _ => {}
        }
    }
    delta
}

fn route_subtitle(volume: f64, muted: bool, is_default: bool) -> String {
    let volume = percent_label(volume);
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

fn percent_label(value: f64) -> String {
    format!("{}%", format_percent(value))
}

fn audio_icon(percent: u8, muted: bool) -> &'static str {
    if muted || percent == 0 {
        "audio-volume-muted-symbolic"
    } else if percent < 35 {
        "audio-volume-low-symbolic"
    } else if percent < 70 {
        "audio-volume-medium-symbolic"
    } else {
        "audio-volume-high-symbolic"
    }
}

fn audio_tooltip(default_route: Option<&AudioRouteView>, percent: u8, muted: bool) -> String {
    let state = if muted { "muted" } else { "active" };
    match default_route {
        Some(route) => format!("{}: {percent}% ({state})", route.title),
        None => format!("Audio Output: {percent}% ({state})"),
    }
}

trait AudioRoutePercent {
    fn subtitle_percent(&self) -> Option<u8>;
}

impl AudioRoutePercent for AudioRouteView {
    fn subtitle_percent(&self) -> Option<u8> {
        self.subtitle
            .split(|character: char| !character.is_ascii_digit())
            .find(|part| !part.is_empty())
            .and_then(|part| part.parse().ok())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_pipewire_updates, audio_icon, default_audio_sink_name, format_percent,
        json_bracket_delta, pipewire_sink_route,
    };

    #[test]
    fn parses_pipewire_default_sink_and_route() {
        let objects = serde_json::json!([
            {
                "id": 40,
                "type": "PipeWire:Interface:Metadata",
                "metadata": [
                    {
                        "subject": 0,
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
                    "params": {
                        "Props": [
                            { "volume": 0.5, "mute": true }
                        ]
                    }
                }
            }
        ]);
        let objects = objects.as_array().unwrap();
        let default = default_audio_sink_name(objects);
        let route = pipewire_sink_route(&objects[1], default.as_deref()).unwrap();

        assert_eq!(default.as_deref(), Some("alsa_output.default"));
        assert_eq!(route.id, 44);
        assert!(route.is_default);
        assert_eq!(route.title, "Speakers");
        assert_eq!(route.subtitle, "Default output · 50% · muted");
    }

    #[test]
    fn chooses_volume_icons() {
        assert_eq!(audio_icon(0, false), "audio-volume-muted-symbolic");
        assert_eq!(audio_icon(20, false), "audio-volume-low-symbolic");
        assert_eq!(audio_icon(50, false), "audio-volume-medium-symbolic");
        assert_eq!(audio_icon(90, false), "audio-volume-high-symbolic");
        assert_eq!(audio_icon(90, true), "audio-volume-muted-symbolic");
    }

    #[test]
    fn percent_is_clamped_to_reasonable_bar_range() {
        assert_eq!(format_percent(1.0), 100);
        assert_eq!(format_percent(2.0), 150);
    }

    #[test]
    fn route_volume_prefers_channel_volumes() {
        let objects = serde_json::json!([
            {
                "id": 44,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "props": {
                        "media.class": "Audio/Sink",
                        "node.description": "Speakers",
                        "node.name": "alsa_output.default"
                    },
                    "params": {
                        "Props": [
                            {
                                "volume": 1.0,
                                "channelVolumes": [0.064, 0.064],
                                "mute": false
                            }
                        ]
                    }
                }
            }
        ]);
        let objects = objects.as_array().unwrap();
        let route = pipewire_sink_route(&objects[0], Some("alsa_output.default")).unwrap();

        assert_eq!(route.subtitle, "Default output · 40%");
    }

    #[test]
    fn partial_pipewire_update_preserves_sink_fields() {
        let mut objects = serde_json::json!([
            {
                "id": 44,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "props": {
                        "media.class": "Audio/Sink",
                        "node.description": "Speakers",
                        "node.name": "alsa_output.default"
                    },
                    "params": {
                        "Props": [
                            { "volume": 0.5, "mute": true }
                        ]
                    }
                }
            }
        ])
        .as_array()
        .unwrap()
        .to_owned();

        apply_pipewire_updates(
            &mut objects,
            vec![serde_json::json!({
                "id": 44,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "params": {
                        "Props": [
                            { "volume": 0.7 }
                        ]
                    }
                }
            })],
        );

        let route = pipewire_sink_route(&objects[0], Some("alsa_output.default")).unwrap();
        assert_eq!(route.title, "Speakers");
        assert_eq!(route.subtitle, "Default output · 70% · muted");
    }

    #[test]
    fn bracket_delta_ignores_strings() {
        assert_eq!(
            json_bracket_delta(r#""audio.position": "[ AUX0, AUX1 ]""#),
            0
        );
        assert_eq!(json_bracket_delta("["), 1);
        assert_eq!(json_bracket_delta("]"), -1);
    }
}
