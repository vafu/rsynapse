use shell_core::source::Source;

#[shell_macros::dbus_model(
    module = root,
    interface = "org.rsynapse.Niri1",
    default_service = "org.rsynapse.Niri",
    default_path = "/org/rsynapse/Niri"
)]
struct NiriRoot {
    #[dbus(model)]
    windows: Vec<NiriWindow>,
}

#[shell_macros::dbus_model(
    module = window,
    interface = "org.rsynapse.Niri1.Window",
    default_service = "org.rsynapse.Niri"
)]
pub(crate) struct NiriWindow {
    id: u64,
    title: Option<String>,
    app_id: Option<String>,
    focused: bool,
}

pub(crate) fn windows() -> Source<Vec<NiriWindow>> {
    NiriRoot::new().windows()
}

#[cfg(test)]
mod tests {
    use super::{NiriRoot, NiriWindow};

    #[test]
    fn dbus_models_are_generated() {
        let _ = std::any::type_name::<NiriRoot>();
        let _ = std::any::type_name::<NiriWindow>();
    }
}
