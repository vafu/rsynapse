use crate::{Observable, dbus, rx::Observable as _};

/// Project-owned labels. `None` is unavailable/loading; `Some([])` is a loaded
/// project without labels. Never turn a service restart into an empty filter.
pub fn project_labels(id: impl Into<String>) -> Observable<Option<Vec<String>>> {
    let id = id.into();
    crate::shared_by_key("projd-project-labels", id.clone(), move || {
        let path = proj_model::object_path("Projects", &id);
        crate::switch_map(
            dbus::service_owner(dbus::Bus::Session, proj_model::BUS_NAME),
            move |owner| {
                let Some(owner) = owner else {
                    return crate::once(None);
                };
                let object = dbus::ObjectDescriptor::parse(
                    dbus::Bus::Session,
                    &owner,
                    &path,
                    proj_model::PROJECT_INTERFACE,
                )
                .expect("valid project label descriptor");
                dbus::property::<Vec<String>>(dbus::PropertyDescriptor::new(object, "Labels"))
                    .start_with(vec![None])
                    .box_it()
            },
        )
        .distinct_until_changed()
        .box_it()
    })
}
