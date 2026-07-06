use shell_core::source::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, ObjectManagerDescriptor, ObjectModel, PropertyDescriptor},
    rx::Observable as _,
};
use zbus::zvariant::OwnedObjectPath;

const BUS_NAME: &str = "org.rsynapse.Niri";
const ROOT_PATH: &str = "/org/rsynapse/Niri";
const ROOT_INTERFACE: &str = "org.rsynapse.Niri1";
const WORKSPACE_INTERFACE: &str = "org.rsynapse.Niri1.Workspace";
const WINDOW_INTERFACE: &str = "org.rsynapse.Niri1.Window";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NiriWorkspace {
    path: OwnedObjectPath,
}

impl NiriWorkspace {
    fn at(path: OwnedObjectPath) -> Self {
        Self { path }
    }

    pub(super) fn path_key(&self) -> &str {
        self.path.as_str()
    }

    pub(super) fn id(&self) -> Observable<u64> {
        required(self.property("Id"), 0)
    }

    pub(super) fn name(&self) -> Observable<Option<String>> {
        optional(self.property("Name"))
    }

    pub(super) fn index(&self) -> Observable<u8> {
        required(self.property("Index"), 0)
    }

    pub(super) fn focused(&self) -> Observable<bool> {
        required(self.property("Focused"), false)
    }

    pub(super) fn urgent(&self) -> Observable<bool> {
        required(self.property("Urgent"), false)
    }

    fn property(&self, name: &'static str) -> PropertyDescriptor {
        property(self.path.as_str(), WORKSPACE_INTERFACE, name)
    }
}

impl ObjectModel for NiriWorkspace {
    const INTERFACE: &'static str = WORKSPACE_INTERFACE;

    fn at(path: OwnedObjectPath) -> Self {
        Self { path }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NiriWindow {
    path: OwnedObjectPath,
}

impl NiriWindow {
    pub(super) fn path_key(&self) -> &str {
        self.path.as_str()
    }

    pub(super) fn id(&self) -> Observable<u64> {
        required(self.property("Id"), 0)
    }

    pub(super) fn title(&self) -> Observable<Option<String>> {
        optional(self.property("Title"))
    }

    pub(super) fn app_id(&self) -> Observable<Option<String>> {
        optional(self.property("AppId"))
    }

    pub(super) fn workspace(&self) -> Observable<Option<NiriWorkspace>> {
        model_optional(self.property("Workspace"), NiriWorkspace::at)
    }

    pub(super) fn focused(&self) -> Observable<bool> {
        required(self.property("Focused"), false)
    }

    pub(super) fn urgent(&self) -> Observable<bool> {
        required(self.property("Urgent"), false)
    }

    pub(super) fn column_index(&self) -> Observable<Option<u64>> {
        optional(self.property("ColumnIndex"))
    }

    pub(super) fn row_index(&self) -> Observable<Option<u64>> {
        optional(self.property("RowIndex"))
    }

    fn property(&self, name: &'static str) -> PropertyDescriptor {
        property(self.path.as_str(), WINDOW_INTERFACE, name)
    }
}

impl ObjectModel for NiriWindow {
    const INTERFACE: &'static str = WINDOW_INTERFACE;

    fn at(path: OwnedObjectPath) -> Self {
        Self { path }
    }
}

pub(super) fn workspaces() -> Observable<Vec<NiriWorkspace>> {
    dbus::models::<NiriWorkspace>(niri_object_manager())
        .distinct_until_changed()
        .box_it()
}

pub(super) fn windows() -> Observable<Vec<NiriWindow>> {
    dbus::models::<NiriWindow>(niri_object_manager())
        .distinct_until_changed()
        .box_it()
}

pub(super) fn focused_workspace() -> Observable<Option<NiriWorkspace>> {
    model_optional(root_property("FocusedWorkspace"), NiriWorkspace::at)
        .distinct_until_changed()
        .box_it()
}

fn required<T>(descriptor: PropertyDescriptor, default: T) -> Observable<T>
where
    T: TryFrom<zbus::zvariant::OwnedValue> + Clone + PartialEq + Send + 'static,
    T::Error: std::fmt::Display,
{
    dbus::property_or(descriptor, default)
}

fn optional<T>(descriptor: PropertyDescriptor) -> Observable<Option<T>>
where
    T: Clone + PartialEq + Send + 'static,
    Vec<T>: TryFrom<zbus::zvariant::OwnedValue>,
    <Vec<T> as TryFrom<zbus::zvariant::OwnedValue>>::Error: std::fmt::Display,
{
    dbus::optional_array_property(descriptor)
}

fn model_optional<T>(
    descriptor: PropertyDescriptor,
    map: fn(OwnedObjectPath) -> T,
) -> Observable<Option<T>>
where
    T: Send + 'static,
{
    dbus::optional_array_property::<OwnedObjectPath>(descriptor)
        .map(move |path| path.map(map))
        .box_it()
}

fn root_property(name: &'static str) -> PropertyDescriptor {
    property(ROOT_PATH, ROOT_INTERFACE, name)
}

fn niri_object_manager() -> ObjectManagerDescriptor {
    ObjectManagerDescriptor::parse(Bus::Session, BUS_NAME, ROOT_PATH)
        .expect("static niri ObjectManager descriptor should be valid")
}

fn property(path: &str, interface: &str, name: &'static str) -> PropertyDescriptor {
    PropertyDescriptor::new(object(path, interface), name)
}

fn object(path: &str, interface: &str) -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::Session, BUS_NAME, path, interface)
        .expect("static niri D-Bus descriptor should be valid")
}

#[cfg(test)]
mod tests {
    use shell_core::source::dbus::ObjectModel;

    use super::{NiriWindow, NiriWorkspace};

    #[test]
    fn model_handles_are_comparable_by_path() {
        let path = zbus::zvariant::OwnedObjectPath::try_from("/org/rsynapse/Niri/Windows/window_7")
            .unwrap();

        assert_eq!(
            <NiriWindow as ObjectModel>::at(path.clone()),
            <NiriWindow as ObjectModel>::at(path)
        );
        let _ = std::any::type_name::<NiriWorkspace>();
    }
}
