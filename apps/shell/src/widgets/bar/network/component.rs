use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use super::{NetworkView, network_status};
use crate::widgets::bar::bar_item;

#[derive(Debug)]
pub(crate) enum NetworkIndicatorsInput {
    Source(network_indicators_sources::Msg),
}

impl From<network_indicators_sources::Msg> for NetworkIndicatorsInput {
    fn from(message: network_indicators_sources::Msg) -> Self {
        Self::Source(message)
    }
}

#[derive(Debug)]
#[shell_macros::model(module = network_indicators_sources)]
pub(crate) struct NetworkIndicators {
    #[source(network_status())]
    view: NetworkView,
}

#[shell_macros::component(
    module = network_indicators_sources,
    model = NetworkIndicators
)]
#[relm4::component(pub(crate))]
impl SimpleComponent for NetworkIndicators {
    type Init = ();
    type Input = NetworkIndicatorsInput;
    type Output = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Center,

            gtk::Box {
                set_css_classes: &[
                    bar_item::ITEM_CLASS,
                    bar_item::SQUARE_CLASS,
                    "network-item",
                    "ethernet-item",
                ],
                #[watch]
                set_visible: model.view.ethernet.visible,
                #[watch]
                set_tooltip_text: Some(model.view.ethernet.tooltip.as_str()),

                gtk::Image {
                    set_css_classes: &[
                        bar_item::ICON_CLASS,
                        "network-icon",
                        "ethernet-icon",
                    ],
                    set_halign: gtk::Align::Center,
                    set_hexpand: true,
                    #[watch]
                    set_icon_name: Some(model.view.ethernet.icon.as_str()),
                }
            },

            gtk::Box {
                set_css_classes: &[
                    bar_item::ITEM_CLASS,
                    bar_item::SQUARE_CLASS,
                    "network-item",
                    "wifi-item",
                ],
                #[watch]
                set_visible: model.view.wifi.visible,
                #[watch]
                set_tooltip_text: Some(model.view.wifi.tooltip.as_str()),

                gtk::Image {
                    set_css_classes: &[
                        bar_item::ICON_CLASS,
                        "network-icon",
                        "wifi-icon",
                    ],
                    set_halign: gtk::Align::Center,
                    set_hexpand: true,
                    #[watch]
                    set_icon_name: Some(model.view.wifi.icon.as_str()),
                }
            }
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = NetworkIndicators::new();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            NetworkIndicatorsInput::Source(message) => {
                NetworkIndicators::update(self, message);
            }
        }
    }
}
