use shell_core::source::{self, Observable};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct MprisView {
    pub(super) visible: bool,
    pub(super) metadata: String,
    pub(super) tooltip: String,
    pub(super) state_class: &'static str,
    pub(super) art_url: String,
    pub(super) playerctl_name: String,
    pub(super) play_pause_icon: &'static str,
    pub(super) can_play_pause: bool,
    pub(super) can_go_next: bool,
    pub(super) can_go_previous: bool,
}

pub(super) fn mpris_status() -> Observable<MprisView> {
    source::once(MprisView::default())
}
