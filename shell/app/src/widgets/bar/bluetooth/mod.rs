mod model;
mod source;
mod view;

use adw::prelude::*;
use relm4::prelude::*;
use shell_core::{gtk, list::ComponentListBoxExt, source::Observable};
use zbus::zvariant::OwnedObjectPath;

use crate::widgets::level_indicator::{
    self, LevelRenderStyle, LevelStage, LineStyle, TRACK_CLASSES,
};
use crate::widgets::material_icon;

const BATTERY_STAGES: &[LevelStage] = &[LevelStage {
    level: 5.0,
    class: "ok",
}];

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct BluetoothView {
    pub(super) status: BluetoothStatusView,
    pub(super) keyboard: DeviceGroupView,
    pub(super) audio: DeviceGroupView,
    pub(super) pointer: DeviceGroupView,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BluetoothStatusView {
    pub(super) icon: String,
    pub(super) connected_count: u8,
    pub(super) powered: bool,
    pub(super) adapter_path: Option<OwnedObjectPath>,
}

impl Default for BluetoothStatusView {
    fn default() -> Self {
        Self {
            icon: "bluetooth_disabled".to_owned(),
            connected_count: 0,
            powered: false,
            adapter_path: None,
        }
    }
}

pub(super) fn bluetooth_status() -> Observable<BluetoothView> {
    source::bluetooth_status()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BluetoothDeviceGroup {
    Keyboard,
    Audio,
    Pointer,
}

#[derive(Debug)]
#[shell_macros::model(module = bluetooth_group_popover_sources)]
pub(super) struct BluetoothGroupPopover {
    pub group: BluetoothDeviceGroup,

    #[source(source::bluetooth_group_devices(*group))]
    devices: Vec<BluetoothDeviceView>,
}

#[shell_macros::component(
    module = bluetooth_group_popover_sources,
    model = BluetoothGroupPopover
)]
#[relm4::component(pub(crate))]
impl SimpleComponent for BluetoothGroupPopover {
    type Init = BluetoothDeviceGroup;
    type Input = bluetooth_group_popover_sources::Msg;
    type Output = ();

    view! {
        #[root]
        gtk::Box {
            add_css_class: "bt-device-list",
            set_orientation: gtk::Orientation::Vertical,

            #[bind_list(devices, row = BluetoothDeviceRow)]
            devices -> gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = BluetoothGroupPopover::new(init);
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }
}

#[derive(Debug)]
pub(crate) struct BluetoothDeviceRow {
    device: BluetoothDeviceView,
}

#[relm4::component(pub(crate))]
impl SimpleComponent for BluetoothDeviceRow {
    type Init = BluetoothDeviceView;
    type Input = ();
    type Output = ();

    view! {
        #[root]
        adw::ActionRow {
            add_css_class: "bt-device-row",
            set_activatable: true,
            set_title: model.device.name.as_str(),
            set_subtitle: &device_subtitle(&model.device),

            add_prefix = &gtk::Image {
                add_css_class: "materialicon",
                set_icon_name: Some(material_icon::icon_name(model.device.icon.as_str()).as_str()),
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = BluetoothDeviceRow { device: init };
        let widgets = view_output!();
        let device_path = model.device.path.clone();
        let connect = !model.device.connected;
        let click = gtk::GestureClick::new();
        click.set_button(0);
        click.connect_released(move |_, _, _, _| {
            source::set_device_connected(device_path.clone(), connect);
        });
        root.add_controller(click);

        ComponentParts { model, widgets }
    }
}

pub(super) fn toggle_power(status: &BluetoothStatusView) {
    source::set_adapter_power(status.adapter_path.clone(), !status.powered);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DeviceGroupView {
    pub(super) visible: bool,
    pub(super) icon: String,
    pub(super) tinted: bool,
    pub(super) tooltip: String,
    pub(super) battery: Option<u8>,
    pub(super) devices: Vec<BluetoothDeviceView>,
}

impl Default for DeviceGroupView {
    fn default() -> Self {
        Self {
            visible: false,
            icon: "bluetooth".to_owned(),
            tinted: true,
            tooltip: String::new(),
            battery: None,
            devices: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BluetoothDeviceView {
    pub(super) path: OwnedObjectPath,
    pub(super) name: String,
    pub(super) address: String,
    pub(super) icon: String,
    pub(super) connected: bool,
    pub(super) connecting: bool,
    pub(super) battery: Option<u8>,
}

pub(super) fn status_count(status: &BluetoothStatusView) -> String {
    if status.connected_count == 0 {
        String::new()
    } else {
        status.connected_count.to_string()
    }
}

pub(super) fn status_tooltip(status: &BluetoothStatusView) -> String {
    if !status.powered {
        "Bluetooth disabled".to_owned()
    } else if status.connected_count == 0 {
        "Bluetooth".to_owned()
    } else {
        format!("{} Bluetooth device(s) connected", status.connected_count)
    }
}

pub(super) fn group_classes(group: &DeviceGroupView) -> Vec<&'static str> {
    let mut classes = vec!["flat", "circular", "panel-widget", "bt-device-button"];
    if group.tinted {
        classes.push("tinted");
    }
    classes
}

pub(super) fn battery_root_classes() -> Vec<&'static str> {
    level_indicator::root_classes(["line", "battery", "bt-battery-indicator"])
}

pub(super) fn battery_track_classes() -> &'static [&'static str] {
    TRACK_CLASSES
}

pub(super) fn battery_level_classes(group: &DeviceGroupView) -> Vec<&'static str> {
    level_indicator::level_classes(f64::from(group.battery.unwrap_or(0)), 0.0, BATTERY_STAGES)
}

pub(super) fn battery_track_draw_func()
-> impl Fn(&gtk::DrawingArea, &gtk::cairo::Context, i32, i32) + 'static {
    level_indicator::track_draw_func(LevelRenderStyle::Line(LineStyle::vertical(3.0)))
}

pub(super) fn battery_level_draw_func(
    group: &DeviceGroupView,
) -> impl Fn(&gtk::DrawingArea, &gtk::cairo::Context, i32, i32) + 'static {
    level_indicator::level_draw_func(
        f64::from(group.battery.unwrap_or(0)),
        0.0,
        100.0,
        LevelRenderStyle::Line(LineStyle::vertical(3.0)),
    )
}

fn device_subtitle(device: &BluetoothDeviceView) -> String {
    let mut subtitle = if device.connected {
        "Connected"
    } else if device.connecting {
        "Connecting..."
    } else {
        "Disconnected"
    }
    .to_owned();

    if let Some(battery) = device.battery {
        subtitle.push_str(&format!(" - {battery}%"));
    }

    subtitle
}
