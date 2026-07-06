mod route_popover;
mod route_row;
mod source;

pub(super) use route_popover::AudioRoutePopover;
use route_row::AudioRouteRow;
use source::audio_routes;
pub(super) use source::audio_status;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AudioView {
    pub(super) visible: bool,
    pub(super) icon: String,
    pub(super) tooltip: String,
    pub(super) percent: u8,
    pub(super) muted: bool,
}

impl Default for AudioView {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AudioRouteView {
    pub(super) id: u32,
    pub(super) name: String,
    pub(super) title: String,
    pub(super) subtitle: String,
    pub(super) icon: String,
    pub(super) is_default: bool,
}

pub(super) fn route_popover_tooltip(audio: &AudioView) -> &str {
    if audio.tooltip.is_empty() {
        "Audio Output"
    } else {
        audio.tooltip.as_str()
    }
}
