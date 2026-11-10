mod source;

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
