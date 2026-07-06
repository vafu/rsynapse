use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    source::{self, Observable},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrayItemNode;

pub(crate) fn systray_items() -> Observable<Vec<TrayItemNode>> {
    source::once(Vec::new())
}

pub(crate) struct TrayItem;

#[relm4::component(pub(crate))]
impl SimpleComponent for TrayItem {
    type Init = TrayItemNode;
    type Input = ();
    type Output = ();

    view! {
        #[root]
        gtk::Box {
            set_visible: false,
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = TrayItem;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }
}
