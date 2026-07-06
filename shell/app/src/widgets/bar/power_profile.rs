use std::thread;

use shell_core::source::{
    self, Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
    rx::Observable as _,
};

const POWER_PROFILE_ORDER: &[&str] = &["power-saver", "balanced", "performance"];
const POWER_PROFILE_BUS: &str = "net.hadess.PowerProfiles";
const POWER_PROFILE_INTERFACE: &str = "net.hadess.PowerProfiles";
const POWER_PROFILES_OBJECT_PATH: &str = "/net/hadess/PowerProfiles";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PowerProfileView {
    pub(super) visible: bool,
    pub(super) profile: String,
    pub(super) icon: &'static str,
    pub(super) tooltip: String,
}

impl Default for PowerProfileView {
    fn default() -> Self {
        Self {
            visible: false,
            profile: String::new(),
            icon: "speed",
            tooltip: String::new(),
        }
    }
}

pub(super) fn power_profile_status() -> Observable<PowerProfileView> {
    source::shared_by_key("rsynapse.power-profile-status", "active", || {
        dbus::property_or::<String>(
            PropertyDescriptor::new(power_profiles_object(), "ActiveProfile"),
            String::new(),
        )
        .map(power_profile_view)
        .distinct_until_changed()
        .box_it()
    })
}

pub(super) fn cycle_power_profile(profile: &str) {
    let next = next_profile(profile).to_owned();

    thread::spawn(move || {
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
            .and_then(|runtime| {
                runtime.block_on(async move {
                    let connection = zbus::Connection::system()
                        .await
                        .map_err(|error| error.to_string())?;
                    let proxy = zbus::Proxy::new(
                        &connection,
                        POWER_PROFILE_BUS,
                        POWER_PROFILES_OBJECT_PATH,
                        POWER_PROFILE_INTERFACE,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    proxy
                        .set_property("ActiveProfile", &next)
                        .await
                        .map_err(|error| error.to_string())
                })
            });
        if let Err(error) = result {
            eprintln!("[power-profile] failed to set ActiveProfile: {error}");
        }
    });
}

fn power_profiles_object() -> ObjectDescriptor {
    ObjectDescriptor::parse(
        Bus::System,
        POWER_PROFILE_BUS,
        POWER_PROFILES_OBJECT_PATH,
        POWER_PROFILE_INTERFACE,
    )
    .expect("PowerProfiles descriptor should be valid")
}

fn power_profile_view(profile: String) -> PowerProfileView {
    let profile = profile.trim().to_owned();
    if profile.is_empty() {
        return PowerProfileView::default();
    }

    PowerProfileView {
        visible: true,
        tooltip: tooltip(&profile),
        icon: icon_name(&profile),
        profile,
    }
}

fn icon_name(profile: &str) -> &'static str {
    match profile {
        "performance" => "bolt",
        "power-saver" => "eco",
        _ => "speed",
    }
}

fn tooltip(profile: &str) -> String {
    match profile {
        "performance" => "Performance".to_owned(),
        "power-saver" => "Power Saver".to_owned(),
        "balanced" => "Balanced".to_owned(),
        _ => profile.to_owned(),
    }
}

fn next_profile(profile: &str) -> &'static str {
    let current = POWER_PROFILE_ORDER
        .iter()
        .position(|candidate| *candidate == profile)
        .unwrap_or(1);
    POWER_PROFILE_ORDER[(current + 1) % POWER_PROFILE_ORDER.len()]
}
