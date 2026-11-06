use zbus::{Connection, Proxy};

use super::MediaAction;

const PLAYER_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";

pub(super) async fn perform(player: &str, action: MediaAction) -> Result<(), String> {
    let connection = Connection::session()
        .await
        .map_err(|error| format!("connect MPRIS session bus failed: {error}"))?;
    let proxy = Proxy::new(
        &connection,
        player.to_owned(),
        PLAYER_PATH,
        PLAYER_INTERFACE,
    )
    .await
    .map_err(|error| format!("connect MPRIS player {player} failed: {error}"))?;
    let method = match action {
        MediaAction::Previous => "Previous",
        MediaAction::PlayPause => "PlayPause",
        MediaAction::Next => "Next",
    };
    proxy
        .call::<_, _, ()>(method, &())
        .await
        .map_err(|error| format!("MPRIS {method} failed for {player}: {error}"))
}
