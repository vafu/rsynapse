use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    gtk4_layer_shell::LayerShell,
    list::ComponentListBoxExt,
    window::{self, Anchors, Edge, Layer, WindowConfig},
};

use super::{
    WORKSPACE_RAIL_WIDTH, WorkspaceNode, context_zone::ContextZone, project_label::ProjectLabel,
    workspaces::workspaces,
};

#[derive(Clone)]
pub(super) struct WorkspaceBarInit {
    pub(super) title: &'static str,
    pub(super) monitor: Option<gtk::gdk::Monitor>,
    pub(super) output_name: Option<String>,
    pub(super) primary_output_name: Option<String>,
}

#[shell_macros::model(module = workspace_bar_sources)]
pub(super) struct WorkspaceBar {
    _context_zone: Controller<ContextZone>,
    output_name: Option<String>,
    primary_output_name: Option<String>,

    #[source(workspaces(output_name.clone(), primary_output_name.clone()))]
    project_labels: Vec<WorkspaceNode>,
}

#[shell_macros::component(
    module = workspace_bar_sources,
    model = WorkspaceBar
)]
#[relm4::component(pub(crate))]
impl SimpleComponent for WorkspaceBar {
    type Init = WorkspaceBarInit;
    type Input = workspace_bar_sources::Msg;
    type Output = ();

    view! {
        #[root]
        gtk::Window {
            add_css_class: "workspace-bar-window",
            set_width_request: WORKSPACE_RAIL_WIDTH,

            gtk::CenterBox {
                set_widget_name: "rsynapse-workspace-rail",
                add_css_class: "bar",
                set_width_request: WORKSPACE_RAIL_WIDTH,
                set_orientation: gtk::Orientation::Vertical,
                set_vexpand: true,

                #[wrap(Some)]
                set_start_widget = &gtk::Box {
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Start,
                    set_orientation: gtk::Orientation::Vertical,

                    #[local_ref]
                    context_zone_root -> gtk::Revealer {},
                },

                #[wrap(Some)]
                set_end_widget = &gtk::Box {
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::End,
                    set_orientation: gtk::Orientation::Vertical,

                    #[bind_list(project_labels, row = ProjectLabel)]
                    project_labels -> gtk::Box {
                        set_widget_name: "workspace-rail-list",
                        add_css_class: "bar-indicator-list-vertical",
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::End,
                        set_orientation: gtk::Orientation::Vertical,
                    },
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        window::apply_layer_shell_config(&root, workspace_bar_window_config());
        if let Some(monitor) = init.monitor.as_ref() {
            root.set_monitor(Some(monitor));
        }
        root.set_title(Some(init.title));
        log_workspace_bar_monitor(init.monitor.as_ref(), init.output_name.as_deref());

        let context_zone_builder = ContextZone::builder();
        let context_zone_root = context_zone_builder.root.clone();
        let context_zone = context_zone_builder.launch(()).detach();
        let model = WorkspaceBar::new(context_zone, init.output_name, init.primary_output_name);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }
}

fn workspace_bar_window_config() -> WindowConfig {
    WindowConfig::new(Layer::Top)
        .with_anchors(
            Anchors::NONE
                .with_edge(Edge::Top)
                .with_edge(Edge::Bottom)
                .with_edge(Edge::Left),
        )
        .with_auto_exclusive_zone()
        .with_namespace("rsynapse-workspace-rail")
}

fn log_workspace_bar_monitor(monitor: Option<&gtk::gdk::Monitor>, output_name: Option<&str>) {
    let connector = monitor.and_then(|monitor| monitor.connector());
    let geometry = monitor.map(|monitor| monitor.geometry());
    eprintln!(
        "[bar] launching workspace rail: gtk_connector={:?} geometry={:?} niri_output_filter={:?}",
        connector.as_deref(),
        geometry,
        output_name
    );
}
