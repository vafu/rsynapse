use super::MediaAction;

pub(super) async fn perform(player: &str, action: MediaAction) -> Result<(), String> {
    let action = match action {
        MediaAction::Previous => data_sources::mpris::MediaAction::Previous,
        MediaAction::PlayPause => data_sources::mpris::MediaAction::PlayPause,
        MediaAction::Next => data_sources::mpris::MediaAction::Next,
    };
    data_sources::mpris::control(player, action).await
}
