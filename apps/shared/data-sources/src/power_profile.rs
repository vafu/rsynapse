//! PowerProfiles active-profile control.
use crate::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
};

pub const BUS_NAME: &str = "net.hadess.PowerProfiles";
pub const INTERFACE: &str = "net.hadess.PowerProfiles";
pub const OBJECT_PATH: &str = "/net/hadess/PowerProfiles";

pub fn active() -> Observable<String> {
    dbus::property_or::<String>(
        PropertyDescriptor::new(object(), "ActiveProfile"),
        String::new(),
    )
}

pub fn set_active(profile: String) {
    crate::write::spawn("power-profile", async move {
        let connection = crate::write::system_connection().await?;
        let proxy = zbus::Proxy::new(connection, BUS_NAME, OBJECT_PATH, INTERFACE).await?;
        proxy.set_property("ActiveProfile", &profile).await
    });
}

fn object() -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::System, BUS_NAME, OBJECT_PATH, INTERFACE)
        .expect("PowerProfiles descriptor should be valid")
}
