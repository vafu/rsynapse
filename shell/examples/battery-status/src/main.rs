use relm4::prelude::*;
use shell_core::{
    ShellApp,
    gtk::{self, prelude::*},
    source::{
        self, Observable,
        rx::{BoxedSubscriptionSend, IntoBoxedSubscription, Observable as _, Observer},
    },
};
use zbus::{Connection, Proxy, proxy};

#[proxy(
    interface = "org.freedesktop.UPower",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower"
)]
trait UPower {
    #[zbus(property)]
    fn on_battery(&self) -> zbus::fdo::Result<bool>;
}

#[proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/devices/DisplayDevice"
)]
trait DisplayDevice {
    #[zbus(property)]
    fn percentage(&self) -> zbus::fdo::Result<f64>;

    #[zbus(property)]
    fn state(&self) -> zbus::fdo::Result<u32>;
}

#[derive(Clone, Debug, PartialEq)]
struct BatteryStatus {
    percentage: f64,
    state: BatteryState,
    on_battery: bool,
}

impl BatteryStatus {
    fn percent_label(&self) -> String {
        format!("{:.0}%", self.percentage)
    }

    fn detail_label(&self) -> String {
        let power_source = if self.on_battery { "battery" } else { "AC" };
        format!("{} on {}", self.state.label(), power_source)
    }

    fn fraction(&self) -> f64 {
        (self.percentage / 100.0).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BatteryState {
    Unknown,
    Charging,
    Discharging,
    Empty,
    FullyCharged,
    PendingCharge,
    PendingDischarge,
}

impl BatteryState {
    const fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Charging => "Charging",
            Self::Discharging => "Discharging",
            Self::Empty => "Empty",
            Self::FullyCharged => "Fully charged",
            Self::PendingCharge => "Pending charge",
            Self::PendingDischarge => "Pending discharge",
        }
    }
}

impl From<u32> for BatteryState {
    fn from(value: u32) -> Self {
        match value {
            1 => Self::Charging,
            2 => Self::Discharging,
            3 => Self::Empty,
            4 => Self::FullyCharged,
            5 => Self::PendingCharge,
            6 => Self::PendingDischarge,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum BatteryView {
    Waiting,
    Ready(BatteryStatus),
}

impl BatteryView {
    fn percent_label(&self) -> String {
        match self {
            Self::Waiting => "Waiting for UPower".to_owned(),
            Self::Ready(status) => status.percent_label(),
        }
    }

    fn detail_label(&self) -> String {
        match self {
            Self::Waiting => "Subscribing to DBus properties".to_owned(),
            Self::Ready(status) => status.detail_label(),
        }
    }

    fn fraction(&self) -> f64 {
        match self {
            Self::Waiting => 0.0,
            Self::Ready(status) => status.fraction(),
        }
    }
}

struct BatteryWindow {
    battery: BatteryView,
    last_error: Option<String>,
    _subscription: BoxedSubscriptionSend,
}

#[derive(Debug)]
enum BatteryInput {
    Status(BatteryStatus),
    SourceError(String),
}

#[relm4::component(async)]
impl SimpleAsyncComponent for BatteryWindow {
    type Init = ();
    type Input = BatteryInput;
    type Output = ();

    view! {
        #[root]
        gtk::Window {
            set_title: Some("Battery Status"),
            set_default_size: (320, 140),

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
                set_margin_top: 16,
                set_margin_bottom: 16,
                set_margin_start: 16,
                set_margin_end: 16,

                gtk::Label {
                    add_css_class: "title-1",
                    set_halign: gtk::Align::Start,
                    #[watch]
                    set_label: &model.battery.percent_label(),
                },

                gtk::Label {
                    set_halign: gtk::Align::Start,
                    #[watch]
                    set_label: &model.battery.detail_label(),
                },

                gtk::ProgressBar {
                    set_hexpand: true,
                    #[watch]
                    set_fraction: model.battery.fraction(),
                },

                gtk::Label {
                    add_css_class: "error",
                    set_wrap: true,
                    set_halign: gtk::Align::Start,
                    #[watch]
                    set_visible: model.last_error.is_some(),
                    #[watch]
                    set_label: model.last_error.as_deref().unwrap_or(""),
                }
            }
        }
    }

    async fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let subscription = battery_status()
            .subscribe_with(BatteryObserver(sender.input_sender().clone()))
            .into_boxed();

        let model = Self {
            battery: BatteryView::Waiting,
            last_error: None,
            _subscription: subscription,
        };
        let widgets = view_output!();
        AsyncComponentParts { model, widgets }
    }

    async fn update(&mut self, msg: Self::Input, _sender: AsyncComponentSender<Self>) {
        match msg {
            BatteryInput::Status(status) => {
                self.battery = BatteryView::Ready(status);
                self.last_error = None;
            }
            BatteryInput::SourceError(error) => {
                self.last_error = Some(error);
            }
        }
    }
}

struct BatteryObserver(relm4::Sender<BatteryInput>);

impl Observer<BatteryStatus, String> for BatteryObserver {
    fn next(&mut self, value: BatteryStatus) {
        self.0.emit(BatteryInput::Status(value));
    }

    fn error(self, error: String) {
        self.0.emit(BatteryInput::SourceError(error));
    }

    fn complete(self) {}

    fn is_closed(&self) -> bool {
        false
    }
}

fn battery_status() -> Observable<BatteryStatus> {
    let percentage = source::dbus::proxy_property::<f64, _, _>(
        "upower.display-device.Percentage",
        "Percentage",
        display_device_proxy,
    );
    let state = source::dbus::proxy_property::<u32, _, _>(
        "upower.display-device.State",
        "State",
        display_device_proxy,
    )
    .map(BatteryState::from)
    .box_it();
    let on_battery = source::dbus::proxy_property::<bool, _, _>(
        "upower.root.OnBattery",
        "OnBattery",
        upower_proxy,
    );

    percentage
        .combine_latest(state, |percentage, state| (percentage, state))
        .combine_latest(on_battery, |(percentage, state), on_battery| {
            BatteryStatus {
                percentage,
                state,
                on_battery,
            }
        })
        .distinct_until_changed()
        .box_it()
}

async fn upower_proxy() -> zbus::Result<Proxy<'static>> {
    let connection = Connection::system().await?;
    let proxy: UPowerProxy<'static> = UPowerProxy::new(&connection).await?;
    Ok(proxy.into_inner())
}

async fn display_device_proxy() -> zbus::Result<Proxy<'static>> {
    let connection = Connection::system().await?;
    let proxy: DisplayDeviceProxy<'static> = DisplayDeviceProxy::new(&connection).await?;
    Ok(proxy.into_inner())
}

fn main() {
    ShellApp::new("org.rsynapse.BatteryStatusExample")
        .with_relm_threads(2)
        .run_async::<BatteryWindow>(());
}
