use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    list::{ComponentListBoxExt, ComponentListUpdate},
};

use super::{WindowNode, window_tile::WindowTile};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WindowColumnNode {
    pub(super) column: u64,
    pub(super) windows: Vec<WindowNode>,
}

impl WindowColumnNode {
    pub(super) fn new(column: u64, windows: Vec<WindowNode>) -> Self {
        Self { column, windows }
    }
}

#[derive(Debug)]
pub(super) struct WindowColumn {
    node: WindowColumnNode,
}

#[relm4::component(pub(crate))]
impl SimpleComponent for WindowColumn {
    type Init = WindowColumnNode;
    type Input = ();
    type Output = ();

    view! {
        #[root]
        gtk::Box {
            set_css_classes: &window_column_classes(&model.node),
            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Fill,
            set_vexpand: true,
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 3,

            #[name = "window_tiles"]
            gtk::Box {
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Fill,
                set_vexpand: true,
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 3,
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = WindowColumn { node: init };
        let widgets = view_output!();
        widgets
            .window_tiles
            .set_component_list(ComponentListUpdate::<WindowTile>::new(&model.node.windows));

        ComponentParts { model, widgets }
    }
}

fn window_column_classes(column: &WindowColumnNode) -> Vec<&'static str> {
    let mut classes = vec!["workspace-window-column"];
    if column.windows.len() > 1 {
        classes.push("stacked");
    } else {
        classes.push("single");
    }
    classes
}
