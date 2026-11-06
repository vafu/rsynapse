use shell_core::source::{self, Observable, rx::Observable as _};

const BRIGHTNESS_ICON: &str = "display-brightness-symbolic";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct BrightnessView {
    pub(super) visible: bool,
    pub(super) icon: &'static str,
    pub(super) percent: u8,
}

pub(super) fn brightness_status() -> Observable<BrightnessView> {
    source::backlight::status()
        .map(|backlight| BrightnessView {
            visible: backlight.visible,
            icon: BRIGHTNESS_ICON,
            percent: backlight.percent,
        })
        .distinct_until_changed()
        .box_it()
}
