use super::*;

pub(super) struct SecondaryBar {
    pub(super) output_name: String,
    pub(super) controller: AsyncController<MainBar>,
}

pub(super) fn available_monitors() -> Vec<gtk::gdk::Monitor> {
    let Some(display) = gtk::gdk::Display::default() else {
        return Vec::new();
    };
    let monitors = display.monitors();
    (0..monitors.n_items())
        .filter_map(|index| monitors.item(index))
        .filter_map(|item| item.downcast::<gtk::gdk::Monitor>().ok())
        .collect()
}

pub(super) fn launch_workspace_bar(
    title: &'static str,
    monitor: Option<gtk::gdk::Monitor>,
    output_name: Option<String>,
) -> Option<Controller<WorkspaceBar>> {
    let builder = WorkspaceBar::builder();
    let root = builder.root.clone();
    relm4::main_application().add_window(&root);
    let controller = builder
        .launch(WorkspaceBarInit {
            title,
            monitor,
            output_name,
        })
        .detach();
    root.present();
    Some(controller)
}

pub(super) fn watch_monitors(sender: relm4::Sender<MainBarInput>) -> Option<gtk::gio::ListModel> {
    let display = gtk::gdk::Display::default()?;
    let monitors = display.monitors();
    monitors.connect_items_changed(move |_, _, _, _| {
        eprintln!("[bar] monitor list changed");
        sender.emit(MainBarInput::MonitorsChanged);
    });
    Some(monitors)
}

pub(super) fn launch_secondary_bars(
    title: &'static str,
    monitors: impl Iterator<Item = gtk::gdk::Monitor>,
) -> Vec<SecondaryBar> {
    monitors
        .filter_map(|monitor| {
            monitor_output_name(Some(&monitor)).map(|_| launch_secondary_bar(title, monitor))
        })
        .collect()
}

pub(super) fn launch_secondary_bar(
    title: &'static str,
    monitor: gtk::gdk::Monitor,
) -> SecondaryBar {
    let output_name =
        monitor_output_name(Some(&monitor)).expect("secondary bars require a monitor connector");
    let builder = MainBar::builder();
    let root = builder.root.clone();
    relm4::main_application().add_window(&root);
    let controller = builder
        .launch(MainBarInit::secondary(title, monitor))
        .detach();
    root.present();
    SecondaryBar {
        output_name,
        controller,
    }
}

pub(super) fn monitor_output_name(monitor: Option<&gtk::gdk::Monitor>) -> Option<String> {
    monitor
        .and_then(|monitor| monitor.connector())
        .and_then(|connector| non_empty_string(connector.as_str()))
}

pub(super) fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

pub(super) fn log_bar_monitor(
    primary: bool,
    monitor: Option<&gtk::gdk::Monitor>,
    output_name: Option<&str>,
) {
    let role = if primary { "primary" } else { "secondary" };
    let connector = monitor.and_then(|monitor| monitor.connector());
    let geometry = monitor.map(|monitor| monitor.geometry());
    eprintln!(
        "[bar] launching {role} bar: gtk_connector={:?} geometry={:?} workspace_output_filter={:?}",
        connector.as_deref(),
        geometry,
        output_name
    );
}

pub(super) fn bar_window_config() -> WindowConfig {
    WindowConfig::new(Layer::Top)
        .with_anchors(
            Anchors::NONE
                .with_edge(Edge::Bottom)
                .with_edge(Edge::Right)
                .with_edge(Edge::Left),
        )
        .with_auto_exclusive_zone()
        .with_namespace("rsynapse-bar")
}
