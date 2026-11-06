//! Hold-to-show MPRIS media overlay.
mod actions;
mod component;
mod controller;
mod helpers;
mod sone;

use shell_core::{
    gtk,
    keybind::HoldWatcher,
    source::{StateSignal, mpris::MprisSnapshot},
};

use sone::SoneLikeState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaAction {
    Previous,
    PlayPause,
    Next,
}

#[derive(Debug)]
pub(crate) enum MprisInput {
    Source(sources::Msg),
    Show {
        hold: bool,
    },
    Hide,
    HideWindow(u64),
    Toggle {
        hold: bool,
    },
    Control(MediaAction),
    ControlResult(Result<(), String>),
    ToggleLike,
    LikeStatus {
        track_id: String,
        result: Result<bool, String>,
    },
    LikeResult {
        track_id: String,
        liked: bool,
        result: Result<(), String>,
    },
}

impl From<sources::Msg> for MprisInput {
    fn from(message: sources::Msg) -> Self {
        Self::Source(message)
    }
}

#[shell_macros::model]
pub(crate) struct MprisWindow {
    window: gtk::Window,
    visible: bool,
    revealed: bool,
    hold: bool,
    generation: u64,
    _hold_watcher: HoldWatcher,
    active: StateSignal<bool>,
    sone_track_id: Option<String>,
    sone_like: SoneLikeState,
    sone_busy: bool,

    #[source(shell_core::source::switch_map(
        active.observable(),
        |active| if active {
            shell_core::source::mpris::player()
        } else {
            shell_core::source::once(MprisSnapshot::default())
        }
    ))]
    player: MprisSnapshot,
}
