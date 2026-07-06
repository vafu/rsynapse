use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use super::{AudioRouteView, source};
use crate::widgets::material_icon;

#[derive(Debug)]
pub(in crate::widgets::bar) struct AudioRouteRow {
    route: AudioRouteView,
}

#[relm4::component(pub(crate))]
impl SimpleComponent for AudioRouteRow {
    type Init = AudioRouteView;
    type Input = ();
    type Output = ();

    view! {
        #[root]
        gtk::Button {
            add_css_class: "flat",
            add_css_class: "audio-route-row",
            set_tooltip_text: Some(model.route.subtitle.as_str()),

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                gtk::Image {
                    add_css_class: "materialicon",
                    set_icon_name: Some(material_icon::icon_name(model.route.icon.as_str()).as_str()),
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_hexpand: true,

                    gtk::Label {
                        add_css_class: "audio-route-title",
                        set_halign: gtk::Align::Start,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_label: model.route.title.as_str(),
                    },

                    gtk::Label {
                        add_css_class: "audio-route-subtitle",
                        set_halign: gtk::Align::Start,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_label: model.route.subtitle.as_str(),
                    }
                },

                gtk::Image {
                    add_css_class: "materialicon",
                    set_visible: model.route.is_default,
                    set_icon_name: Some(material_icon::icon_name("check").as_str()),
                }
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = AudioRouteRow { route: init };
        let widgets = view_output!();
        let route_id = model.route.id;
        let root_button = root.clone();

        root.connect_clicked(move |_| {
            if let Some(popover) = root_button
                .ancestor(gtk::Popover::static_type())
                .and_then(|widget| widget.downcast::<gtk::Popover>().ok())
            {
                popover.popdown();
            }

            source::set_default_route(route_id);
        });

        ComponentParts { model, widgets }
    }
}
