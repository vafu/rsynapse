//! Reactive workspace-dbus projection: typed object handles and observables.
use crate::{
    Observable,
    dbus::{self, Bus, ObjectDescriptor, ObjectManagerDescriptor, ObjectModel, PropertyDescriptor},
    rx::Observable as _,
};
use zbus::zvariant::OwnedObjectPath;

use workspace_dbus_api::{
    BUS_NAME, Layout, MANAGER_INTERFACE as ROOT_INTERFACE, OUTPUT_INTERFACE, ROOT_PATH,
    WINDOW_INTERFACE, WORKSPACE_INTERFACE,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceObject {
    path: OwnedObjectPath,
}

impl WorkspaceObject {
    fn at(path: OwnedObjectPath) -> Self {
        Self { path }
    }

    pub fn path_key(&self) -> &str {
        self.path.as_str()
    }

    pub fn id(&self) -> Observable<String> {
        dbus::property::<String>(self.property("Id"))
            .filter_map(|id| id.filter(|id| !id.is_empty()))
            .box_it()
    }

    pub fn name(&self) -> Observable<Option<String>> {
        optional(self.property("Name"))
    }

    pub fn index(&self) -> Observable<u32> {
        required(self.property("Index"), 0)
    }

    pub fn layout(&self) -> Observable<Layout> {
        required(self.property("Layout"), Layout::Unknown)
    }

    pub fn has_windows(&self) -> Observable<bool> {
        dbus::property::<Vec<OwnedObjectPath>>(self.property("Windows"))
            // Keep rows visible until their actual window inventory is known.
            .map(|windows| windows.is_none_or(|windows| !windows.is_empty()))
            .distinct_until_changed()
            .box_it()
    }

    pub fn focused(&self) -> Observable<bool> {
        required(self.property("Focused"), false)
    }

    pub fn active(&self) -> Observable<bool> {
        required(self.property("Active"), false)
    }

    pub fn urgent(&self) -> Observable<bool> {
        required(self.property("Urgent"), false)
    }

    pub fn output_path_key(&self) -> Observable<Option<String>> {
        dbus::optional_array_property::<OwnedObjectPath>(self.property("Output"))
            .map(|path| path.map(|path| path.as_str().to_owned()))
            .distinct_until_changed()
            .box_it()
    }

    fn property(&self, name: &'static str) -> PropertyDescriptor {
        property(self.path.as_str(), WORKSPACE_INTERFACE, name)
    }
}

impl ObjectModel for WorkspaceObject {
    const INTERFACE: &'static str = WORKSPACE_INTERFACE;

    fn at(path: OwnedObjectPath) -> Self {
        Self { path }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowObject {
    path: OwnedObjectPath,
}

impl WindowObject {
    pub fn path_key(&self) -> &str {
        self.path.as_str()
    }

    pub fn id(&self) -> Observable<String> {
        dbus::property::<String>(self.property("Id"))
            .filter_map(|id| id.filter(|id| !id.is_empty()))
            .box_it()
    }

    pub fn app_id(&self) -> Observable<Option<String>> {
        optional(self.property("AppId"))
    }

    pub fn workspace(&self) -> Observable<Option<WorkspaceObject>> {
        model_optional(self.property("Workspace"), WorkspaceObject::at)
    }

    pub fn focused(&self) -> Observable<bool> {
        required(self.property("Focused"), false)
    }

    pub fn urgent(&self) -> Observable<bool> {
        required(self.property("Urgent"), false)
    }

    pub fn column_index(&self) -> Observable<Option<u64>> {
        optional(self.property("ColumnIndex"))
    }

    pub fn row_index(&self) -> Observable<Option<u64>> {
        optional(self.property("RowIndex"))
    }

    fn property(&self, name: &'static str) -> PropertyDescriptor {
        property(self.path.as_str(), WINDOW_INTERFACE, name)
    }
}

impl ObjectModel for WindowObject {
    const INTERFACE: &'static str = WINDOW_INTERFACE;

    fn at(path: OwnedObjectPath) -> Self {
        Self { path }
    }
}

pub fn workspaces() -> Observable<Vec<WorkspaceObject>> {
    dbus::models::<WorkspaceObject>(object_manager())
        .distinct_until_changed()
        .box_it()
}

pub fn windows() -> Observable<Vec<WindowObject>> {
    dbus::models::<WindowObject>(object_manager())
        .distinct_until_changed()
        .box_it()
}

/// The Manager object's authoritative live window path list.
///
/// This is a single PropertiesChanged-backed list, so consumers do not need to
/// reconstruct membership from ObjectManager deltas.
pub fn window_objects() -> Observable<Vec<WindowObject>> {
    dbus::property::<Vec<OwnedObjectPath>>(root_property("Windows"))
        .map(|paths| {
            paths
                .unwrap_or_default()
                .into_iter()
                .map(WindowObject::at)
                .collect::<Vec<_>>()
        })
        .distinct_until_changed()
        .box_it()
}

pub fn focused_workspace() -> Observable<Option<WorkspaceObject>> {
    model_optional(root_property("FocusedWorkspace"), WorkspaceObject::at)
        .distinct_until_changed()
        .box_it()
}

pub fn current_workspace(output_name: Option<String>) -> Observable<Option<WorkspaceObject>> {
    let Some(output_name) = output_name else {
        return focused_workspace();
    };

    model_optional(
        property(
            output_path_for_name(&output_name).as_str(),
            OUTPUT_INTERFACE,
            "CurrentWorkspace",
        ),
        WorkspaceObject::at,
    )
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

pub fn output_path_for_name(name: &str) -> String {
    workspace_dbus_api::output_path(name).to_string()
}

fn root_property(name: &'static str) -> PropertyDescriptor {
    property(ROOT_PATH, ROOT_INTERFACE, name)
}

fn object_manager() -> ObjectManagerDescriptor {
    ObjectManagerDescriptor::parse(Bus::Session, BUS_NAME, ROOT_PATH)
        .expect("workspace ObjectManager descriptor should be valid")
}

fn property(path: &str, interface: &str, name: &'static str) -> PropertyDescriptor {
    PropertyDescriptor::new(object(path, interface), name)
}

fn object(path: &str, interface: &str) -> ObjectDescriptor {
    ObjectDescriptor::parse(Bus::Session, BUS_NAME, path, interface)
        .expect("workspace D-Bus descriptor should be valid")
}

#[cfg(test)]
mod tests {
    use crate::dbus::ObjectModel;

    use super::{WindowObject, WorkspaceObject};

    #[test]
    fn model_handles_are_comparable_by_path() {
        let path = workspace_dbus_api::window_path("opaque:7/é");

        assert_eq!(
            <WindowObject as ObjectModel>::at(path.clone()),
            <WindowObject as ObjectModel>::at(path)
        );
        let _ = std::any::type_name::<WorkspaceObject>();
    }
}
