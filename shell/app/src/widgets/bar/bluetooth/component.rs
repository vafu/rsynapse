use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use super::{BluetoothView, bluetooth_status, status_count, status_tooltip, toggle_power};
use crate::widgets::bar::bar_item;
use crate::widgets::nerd_icon::NerdIconLabelExt;

#[derive(Debug)]
pub(crate) enum BluetoothIndicatorInput {
    Source(bluetooth_indicator_sources::Msg),
    Toggle,
}

impl From<bluetooth_indicator_sources::Msg> for BluetoothIndicatorInput {
    fn from(message: bluetooth_indicator_sources::Msg) -> Self {
        Self::Source(message)
    }
}

#[derive(Debug)]
#[shell_macros::model(module = bluetooth_indicator_sources)]
pub(crate) struct BluetoothIndicator {
    #[source(bluetooth_status())]
    view: BluetoothView,
}

#[shell_macros::component(
    module = bluetooth_indicator_sources,
    model = BluetoothIndicator
)]
#[relm4::component(pub(crate))]
impl SimpleComponent for BluetoothIndicator {
    type Init = ();
    type Input = BluetoothIndicatorInput;
    type Output = ();

    view! {
        #[root]
        gtk::Box {
            set_css_classes: &bar_item::classes(&[
                bar_item::SQUARE_CLASS,
                "bt-power-item",
            ]),

            gtk::Button {
                set_css_classes: &bar_item::action_classes(&["bt-power-button"]),
                connect_clicked => BluetoothIndicatorInput::Toggle,
                #[watch]
                set_tooltip_text: Some(status_tooltip(&model.view.status).as_str()),

                gtk::Overlay {
                    gtk::Label {
                        set_css_classes: &["nerdicon", bar_item::ICON_CLASS],
                        set_halign: gtk::Align::Center,
                        #[watch]
                        set_nerd_icon: model.view.status.icon.clone(),
                    },

                    add_overlay = &gtk::Label {
                        add_css_class: "bt-count",
                        #[watch]
                        set_visible: model.view.status.connected_count > 0,
                        #[watch]
                        set_label: status_count(&model.view.status).as_str(),
                    }
                }
            }
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = BluetoothIndicator::new();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            BluetoothIndicatorInput::Source(message) => {
                BluetoothIndicator::update(self, message);
            }
            BluetoothIndicatorInput::Toggle => toggle_power(&self.view.status),
        }
    }
}
