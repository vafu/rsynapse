//! Notification service state exposed by the standalone notifications app.

use crate::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
};

const NOTIFICATIONS_BUS_NAME: &str = "org.freedesktop.Notifications";
const NOTIFICATIONS_OBJECT_PATH: &str = "/org/freedesktop/Notifications";
const RSYNAPSE_NOTIFICATIONS_INTERFACE: &str = "org.rsynapse.Notifications1";

pub fn has_notification_items() -> Observable<bool> {
    dbus::property_or(
        PropertyDescriptor::new(notification_state_object(), "HasItems"),
        false,
    )
}

fn notification_state_object() -> ObjectDescriptor {
    ObjectDescriptor::parse(
        Bus::Session,
        NOTIFICATIONS_BUS_NAME,
        NOTIFICATIONS_OBJECT_PATH,
        RSYNAPSE_NOTIFICATIONS_INTERFACE,
    )
    .expect("static notification state D-Bus descriptor should be valid")
}
