use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use crate::{model::WindowTile, niri::NiriWindow};

pub(crate) struct WindowTileRow {
    window: NiriWindow,
    vm: WindowTile,
    __shell: sources::Runtime,
}

impl WindowTileRow {
    fn new(window: NiriWindow) -> Self {
        Self {
            window,
            vm: WindowTile::default(),
            __shell: sources::Runtime::default(),
        }
    }
}

#[shell_macros::view_model(model = WindowTile, source = WindowTile::source(model.window.clone()))]
#[relm4::component(pub(crate))]
impl SimpleComponent for WindowTileRow {
    type Init = NiriWindow;
    type Output = ();

    view! {
        #[root]
        gtk::Button {
            set_width_request: 42,
            set_height_request: 42,
            #[watch]
            set_css_classes: &model.vm.classes(),
            #[watch]
            set_tooltip_text: Some(model.vm.title()),

            gtk::Image {
                set_pixel_size: 24,
                #[watch]
                set_icon_name: Some(model.vm.icon_name()),
            }
        }
    }

    fn init(
        window: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = WindowTileRow::new(window);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }
}
