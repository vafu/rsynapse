use data_sources::{Observable, mist, rx::Observable as _};
use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use crate::widgets::nerd_icon::{NerdIcon, NerdIconLabelExt, md};

/// One device row in the Mist popover.
///
/// The row init is only the stable endpoint key; live status/name/selection come
/// from `mist::device(endpoint_id)` via this row's own `#[source]`, so an update
/// to one device never rebuilds the list.
#[derive(Debug)]
#[shell_macros::model(module = mist_device_row_sources)]
pub(super) struct MistDeviceRow {
    endpoint_id: String,

    #[source(device_source(endpoint_id.clone()))]
    device: Option<RowView>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RowView {
    name: String,
    status_label: String,
    selected: bool,
    connected: bool,
    icon: NerdIcon,
}

fn device_source(endpoint_id: String) -> Observable<Option<RowView>> {
    mist::device(endpoint_id)
        .map(|device| device.map(row_view))
        .distinct_until_changed()
        .box_it()
}

fn row_view(device: mist::MistDevice) -> RowView {
    RowView {
        name: device.name,
        status_label: status_label(device.status, device.transport),
        selected: device.selected,
        connected: device.status == mist::DeviceStatus::Connected,
        icon: class_icon(device.class),
    }
}

fn status_label(status: mist::DeviceStatus, transport: mist::ConnectionTransport) -> String {
    match status {
        mist::DeviceStatus::Connected => match mist::transport_label(transport) {
            Some(transport) => format!("connected · {transport}"),
            None => "connected".to_owned(),
        },
        mist::DeviceStatus::Connecting => "connecting".to_owned(),
        mist::DeviceStatus::Disconnected => "disconnected".to_owned(),
    }
}

fn class_icon(class: mist::DeviceClass) -> NerdIcon {
    use mist::DeviceClass;
    NerdIcon::new(match class {
        DeviceClass::Phone => md::MD_CELLPHONE,
        DeviceClass::Tablet => md::MD_TABLET,
        DeviceClass::Laptop => md::MD_LAPTOP,
        DeviceClass::Desktop => md::MD_DESKTOP_TOWER,
        DeviceClass::Server => md::MD_SERVER,
        DeviceClass::Other => md::MD_DEVICES,
    })
}

#[shell_macros::component(module = mist_device_row_sources, model = MistDeviceRow)]
#[relm4::component(pub(crate))]
impl SimpleComponent for MistDeviceRow {
    type Init = String;
    type Input = mist_device_row_sources::Msg;
    type Output = ();

    view! {
        #[root]
        gtk::Box {
            add_css_class: "mist-device-row",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 8,
            set_valign: gtk::Align::Center,

            gtk::Label {
                add_css_class: "nerdicon",
                add_css_class: "mist-device-icon",
                set_valign: gtk::Align::Center,
                #[watch]
                set_nerd_icon: model.device.as_ref().map_or_else(
                    || NerdIcon::new(md::MD_BRIDGE),
                    |device| device.icon.clone(),
                ),
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_hexpand: true,

                gtk::Label {
                    add_css_class: "mist-device-name",
                    set_xalign: 0.0,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    #[watch]
                    set_label: model.device.as_ref().map_or("", |device| device.name.as_str()),
                },

                gtk::Label {
                    set_xalign: 0.0,
                    #[watch]
                    set_css_classes: &status_classes(model.device.as_ref()),
                    #[watch]
                    set_label: model.device.as_ref().map_or("", |device| device.status_label.as_str()),
                },
            },

            gtk::Label {
                add_css_class: "mist-device-selected",
                set_label: "selected",
                #[watch]
                set_visible: model.device.as_ref().is_some_and(|device| device.selected),
            },
        }
    }

    fn init(
        endpoint_id: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = MistDeviceRow::new(endpoint_id);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        MistDeviceRow::update(self, message);
    }
}

fn status_classes(device: Option<&RowView>) -> Vec<&'static str> {
    let mut classes = vec!["mist-device-status"];
    if device.is_some_and(|device| device.connected) {
        classes.push("mist-connected");
    } else {
        classes.push("mist-disconnected");
    }
    classes
}

#[cfg(test)]
mod tests {
    use super::{class_icon, row_view, status_label};
    use crate::widgets::nerd_icon::md;
    use data_sources::mist::{ConnectionTransport, DeviceClass, DeviceStatus, MistDevice};

    fn device(status: DeviceStatus, transport: ConnectionTransport) -> MistDevice {
        MistDevice {
            endpoint_id: "endpoint".to_owned(),
            name: "Pixel".to_owned(),
            class: DeviceClass::Phone,
            platform: "android".to_owned(),
            status,
            transport,
            selected: true,
        }
    }

    #[test]
    fn connected_row_includes_transport() {
        let view = row_view(device(DeviceStatus::Connected, ConnectionTransport::Relay));
        assert_eq!(view.status_label, "connected · relay");
        assert!(view.connected);
        assert!(view.selected);
        assert_eq!(view.icon.glyph(), md::MD_CELLPHONE);
    }

    #[test]
    fn unknown_transport_is_omitted() {
        assert_eq!(
            status_label(DeviceStatus::Connected, ConnectionTransport::Unknown),
            "connected"
        );
    }

    #[test]
    fn disconnected_row_keeps_class_icon() {
        let view = row_view(device(
            DeviceStatus::Disconnected,
            ConnectionTransport::Unknown,
        ));
        assert!(!view.connected);
        assert_eq!(view.status_label, "disconnected");
        assert_eq!(view.icon.glyph(), md::MD_CELLPHONE);
    }

    #[test]
    fn class_icon_matches_declared_device_class() {
        assert_eq!(class_icon(DeviceClass::Phone).glyph(), md::MD_CELLPHONE);
        assert_eq!(class_icon(DeviceClass::Tablet).glyph(), md::MD_TABLET);
        assert_eq!(class_icon(DeviceClass::Laptop).glyph(), md::MD_LAPTOP);
        assert_eq!(
            class_icon(DeviceClass::Desktop).glyph(),
            md::MD_DESKTOP_TOWER
        );
        assert_eq!(class_icon(DeviceClass::Server).glyph(), md::MD_SERVER);
        assert_eq!(class_icon(DeviceClass::Other).glyph(), md::MD_DEVICES);
    }
}
