use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    list::ComponentListBoxExt,
};

use super::{AudioRouteRow, AudioRouteView, audio_routes};

#[shell_macros::model]
pub(crate) struct AudioRoutePopover {
    #[source(audio_routes())]
    routes: Vec<AudioRouteView>,
}

#[shell_macros::component(model = AudioRoutePopover)]
#[relm4::component(pub(crate))]
impl SimpleComponent for AudioRoutePopover {
    type Init = ();
    type Input = sources::Msg;
    type Output = ();

    view! {
        #[root]
        gtk::Box {
            add_css_class: "audio-route",
            set_orientation: gtk::Orientation::Vertical,

            gtk::Label {
                add_css_class: "audio-route-heading",
                set_halign: gtk::Align::Start,
                set_label: "Audio Output",
            },

            #[bind_list(routes, row = AudioRouteRow)]
            routes -> gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
            }
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = AudioRoutePopover::new();
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }
}
