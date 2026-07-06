use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    list::{ComponentListBoxExt, ComponentListUpdate},
};

use crate::{
    niri::{self, NiriWindow},
    row::WindowTileRow,
};

pub(crate) struct WindowTiles {
    vm: Vec<NiriWindow>,
    __shell: sources::Runtime,
}

impl WindowTiles {
    fn new() -> Self {
        Self {
            vm: Vec::new(),
            __shell: sources::Runtime::default(),
        }
    }
}

#[shell_macros::view_model(model = Vec<NiriWindow>, source = niri::windows().distinct_until_changed())]
#[relm4::component(pub(crate) async)]
impl SimpleAsyncComponent for WindowTiles {
    type Init = ();
    type Output = ();

    view! {
        #[root]
        gtk::Window {
            set_title: Some("Window Tiles"),
            set_default_size: (360, 96),

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 6,
                set_margin_top: 12,
                set_margin_bottom: 12,
                set_margin_start: 12,
                set_margin_end: 12,

                #[watch]
                set_component_list: ComponentListUpdate::<WindowTileRow>::new(&model.vm),
            }
        }
    }

    async fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let model = WindowTiles::new();
        let widgets = view_output!();
        AsyncComponentParts { model, widgets }
    }
}
