//! PipeWire default-output discovery and control.
//!
//! UI surfaces consume typed sinks or a compact status snapshot. The launcher
//! plugin API is synchronous, so blocking adapters are provided here instead of
//! letting plugins spawn `pw-dump`/`wpctl` themselves.
use std::process::Command;

use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Child,
};

use crate::{Observable, rx::Observable as _};

const AUDIO_SOURCE_KEY: &str = "default-output";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioSink {
    pub id: u32,
    pub name: String,
    pub title: String,
    pub subtitle: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioStatus {
    pub visible: bool,
    pub icon: String,
    pub tooltip: String,
    pub percent: u8,
    pub muted: bool,
}

impl Default for AudioStatus {
    fn default() -> Self {
        Self {
            visible: false,
            icon: "audio-volume-medium-symbolic".to_owned(),
            tooltip: String::new(),
            percent: 0,
            muted: false,
        }
    }
}

/// Live audio sinks, following PipeWire monitor updates.
pub fn sinks() -> Observable<Vec<AudioSink>> {
    crate::shared_by_key("rsynapse.pipewire-audio", AUDIO_SOURCE_KEY, || {
        crate::from_task(|sender| async move {
            let mut latest = None;
            let mut monitor = match spawn_monitor() {
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
            while let Some(line) = read_next_line(&mut reader, &sender).await {
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
                        match parse_chunk(&chunk) {
                            Ok(updates) if !initial_dump_seen => {
                                objects = updates;
                                emit_if_changed(&sender, &mut latest, &objects).await;
                                initial_dump_seen = true;
                            }
                            Ok(updates) => {
                                apply_updates(&mut objects, updates);
                                emit_if_changed(&sender, &mut latest, &objects).await;
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

/// Compact status for indicator surfaces.
pub fn status() -> Observable<AudioStatus> {
    sinks()
        .map(|sinks| status_from_sinks(&sinks))
        .distinct_until_changed()
        .box_it()
}

/// Blocking sink snapshot for launcher plugins.
pub fn sinks_blocking() -> Result<Vec<AudioSink>, String> {
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
    let objects = serde_json::from_slice::<Vec<Value>>(&output.stdout)
        .map_err(|error| format!("parse pw-dump output: {error}"))?;
    Ok(sinks_from_objects(&objects))
}

/// Blocking default-sink write for launcher plugins.
pub fn set_default_sink_blocking(id: u32) -> Result<(), String> {
    let status = Command::new("wpctl")
        .arg("set-default")
        .arg(id.to_string())
        .status()
        .map_err(|error| format!("wpctl set-default {id}: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("wpctl set-default {id} exited with {status}"))
}

fn status_from_sinks(sinks: &[AudioSink]) -> AudioStatus {
    let default_route = sinks
        .iter()
        .find(|sink| sink.is_default)
        .or_else(|| sinks.first());
    let percent = default_route
        .and_then(|sink| subtitle_percent(&sink.subtitle))
        .unwrap_or(0);
    let muted = default_route.is_some_and(|sink| sink.subtitle.contains("muted"));
    AudioStatus {
        visible: !sinks.is_empty(),
        icon: audio_icon(percent, muted).to_owned(),
        tooltip: audio_tooltip(default_route, percent, muted),
        percent,
        muted,
    }
}

async fn read_next_line(
    reader: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    sender: &async_channel::Sender<Result<Vec<AudioSink>, String>>,
) -> Option<String> {
    match reader.next_line().await {
        Ok(Some(line)) => Some(line),
        Ok(None) => {
            let _ = sender.send(Err("pw-dump monitor exited".to_owned())).await;
            None
        }
        Err(error) => {
            let _ = sender
                .send(Err(format!("pw-dump monitor read failed: {error}")))
                .await;
            None
        }
    }
}

async fn emit_if_changed(
    sender: &async_channel::Sender<Result<Vec<AudioSink>, String>>,
    latest: &mut Option<Vec<AudioSink>>,
    objects: &[Value],
) {
    let snapshot = sinks_from_objects(objects);
    if latest.as_ref() != Some(&snapshot) {
        *latest = Some(snapshot.clone());
        let _ = sender.send(Ok(snapshot)).await;
    }
}

fn spawn_monitor() -> Result<Child, String> {
    tokio::process::Command::new("pw-dump")
        .arg("-m")
        .arg("-N")
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("start pw-dump monitor failed: {error}"))
}

fn parse_chunk(output: &str) -> Result<Vec<Value>, String> {
    serde_json::from_str::<Vec<Value>>(output)
        .map_err(|error| format!("parse pw-dump output failed: {error}"))
}

fn sinks_from_objects(objects: &[Value]) -> Vec<AudioSink> {
    let default_sink = default_audio_sink_name(objects);
    let mut sinks = objects
        .iter()
        .filter_map(|object| sink_from_object(object, default_sink.as_deref()))
        .collect::<Vec<_>>();
    if !sinks.iter().any(|sink| sink.is_default)
        && let Some(first) = sinks.first_mut()
    {
        first.is_default = true;
        first.subtitle = format!("Default output · {}", first.subtitle);
    }
    sinks
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

fn sink_from_object(object: &Value, default_sink: Option<&str>) -> Option<AudioSink> {
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
    Some(AudioSink {
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

fn apply_updates(objects: &mut Vec<Value>, updates: Vec<Value>) {
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

fn subtitle_percent(subtitle: &str) -> Option<u8> {
    subtitle
        .split(|character: char| !character.is_ascii_digit())
        .find(|part| !part.is_empty())
        .and_then(|part| part.parse().ok())
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

fn audio_tooltip(default_route: Option<&AudioSink>, percent: u8, muted: bool) -> String {
    let state = if muted { "muted" } else { "active" };
    match default_route {
        Some(route) => format!("{}: {percent}% ({state})", route.title),
        None => format!("Audio Output: {percent}% ({state})"),
    }
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
        let sinks = sinks_from_objects(&objects());
        assert_eq!(sinks.len(), 2);
        let speakers = sinks.iter().find(|sink| sink.id == 44).unwrap();
        assert!(speakers.is_default);
        assert_eq!(speakers.title, "Speakers");
        assert_eq!(speakers.subtitle, "Default output · 50% · muted");
        let headphones = sinks.iter().find(|sink| sink.id == 45).unwrap();
        assert!(!headphones.is_default);
        assert_eq!(headphones.subtitle, "25%");
    }

    #[test]
    fn status_tracks_default_sink_volume_and_mute() {
        let status = status_from_sinks(&sinks_from_objects(&objects()));
        assert!(status.visible);
        assert_eq!(status.percent, 50);
        assert!(status.muted);
        assert_eq!(status.icon, "audio-volume-muted-symbolic");
    }
}
