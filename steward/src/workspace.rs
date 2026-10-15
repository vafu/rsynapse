//! API-only reactive identity helpers. Never recover identity from path text.
use shell_source::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
    rx::Observable as _,
};
use workspace_dbus_api::{
    BUS_NAME, MANAGER_INTERFACE, ROOT_PATH, WINDOW_INTERFACE, WORKSPACE_INTERFACE,
};
use zbus::zvariant::OwnedObjectPath;

pub fn object(path: &str, interface: &str) -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::Session, BUS_NAME, path, interface).expect("workspace descriptor")
}
pub fn root_property(name: &'static str) -> PropertyDescriptor {
    PropertyDescriptor::new(object(ROOT_PATH, MANAGER_INTERFACE), name)
}
pub fn id(path: Option<OwnedObjectPath>, interface: &'static str) -> Observable<Option<String>> {
    let Some(path) = path else {
        return shell_source::once(None);
    };
    dbus::property_or(
        PropertyDescriptor::new(object(path.as_str(), interface), "Id"),
        String::new(),
    )
    .map(|id| (!id.is_empty()).then_some(id))
    .distinct_until_changed()
    .box_it()
}
pub fn focused_workspace_id() -> Observable<Option<String>> {
    shell_source::switch_map(
        dbus::optional_array_property::<OwnedObjectPath>(root_property("FocusedWorkspace")),
        |path| id(path, WORKSPACE_INTERFACE),
    )
}
pub fn focused_window_id() -> Observable<Option<String>> {
    shell_source::switch_map(
        dbus::optional_array_property::<OwnedObjectPath>(root_property("FocusedWindow")),
        |path| id(path, WINDOW_INTERFACE),
    )
}
