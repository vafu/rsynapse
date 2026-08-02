use shell_core::source::{self, Observable};

use crate::widgets::nerd_icon::NerdIcon;
use crate::widgets::nerd_icon::fa;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct MprisView {
    pub(super) visible: bool,
    pub(super) metadata: String,
    pub(super) tooltip: String,
    pub(super) state_class: &'static str,
    pub(super) art_url: String,
    pub(super) playerctl_name: String,
    pub(super) play_pause_icon: NerdIcon,
    pub(super) can_play_pause: bool,
    pub(super) can_go_next: bool,
    pub(super) can_go_previous: bool,
}

impl Default for MprisView {
    fn default() -> Self {
        Self {
            visible: false,
            metadata: String::new(),
            tooltip: String::new(),
            state_class: "",
            art_url: String::new(),
            playerctl_name: String::new(),
            play_pause_icon: NerdIcon::new(fa::FA_PLAY),
            can_play_pause: false,
            can_go_next: false,
            can_go_previous: false,
        }
    }
}

pub(super) fn mpris_status() -> Observable<MprisView> {
    source::once(MprisView::default())
}
