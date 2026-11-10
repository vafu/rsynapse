use data_sources::{self, Observable, rx::Observable as _};

use super::AudioView;

pub(in crate::widgets::bar) fn audio_status() -> Observable<AudioView> {
    data_sources::audio::status()
        .map(|status| AudioView {
            visible: status.visible,
            icon: status.icon,
            tooltip: status.tooltip,
            percent: status.percent,
            muted: status.muted,
        })
        .distinct_until_changed()
        .box_it()
}
