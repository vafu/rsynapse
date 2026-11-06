use std::{
    collections::HashMap,
    fmt,
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use shell_rx_macros::combine_latest;
use tokio::time::{MissedTickBehavior, interval};
use zbus::{Connection, Proxy, fdo::DBusProxy, zvariant::OwnedValue};

use crate::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor, SignalDescriptor},
    from_task, once,
    rx::Observable as _,
    shared_by_key, switch_map,
};

const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const PLAYER_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const ROOT_INTERFACE: &str = "org.mpris.MediaPlayer2";
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PlaybackStatus {
    #[default]
    Stopped,
    Playing,
    Paused,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MprisSnapshot {
    pub player: String,
    pub identity: String,
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: String,
    pub playback_status: PlaybackStatus,
    pub position_us: i64,
    pub length_us: i64,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    pub can_play_pause: bool,
}

impl MprisSnapshot {
    pub fn is_available(&self) -> bool {
        !self.player.is_empty()
    }

    pub fn progress(&self) -> f64 {
        if self.length_us <= 0 {
            return 0.0;
        }
        (self.position_us.max(0) as f64 / self.length_us as f64).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Metadata {
    track_id: String,
    title: String,
    artist: String,
    album: String,
    art_url: String,
    length_us: i64,
}

impl TryFrom<OwnedValue> for Metadata {
    type Error = zbus::zvariant::Error;

    fn try_from(value: OwnedValue) -> Result<Self, Self::Error> {
        let properties = HashMap::<String, OwnedValue>::try_from(value)?;
        Ok(parse_metadata(&properties))
    }
}

/// Emits the active MPRIS player and its live playback state.
///
/// Player membership follows `NameOwnerChanged`. Metadata, playback status,
/// rate and capabilities follow `PropertiesChanged`; seeking follows `Seeked`.
/// Position is interpolated locally from the last MPRIS position, elapsed time
/// and playback rate. No D-Bus property is polled on a timer.
pub fn player() -> Observable<MprisSnapshot> {
    shared_by_key("mpris-player", "active", || {
        switch_map(player_names(), |names| {
            switch_map(choose_player(names), |player| match player {
                Some(player) => player_snapshot(player),
                None => once(MprisSnapshot::default()),
            })
        })
        .distinct_until_changed()
        .box_it()
    })
}

fn player_names() -> Observable<Vec<String>> {
    from_task(|sender| async move {
        let result = async {
            let connection = Connection::session()
                .await
                .map_err(|error| format!("connect MPRIS session bus failed: {error}"))?;
            let bus = DBusProxy::new(&connection)
                .await
                .map_err(|error| format!("connect MPRIS bus proxy failed: {error}"))?;
            let mut changes = bus
                .receive_name_owner_changed()
                .await
                .map_err(|error| format!("watch MPRIS name changes failed: {error}"))?;
            let mut names = list_players(&bus).await?;
            if sender.send(Ok(names.clone())).await.is_err() {
                return Ok(());
            }
            while changes.next().await.is_some() {
                names = list_players(&bus).await?;
                if sender.send(Ok(names.clone())).await.is_err() {
                    return Ok(());
                }
            }
            Ok(())
        }
        .await;

        if let Err(error) = result {
            let _ = sender.send(Err(error)).await;
        }
    })
    .distinct_until_changed()
    .box_it()
}

fn choose_player(names: Vec<String>) -> Observable<Option<String>> {
    if names.is_empty() {
        return once(None);
    }

    from_task(move |sender| {
        let names = names.clone();
        async move {
            let result = async {
                let connection = Connection::session()
                    .await
                    .map_err(|error| format!("connect MPRIS session bus failed: {error}"))?;
                let mut fallback = None;
                for name in names {
                    if playback_status(&connection, &name).await == PlaybackStatus::Playing {
                        return Ok(Some(name));
                    }
                    if fallback.is_none() {
                        fallback = Some(name);
                    }
                }
                Ok(fallback)
            }
            .await;
            let _ = sender.send(result).await;
        }
    })
    .distinct_until_changed()
    .box_it()
}

fn player_snapshot(player: String) -> Observable<MprisSnapshot> {
    let identity = dbus::property_or(
        PropertyDescriptor::new(player_object(&player, ROOT_INTERFACE), "Identity"),
        player.clone(),
    );
    let playback_status =
        player_property::<String>(&player, "PlaybackStatus", "Stopped".to_owned())
            .map(|status| parse_playback_status(&status))
            .distinct_until_changed();
    let metadata = player_property::<Metadata>(&player, "Metadata", Metadata::default())
        .distinct_until_changed();
    let position = progress(player.clone());
    let can_go_next = player_property::<bool>(&player, "CanGoNext", false);
    let can_go_previous = player_property::<bool>(&player, "CanGoPrevious", false);
    let can_play = player_property::<bool>(&player, "CanPlay", false);
    let can_pause = player_property::<bool>(&player, "CanPause", false);
    let can_control = player_property::<bool>(&player, "CanControl", false);
    let player_for_view = player.clone();

    combine_latest!(
        identity,
        playback_status,
        metadata,
        position,
        can_go_next,
        can_go_previous,
        can_play,
        can_pause,
        can_control => move |(identity, playback_status, metadata, position_us, can_go_next, can_go_previous, can_play, can_pause, can_control)| {
            MprisSnapshot {
                player: player_for_view.clone(),
                identity,
                track_id: metadata.track_id,
                title: metadata.title,
                artist: metadata.artist,
                album: metadata.album,
                art_url: metadata.art_url,
                playback_status,
                position_us,
                length_us: metadata.length_us,
                can_go_next: can_control && can_go_next,
                can_go_previous: can_control && can_go_previous,
                can_play_pause: can_control && (can_play || can_pause),
            }
        }
    )
    .distinct_until_changed()
    .box_it()
}

fn progress(player: String) -> Observable<i64> {
    from_task(move |sender| {
        let player = player.clone();

        async move {
            let mut initial_position = player_property::<i64>(&player, "Position", 0).into_stream();
            let mut rate = player_property::<f64>(&player, "Rate", 1.0).into_stream();
            let mut status = player_property::<String>(&player, "PlaybackStatus", "Stopped".to_owned())
                .map(|status| parse_playback_status(&status))
                .distinct_until_changed()
                .into_stream();
            let mut metadata = player_property::<Metadata>(&player, "Metadata", Metadata::default())
                .into_stream();
            let mut seeked = dbus::signal::<i64>(SignalDescriptor::new(
                player_object(&player, PLAYER_INTERFACE),
                "Seeked",
            ))
            .into_stream();
            let mut anchor_position = 0i64;
            let mut anchor_at = Instant::now();
            let mut current_rate = 1.0f64;
            let mut current_status = PlaybackStatus::Stopped;
            let mut status_initialized = false;
            let mut current_track_id = None::<String>;
            let mut last = i64::MIN;
            let mut ticker = interval(PROGRESS_INTERVAL);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

            loop {
                tokio::select! {
                    item = initial_position.next() => {
                        if let Some(Ok(value)) = item {
                            anchor_position = value;
                            anchor_at = Instant::now();
                        }
                    }
                    item = rate.next() => {
                        if let Some(Ok(value)) = item {
                            let now = Instant::now();
                            anchor_position =
                                interpolate(anchor_position, anchor_at, current_rate, current_status, now);
                            anchor_at = now;
                            current_rate = value;
                        }
                    }
                    item = status.next() => {
                        if let Some(Ok(value)) = item {
                            if !status_initialized {
                                status_initialized = true;
                                current_status = value;
                            } else {
                                let now = Instant::now();
                                anchor_position = interpolate(
                                    anchor_position,
                                    anchor_at,
                                    current_rate,
                                    current_status,
                                    now,
                                );
                                anchor_at = now;
                                if value == PlaybackStatus::Playing
                                    && current_status == PlaybackStatus::Stopped
                                {
                                    anchor_position = 0;
                                }
                                current_status = value;
                            }
                        }
                    }
                    item = metadata.next() => {
                        if let Some(Ok(value)) = item {
                            let track_changed = current_track_id
                                .as_deref()
                                .is_some_and(|track_id| track_id != value.track_id);
                            current_track_id = Some(value.track_id);
                            if track_changed {
                                anchor_position = 0;
                                anchor_at = Instant::now();
                            }
                        }
                    }
                    item = seeked.next() => {
                        if let Some(Ok(value)) = item {
                            anchor_position = value;
                            anchor_at = Instant::now();
                        }
                    }
                    _ = ticker.tick() => {}
                }

                let position = interpolate(
                    anchor_position,
                    anchor_at,
                    current_rate,
                    current_status,
                    Instant::now(),
                );
                if position != last {
                    if sender.send(Ok(position)).await.is_err() {
                        return;
                    }
                    last = position;
                }
            }
        }
    })
    .distinct_until_changed()
    .box_it()
}

fn interpolate(
    anchor_position: i64,
    anchor_at: Instant,
    rate: f64,
    status: PlaybackStatus,
    now: Instant,
) -> i64 {
    if status != PlaybackStatus::Playing {
        return anchor_position;
    }
    let elapsed_us = now.saturating_duration_since(anchor_at).as_micros() as i64;
    anchor_position.saturating_add((elapsed_us as f64 * rate).round() as i64)
}

async fn list_players(bus: &DBusProxy<'_>) -> Result<Vec<String>, String> {
    let names = bus
        .list_names()
        .await
        .map_err(|error| format!("list MPRIS bus names failed: {error}"))?;
    let mut players = names
        .into_iter()
        .map(|name| name.to_string())
        .filter(|name| name.starts_with(MPRIS_PREFIX))
        .collect::<Vec<_>>();
    players.sort();
    Ok(players)
}

async fn playback_status(connection: &Connection, name: &str) -> PlaybackStatus {
    let Ok(proxy) = Proxy::new(connection, name.to_owned(), PLAYER_PATH, PLAYER_INTERFACE).await
    else {
        return PlaybackStatus::Stopped;
    };
    match proxy.get_property::<String>("PlaybackStatus").await {
        Ok(status) => parse_playback_status(&status),
        Err(_) => PlaybackStatus::Stopped,
    }
}

fn player_object(player: &str, interface: &str) -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::Session, player, PLAYER_PATH, interface)
        .expect("MPRIS player descriptors should be valid")
}

fn player_property<T>(player: &str, name: &str, default: T) -> Observable<T>
where
    T: TryFrom<OwnedValue> + Clone + PartialEq + Send + 'static,
    T::Error: fmt::Display,
{
    dbus::property_or(
        PropertyDescriptor::new(player_object(player, PLAYER_INTERFACE), name),
        default,
    )
}

fn parse_playback_status(status: &str) -> PlaybackStatus {
    match status {
        "Playing" => PlaybackStatus::Playing,
        "Paused" => PlaybackStatus::Paused,
        _ => PlaybackStatus::Stopped,
    }
}

fn parse_metadata(properties: &HashMap<String, OwnedValue>) -> Metadata {
    Metadata {
        track_id: object_path_value(properties.get("mpris:trackid")).unwrap_or_default(),
        title: string_value(properties.get("xesam:title")).unwrap_or_default(),
        artist: string_array(properties.get("xesam:artist")).unwrap_or_default(),
        album: string_value(properties.get("xesam:album")).unwrap_or_default(),
        art_url: string_value(properties.get("mpris:artUrl")).unwrap_or_default(),
        length_us: i64_value(properties.get("mpris:length")).unwrap_or_default(),
    }
}

fn object_path_value(value: Option<&OwnedValue>) -> Option<String> {
    let value = value?.try_clone().ok()?;
    zbus::zvariant::OwnedObjectPath::try_from(value)
        .ok()
        .map(|path| path.to_string())
}

fn string_value(value: Option<&OwnedValue>) -> Option<String> {
    let value = value?.try_clone().ok()?;
    String::try_from(value)
        .ok()
        .filter(|value| !value.is_empty())
}

fn string_array(value: Option<&OwnedValue>) -> Option<String> {
    let value = value?.try_clone().ok()?;
    let values = Vec::<String>::try_from(value).ok()?;
    let joined = values
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    (!joined.is_empty()).then_some(joined)
}

fn i64_value(value: Option<&OwnedValue>) -> Option<i64> {
    let value = value?;
    i64::try_from(value)
        .ok()
        .or_else(|| {
            u64::try_from(value)
                .ok()
                .and_then(|value| i64::try_from(value).ok())
        })
        .or_else(|| i32::try_from(value).ok().map(i64::from))
        .or_else(|| u32::try_from(value).ok().map(i64::from))
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use zbus::zvariant::{OwnedValue, Value};

    use super::{MPRIS_PREFIX, PlaybackStatus, interpolate, parse_metadata, parse_playback_status};

    fn value<T>(value: T) -> OwnedValue
    where
        Value<'static>: From<T>,
    {
        OwnedValue::try_from(Value::from(value)).unwrap()
    }

    #[test]
    fn parses_playback_status() {
        assert_eq!(parse_playback_status("Playing"), PlaybackStatus::Playing);
        assert_eq!(parse_playback_status("Paused"), PlaybackStatus::Paused);
        assert_eq!(parse_playback_status("Stopped"), PlaybackStatus::Stopped);
        assert_eq!(parse_playback_status("unknown"), PlaybackStatus::Stopped);
    }

    #[test]
    fn parses_mpris_metadata() {
        let mut properties = HashMap::new();
        properties.insert("xesam:title".to_owned(), value("Song"));
        properties.insert("xesam:artist".to_owned(), value(vec!["Artist", "Guest"]));
        properties.insert("xesam:album".to_owned(), value("Album"));
        properties.insert("mpris:artUrl".to_owned(), value("file:///tmp/art.png"));
        properties.insert(
            "mpris:trackid".to_owned(),
            OwnedValue::try_from(Value::from(
                zbus::zvariant::ObjectPath::try_from("/org/mpris/MediaPlayer2/Track/1").unwrap(),
            ))
            .unwrap(),
        );
        properties.insert("mpris:length".to_owned(), value(123_000_i64));

        let metadata = parse_metadata(&properties);

        assert_eq!(metadata.track_id, "/org/mpris/MediaPlayer2/Track/1");
        assert_eq!(metadata.title, "Song");
        assert_eq!(metadata.artist, "Artist, Guest");
        assert_eq!(metadata.album, "Album");
        assert_eq!(metadata.art_url, "file:///tmp/art.png");
        assert_eq!(metadata.length_us, 123_000);
    }

    #[test]
    fn interpolates_position_only_while_playing() {
        let anchor = std::time::Instant::now();
        assert_eq!(
            interpolate(10, anchor, 1.0, PlaybackStatus::Playing, anchor),
            10
        );
        assert_eq!(
            interpolate(
                10,
                anchor,
                1.0,
                PlaybackStatus::Playing,
                anchor + std::time::Duration::from_secs(1)
            ),
            1_000_010
        );
        assert_eq!(
            interpolate(
                10,
                anchor,
                0.5,
                PlaybackStatus::Playing,
                anchor + std::time::Duration::from_secs(1)
            ),
            500_010
        );
        assert_eq!(
            interpolate(
                10,
                anchor,
                1.0,
                PlaybackStatus::Paused,
                anchor + std::time::Duration::from_secs(5)
            ),
            10
        );
    }

    #[test]
    fn player_names_use_mpris_prefix() {
        assert!("org.mpris.MediaPlayer2.spotify".starts_with(MPRIS_PREFIX));
        assert!(!"org.mpris.MediaPlayer2".starts_with(MPRIS_PREFIX));
    }
}
