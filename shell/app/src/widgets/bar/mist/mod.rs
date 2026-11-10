mod device_row;

use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    list::ComponentListBoxExt,
    source::{self, Observable, mist, rx::Observable as _},
};

use self::device_row::MistDeviceRow;
use crate::widgets::bar::bar_item;
use crate::widgets::nerd_icon::{NerdIcon, NerdIconLabelExt, md};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MistView {
    pub(super) visible: bool,
    pub(super) connected: bool,
    pub(super) connected_count: usize,
    pub(super) device_count: usize,
    pub(super) empty: bool,
    pub(super) icon: NerdIcon,
}

impl Default for MistView {
    fn default() -> Self {
        Self {
            visible: false,
            connected: false,
            connected_count: 0,
            device_count: 0,
            empty: false,
            icon: NerdIcon::new(md::MD_BRIDGE),
        }
    }
}

#[derive(Debug)]
#[shell_macros::model(module = mist_indicator_sources)]
pub(crate) struct MistIndicator {
    #[source(mist_status())]
    view: MistView,

    /// Structural device keys; drives popover row reconciliation only.
    #[source(mist::device_keys())]
    devices: Vec<String>,
}

#[shell_macros::component(module = mist_indicator_sources, model = MistIndicator)]
#[relm4::component(pub(crate))]
impl SimpleComponent for MistIndicator {
    type Init = ();
    type Input = mist_indicator_sources::Msg;
    type Output = ();

    view! {
        #[root]
        gtk::MenuButton {
            #[watch]
            set_visible: model.view.visible,
            set_css_classes: &bar_item::action_classes(&["mist-button"]),
            set_always_show_arrow: false,
            set_has_frame: false,
            #[watch]
            set_tooltip_text: Some(status_tooltip(&model.view).as_str()),

            #[wrap(Some)]
            set_popover = &gtk::Popover {
                add_css_class: "mist-popover",
                set_position: gtk::PositionType::Bottom,

                gtk::Box {
                    add_css_class: "mist-device-list",
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 6,
                    set_margin_all: 10,

                    gtk::Label {
                        add_css_class: "mist-empty",
                        set_label: "No paired devices",
                        #[watch]
                        set_visible: model.view.empty,
                    },

                    #[bind_list(devices, row = MistDeviceRow)]
                    devices -> gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 6,
                    },
                },
            },

            #[wrap(Some)]
            set_child = &gtk::Overlay {
                gtk::Label {
                    set_css_classes: &["nerdicon", bar_item::ICON_CLASS],
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    #[watch]
                    set_nerd_icon: model.view.icon.clone(),
                },

                add_overlay = &gtk::Label {
                    add_css_class: "mist-count",
                    #[watch]
                    set_visible: model.view.connected_count > 0,
                    #[watch]
                    set_label: connected_count_label(&model.view).as_str(),
                }
            }
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = MistIndicator::new();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        MistIndicator::update(self, message);
    }
}

pub(super) fn mist_status() -> Observable<MistView> {
    source::shared_by_key("rsynapse.mist-status", mist::OBJECT_PATH, || {
        mist::devices()
            .map(|devices| match devices {
                None => MistView::default(),
                Some(devices) => mist_view(&devices),
            })
            .distinct_until_changed()
            .box_it()
    })
}

fn mist_view(devices: &[mist::MistDevice]) -> MistView {
    let connected_count = devices
        .iter()
        .filter(|device| device.status == mist::DeviceStatus::Connected)
        .count();
    let connected = connected_count > 0;
    MistView {
        visible: true,
        connected,
        connected_count,
        device_count: devices.len(),
        empty: devices.is_empty(),
        icon: NerdIcon::new(md::MD_BRIDGE),
    }
}

pub(super) fn connected_count_label(view: &MistView) -> String {
    if view.connected_count == 0 {
        String::new()
    } else {
        view.connected_count.to_string()
    }
}

pub(super) fn status_tooltip(view: &MistView) -> String {
    if !view.visible {
        return "Mist is not running".to_owned();
    }
    if view.device_count == 0 {
        return "Mist: no paired devices".to_owned();
    }
    let state = if view.connected {
        format!(
            "connected ({} of {} devices)",
            view.connected_count, view.device_count
        )
    } else {
        format!("disconnected ({} devices)", view.device_count)
    };
    format!("Mist: {state}")
}

#[cfg(test)]
mod tests {
    use super::{MistView, mist_view, status_tooltip};
    use shell_core::source::mist::{ConnectionTransport, DeviceStatus, MistDevice};

    fn device(status: DeviceStatus) -> MistDevice {
        MistDevice {
            endpoint_id: "endpoint".to_owned(),
            name: "Pixel".to_owned(),
            class: shell_core::source::mist::DeviceClass::Phone,
            platform: "android".to_owned(),
            status,
            transport: ConnectionTransport::Unknown,
            selected: true,
        }
    }

    #[test]
    fn tooltip_reports_absence() {
        assert_eq!(status_tooltip(&MistView::default()), "Mist is not running");
    }

    #[test]
    fn view_counts_connected_devices() {
        let view = mist_view(&[
            device(DeviceStatus::Connected),
            device(DeviceStatus::Disconnected),
        ]);
        assert!(view.visible);
        assert!(view.connected);
        assert_eq!(view.device_count, 2);
        assert_eq!(view.connected_count, 1);
        assert!(!view.empty);
    }

    #[test]
    fn empty_snapshot_marks_empty() {
        let view = mist_view(&[]);
        assert!(view.visible);
        assert!(!view.connected);
        assert!(view.empty);
    }
}
