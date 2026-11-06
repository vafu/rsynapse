use std::path::Path;

use shell_core::{
    gtk::gio,
    source::mpris::{MprisSnapshot, PlaybackStatus},
};

pub(super) fn title_text(player: &MprisSnapshot) -> String {
    if !player.is_available() {
        return "No media player".to_owned();
    }
    if !player.title.is_empty() {
        return player.title.clone();
    }
    if !player.identity.is_empty() {
        return player.identity.clone();
    }
    "Unknown track".to_owned()
}

pub(super) fn artist_text(player: &MprisSnapshot) -> String {
    if player.artist.is_empty() {
        if player.is_available() {
            "Unknown artist".to_owned()
        } else {
            "Start a player to see media here".to_owned()
        }
    } else {
        player.artist.clone()
    }
}

pub(super) fn play_pause_icon(player: &MprisSnapshot) -> &'static str {
    if player.playback_status == PlaybackStatus::Playing {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    }
}

pub(super) fn art_file(player: &MprisSnapshot) -> Option<gio::File> {
    let url = player.art_url.trim();
    if url.is_empty() {
        return None;
    }
    if url.contains("://") {
        Some(gio::File::for_uri(url))
    } else {
        Some(gio::File::for_path(Path::new(url)))
    }
}

pub(super) fn format_duration(microseconds: i64) -> String {
    let seconds = microseconds.max(0) / 1_000_000;
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod test {
    use super::format_duration;

    #[test]
    fn formats_media_durations() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(65_000_000), "1:05");
        assert_eq!(format_duration(3_665_000_000), "1:01:05");
        assert_eq!(format_duration(-1), "0:00");
    }
}
