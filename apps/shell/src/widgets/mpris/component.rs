use relm4::prelude::*;
use shell_core::{
    gtk::{self, gdk, prelude::*},
    keybind::{HoldAction, HoldBindings, HoldWatcher},
    window::{self, Anchors, Edge, Layer, WindowConfig},
};

use super::{
    MediaAction, MprisInput, MprisWindow,
    helpers::{art_file, artist_text, format_duration, play_pause_icon, title_text},
    sone::SoneLikeState,
};
use crate::widgets::nerd_icon::{NerdIcon, NerdIconLabelExt};

#[shell_macros::component(model = MprisWindow)]
#[relm4::component(pub(crate))]
impl SimpleComponent for MprisWindow {
    type Init = data_sources::StateSignal<bool>;
    type Input = MprisInput;
    type Output = ();

    view! {
        #[root]
        gtk::Window {
            add_css_class: "mpris-window",
            set_title: Some("Media"),
            set_decorated: false,
            set_hide_on_close: true,
            #[watch]
            set_visible: model.visible,

            gtk::Box {
                add_css_class: "mpris-frame",
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::End,

                gtk::Revealer {
                    set_transition_type: gtk::RevealerTransitionType::SlideUp,
                    set_transition_duration: 180,
                    #[watch]
                    set_reveal_child: model.revealed,

                gtk::Box {
                    add_css_class: "mpris-overlay",
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 16,
                    set_width_request: 440,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 18,

                        gtk::AspectFrame {
                            add_css_class: "mpris-art-frame",
                            set_valign: gtk::Align::Center,
                            set_obey_child: false,
                            set_ratio: 1.0,
                            set_size_request: (160, 160),
                            set_overflow: gtk::Overflow::Hidden,

                            #[wrap(Some)]
                            set_child = &gtk::Overlay {
                                gtk::Box {
                                    set_halign: gtk::Align::Center,
                                    set_valign: gtk::Align::Center,
                                    set_hexpand: true,
                                    set_vexpand: true,

                                    gtk::Image {
                                        add_css_class: "mpris-art-fallback",
                                        set_icon_name: Some("audio-x-generic-symbolic"),
                                        set_pixel_size: 64,
                                    }
                                },

                                add_overlay = &gtk::Picture {
                                    add_css_class: "mpris-art",
                                    set_can_shrink: true,
                                    set_content_fit: gtk::ContentFit::Cover,
                                    set_hexpand: true,
                                    set_vexpand: true,
                                    #[watch]
                                    set_file: art_file(&model.player).as_ref(),
                                }
                            }
                        },

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 4,
                            set_valign: gtk::Align::Center,
                            set_hexpand: true,

                            gtk::Label {
                                add_css_class: "mpris-title",
                                set_xalign: 0.0,
                                set_ellipsize: gtk::pango::EllipsizeMode::End,
                                set_max_width_chars: 32,
                                #[watch]
                                set_label: title_text(&model.player).as_str(),
                            },

                            gtk::Label {
                                add_css_class: "mpris-artist",
                                set_xalign: 0.0,
                                set_ellipsize: gtk::pango::EllipsizeMode::End,
                                set_max_width_chars: 32,
                                #[watch]
                                set_label: artist_text(&model.player).as_str(),
                            },

                            gtk::Label {
                                add_css_class: "mpris-album",
                                set_xalign: 0.0,
                                set_ellipsize: gtk::pango::EllipsizeMode::End,
                                set_max_width_chars: 32,
                                #[watch]
                                set_visible: !model.player.album.is_empty(),
                                #[watch]
                                set_label: model.player.album.as_str(),
                            },

                            gtk::Label {
                                add_css_class: "mpris-player",
                                set_xalign: 0.0,
                                set_ellipsize: gtk::pango::EllipsizeMode::End,
                                set_max_width_chars: 32,
                                #[watch]
                                set_visible: model.player.is_available(),
                                #[watch]
                                set_label: model.player.identity.as_str(),
                            },

                            gtk::Button {
                                add_css_class: "mpris-like",
                                set_halign: gtk::Align::Start,
                                set_tooltip_text: Some("Toggle like (L)"),
                                #[watch]
                                set_visible: model.sone_like.visible(),
                                #[watch]
                                set_sensitive: model.sone_like.can_toggle() && !model.sone_busy,
                                connect_clicked => MprisInput::ToggleLike,

                                gtk::Label {
                                    add_css_class: "mpris-like-icon",
                                    #[watch]
                                    set_nerd_icon: NerdIcon::new(model.sone_like.icon()),
                                }
                            }
                        }
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,

                        gtk::ProgressBar {
                            add_css_class: "mpris-progress",
                            set_show_text: false,
                            #[watch]
                            set_fraction: model.player.progress(),
                        },

                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,

                            gtk::Label {
                                add_css_class: "mpris-time",
                                set_xalign: 0.0,
                                set_hexpand: true,
                                #[watch]
                                set_label: format_duration(model.player.position_us).as_str(),
                            },

                            gtk::Label {
                                add_css_class: "mpris-time",
                                set_xalign: 1.0,
                                #[watch]
                                set_label: format_duration(model.player.length_us).as_str(),
                            }
                        }
                    },

                    gtk::Box {
                        add_css_class: "mpris-controls",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 18,
                        set_halign: gtk::Align::Center,

                        gtk::Button {
                            add_css_class: "mpris-control",
                            set_icon_name: "media-skip-backward-symbolic",
                            set_tooltip_text: Some("Previous"),
                            #[watch]
                            set_sensitive: model.player.can_go_previous,
                            connect_clicked => MprisInput::Control(MediaAction::Previous),
                        },

                        gtk::Button {
                            add_css_class: "mpris-control",
                            add_css_class: "mpris-play-pause",
                            set_tooltip_text: Some("Play/Pause"),
                            #[watch]
                            set_sensitive: model.player.can_play_pause,
                            connect_clicked => MprisInput::Control(MediaAction::PlayPause),

                            gtk::Image {
                                #[watch]
                                set_icon_name: Some(play_pause_icon(&model.player)),
                            }
                        },

                        gtk::Button {
                            add_css_class: "mpris-control",
                            set_icon_name: "media-skip-forward-symbolic",
                            set_tooltip_text: Some("Next"),
                            #[watch]
                            set_sensitive: model.player.can_go_next,
                            connect_clicked => MprisInput::Control(MediaAction::Next),
                        }
                    },
                }
                }
            }
        }
    }

    fn init(
        active: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        window::apply_layer_shell_config(
            &root,
            WindowConfig::new(Layer::Overlay)
                .with_anchors(
                    Anchors::NONE
                        .with_edge(Edge::Bottom)
                        .with_edge(Edge::Left)
                        .with_edge(Edge::Right),
                )
                .with_namespace("rsynapse-mpris"),
        );
        root.set_decorated(false);

        let bindings = HoldBindings {
            next: Vec::new(),
            previous: Vec::new(),
            activate: Vec::new(),
            cancel: Vec::new(),
            ..HoldBindings::default()
        };
        let hold_sender = sender.clone();
        let hold_watcher = HoldWatcher::install(&root, bindings, move |action| {
            if action == HoldAction::ModifierReleased {
                hold_sender.input(MprisInput::Hide);
            }
        });

        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let key_sender = sender.clone();
        keys.connect_key_pressed(move |_, key, _, _| match key {
            gdk::Key::Escape => {
                key_sender.input(MprisInput::Hide);
                gtk::glib::Propagation::Stop
            }
            gdk::Key::m => {
                key_sender.input(MprisInput::Control(MediaAction::PlayPause));
                gtk::glib::Propagation::Stop
            }
            gdk::Key::l | gdk::Key::L => {
                key_sender.input(MprisInput::ToggleLike);
                gtk::glib::Propagation::Stop
            }
            gdk::Key::Left => {
                key_sender.input(MprisInput::Control(MediaAction::Previous));
                gtk::glib::Propagation::Stop
            }
            gdk::Key::Right => {
                key_sender.input(MprisInput::Control(MediaAction::Next));
                gtk::glib::Propagation::Stop
            }
            _ => gtk::glib::Propagation::Proceed,
        });
        root.add_controller(keys);

        let close_sender = sender.clone();
        root.connect_close_request(move |_| {
            close_sender.input(MprisInput::Hide);
            gtk::glib::Propagation::Stop
        });

        let model = Self::new(
            root.clone(),
            false,
            false,
            false,
            0,
            hold_watcher,
            active,
            None,
            SoneLikeState::Hidden,
            false,
        );
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        self.handle_input(message, sender)
    }
}
