use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    window::{self, Anchors, Edge, Layer, WindowConfig},
};
use std::time::Duration;

use crate::widgets::BACKGROUND_BLUR_CLASS;

const OSD_SHELL_CLASS: &str = "osd-shell";
const OSD_BACKGROUND_BLUR_CLASSES: &[&str] = &[BACKGROUND_BLUR_CLASS];
const OSD_BACKGROUND_BLUR_RADIUS: i32 = 24;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OsdInit {
    pub title: &'static str,
}

pub struct OsdWindow {
    visible: bool,
    revealed: bool,
    icon: String,
    value: f64,
    generation: u64,
}

#[derive(Debug)]
pub enum OsdInput {
    ShowAudio(OsdAudioView),
    ShowBrightness(OsdBrightnessView),
    Hide(u64),
    HideWindow(u64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OsdAudioView {
    pub(crate) icon: String,
    pub(crate) percent: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OsdBrightnessView {
    pub(crate) icon: String,
    pub(crate) percent: u8,
}

#[relm4::component(pub, async)]
impl SimpleAsyncComponent for OsdWindow {
    type Init = OsdInit;
    type Input = OsdInput;
    type Output = ();

    view! {
        #[root]
        gtk::Window {
            add_css_class: "OSD",
            #[watch]
            set_visible: model.visible,

            gtk::Box {
                add_css_class: BACKGROUND_BLUR_CLASS,
                add_css_class: OSD_SHELL_CLASS,
                set_orientation: gtk::Orientation::Vertical,

                gtk::Revealer {
                    #[watch]
                    set_reveal_child: model.revealed,
                    set_transition_type: gtk::RevealerTransitionType::Crossfade,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,

                        gtk::Image {
                            add_css_class: "materialicon",
                            #[watch]
                            set_icon_name: Some(model.icon.as_str()),
                            set_icon_size: gtk::IconSize::Large,
                        },

                        gtk::LevelBar {
                            set_valign: gtk::Align::Center,
                            set_width_request: 100,
                            set_min_value: 0.0,
                            set_max_value: 1.0,
                            #[watch]
                            set_value: model.value,
                        }
                    }
                }
            }
        }
    }

    async fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        window::apply_layer_shell_config(&root, osd_window_config());
        root.set_title(Some(init.title));

        let model = OsdWindow {
            visible: false,
            revealed: false,
            icon: "audio-volume-medium-symbolic".to_owned(),
            value: 0.0,
            generation: 0,
        };
        let widgets = view_output!();
        AsyncComponentParts { model, widgets }
    }

    async fn update(&mut self, msg: Self::Input, sender: AsyncComponentSender<Self>) {
        match msg {
            OsdInput::ShowAudio(view) => self.show_level(view.icon, view.percent, &sender),
            OsdInput::ShowBrightness(view) => self.show_level(view.icon, view.percent, &sender),
            OsdInput::Hide(generation) if generation == self.generation => {
                self.revealed = false;

                let input = sender.input_sender().clone();
                relm4::spawn_local(async move {
                    gtk::glib::timeout_future(Duration::from_millis(100)).await;
                    input.emit(OsdInput::HideWindow(generation));
                });
            }
            OsdInput::Hide(_) => {}
            OsdInput::HideWindow(generation) if generation == self.generation => {
                self.visible = false;
            }
            OsdInput::HideWindow(_) => {}
        }
    }
}

impl OsdWindow {
    fn show_level(&mut self, icon: String, percent: u8, sender: &AsyncComponentSender<Self>) {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.visible = true;
        self.revealed = true;
        self.icon = icon;
        self.value = f64::from(percent.min(100)) / 100.0;

        let input = sender.input_sender().clone();
        relm4::spawn_local(async move {
            gtk::glib::timeout_future_seconds(1).await;
            input.emit(OsdInput::Hide(generation));
        });
    }
}

const fn osd_window_config() -> WindowConfig {
    WindowConfig::new(Layer::Overlay)
        .with_anchors(Anchors::NONE.with_edge(Edge::Bottom))
        .with_rounded_background_blur_for_css_classes(
            OSD_BACKGROUND_BLUR_CLASSES,
            OSD_BACKGROUND_BLUR_RADIUS,
        )
        .with_namespace("rsynapse-osd")
}
