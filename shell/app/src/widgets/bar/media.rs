use shell_core::source::{
    Observable,
    mpris::{self, MprisSnapshot, PlaybackStatus},
    rx::Observable as _,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct MediaView {
    pub(super) visible: bool,
    pub(super) icon: &'static str,
    pub(super) tooltip: &'static str,
}

pub(super) fn media_status() -> Observable<MediaView> {
    mpris::player()
        .map(media_view)
        .distinct_until_changed()
        .box_it()
}

fn media_view(player: MprisSnapshot) -> MediaView {
    if !player.is_available() {
        return MediaView::default();
    }
    let (icon, tooltip) = match player.playback_status {
        PlaybackStatus::Playing => ("media-playback-start-symbolic", "Playing"),
        PlaybackStatus::Paused => ("media-playback-pause-symbolic", "Paused"),
        PlaybackStatus::Stopped => ("media-playback-stop-symbolic", "Stopped"),
    };
    MediaView {
        visible: true,
        icon,
        tooltip,
    }
}

#[cfg(test)]
mod test {
    use super::{PlaybackStatus, media_view};
    use shell_core::source::mpris::MprisSnapshot;

    fn player(status: PlaybackStatus) -> MprisSnapshot {
        MprisSnapshot {
            player: "org.mpris.MediaPlayer2.test".to_owned(),
            playback_status: status,
            ..MprisSnapshot::default()
        }
    }

    #[test]
    fn follows_mpris_playback_state() {
        assert!(!media_view(MprisSnapshot::default()).visible);
        assert_eq!(
            media_view(player(PlaybackStatus::Playing)).icon,
            "media-playback-start-symbolic"
        );
        assert_eq!(
            media_view(player(PlaybackStatus::Paused)).icon,
            "media-playback-pause-symbolic"
        );
        assert_eq!(
            media_view(player(PlaybackStatus::Stopped)).icon,
            "media-playback-stop-symbolic"
        );
    }
}
