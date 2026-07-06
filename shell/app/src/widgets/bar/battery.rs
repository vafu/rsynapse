use shell_core::source::{
    self, Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
    rx::Observable as _,
};
use shell_rx_macros::combine_latest;

const UPOWER_BUS: &str = "org.freedesktop.UPower";
const UPOWER_DEVICE_INTERFACE: &str = "org.freedesktop.UPower.Device";
const BATTERY_OBJECT_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BatteryView {
    pub(super) present: bool,
    pub(super) percent: u8,
    pub(super) state: BatteryState,
}

impl Default for BatteryView {
    fn default() -> Self {
        Self {
            present: false,
            percent: 0,
            state: BatteryState::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum BatteryState {
    Charging,
    Discharging,
    Empty,
    Full,
    PendingCharge,
    PendingDischarge,
    #[default]
    Unknown,
}

impl BatteryState {
    pub(super) fn is_charging(self) -> bool {
        matches!(self, Self::Charging | Self::Full)
    }
}

pub(super) fn battery_status() -> Observable<BatteryView> {
    source::shared_by_key("rsynapse.battery-status", BATTERY_OBJECT_PATH, || {
        combine_latest!(
            property_or::<bool>(BATTERY_OBJECT_PATH, "IsPresent", false),
            property_or::<f64>(BATTERY_OBJECT_PATH, "Percentage", 0.0).map(percent),
            property_or::<u32>(BATTERY_OBJECT_PATH, "State", 0)
                .map(battery_state)
                => |(present, percent, state)| BatteryView {
                    present,
                    percent,
                    state,
                },
        )
        .distinct_until_changed()
        .box_it()
    })
}

fn property_or<T>(path: &'static str, property: &'static str, default: T) -> Observable<T>
where
    T: TryFrom<zbus::zvariant::OwnedValue> + Clone + PartialEq + Send + 'static,
    T::Error: std::fmt::Display,
{
    dbus::property_or(PropertyDescriptor::new(object(path), property), default)
}

fn object(path: &'static str) -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::System, UPOWER_BUS, path, UPOWER_DEVICE_INTERFACE)
        .expect("UPower descriptor should be valid")
}

fn percent(value: f64) -> u8 {
    Some(value)
        .filter(|value| value.is_finite())
        .map(|value| value.round().clamp(0.0, 100.0) as u8)
        .unwrap_or(0)
}

fn battery_state(value: u32) -> BatteryState {
    match value {
        1 => BatteryState::Charging,
        2 => BatteryState::Discharging,
        3 => BatteryState::Empty,
        4 => BatteryState::Full,
        5 => BatteryState::PendingCharge,
        6 => BatteryState::PendingDischarge,
        _ => BatteryState::Unknown,
    }
}
