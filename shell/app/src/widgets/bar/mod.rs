mod audio;
mod battery;
mod bluetooth;
mod brightness;
mod bzbus;
mod mpris;
mod network;
mod niri;
mod power_profile;
mod project_label;
mod source_errors;
mod system_stats;
mod systray;
mod time;
mod window_source;
mod window_tile;
mod workspaces;

use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    thread,
};

use gtk4_background_effect::BackgroundEffectRegion;
use relm4::component::ComponentController;
use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    gtk4_layer_shell::LayerShell,
    list::ComponentListBoxExt,
    source::SourceError,
    window::{self, Anchors, Edge, Layer, WindowConfig},
};

use crate::widgets::{BACKGROUND_BLUR_CLASS, level_indicator, material_icon};

use self::audio::{AudioRoutePopover, AudioView, audio_status};
use self::battery::BatteryView;
use self::battery::battery_status;
use self::bluetooth::{
    BluetoothDeviceGroup, BluetoothGroupPopover, BluetoothView, bluetooth_status,
};
use self::brightness::{BrightnessView, brightness_status};
use self::bzbus::{BzBusView, bzbus_status};
use self::mpris::{MprisView, mpris_status};
use self::network::{NetworkView, network_status};
use self::power_profile::{PowerProfileView, power_profile_status};
use self::project_label::ProjectLabel;
use self::source_errors::{SourceErrorRow, source_error_count, source_error_items};
use self::system_stats::{ArcSide, SysStatsView, sys_stats};
use self::systray::{TrayItem, systray_items};
use self::time::{ClockView, clock};
use self::window_tile::WindowTile;
use self::workspaces::{WorkspaceNode, selected_workspace_windows, workspaces};
use super::{
    OsdAudioView, OsdBrightnessView, OsdInit, OsdInput, OsdWindow, has_notification_items,
};
use crate::{hints, request, theme};

type WindowNode = niri::NiriWindow;

const BAR_BACKGROUND_BLUR_CLASSES: &[&str] = &[BACKGROUND_BLUR_CLASS];
const BAR_BACKGROUND_BLUR_RADIUS: i32 = 12;

#[derive(Clone)]
pub struct MainBarInit {
    pub title: &'static str,
    monitor: Option<gtk::gdk::Monitor>,
    output_name: Option<String>,
    primary: bool,
}

impl MainBarInit {
    pub fn primary(title: &'static str) -> Self {
        Self {
            title,
            monitor: None,
            output_name: None,
            primary: true,
        }
    }

    fn secondary(title: &'static str, monitor: gtk::gdk::Monitor) -> Self {
        Self {
            title,
            output_name: monitor_output_name(Some(&monitor)),
            monitor: Some(monitor),
            primary: false,
        }
    }
}

impl std::fmt::Debug for MainBarInit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MainBarInit")
            .field("title", &self.title)
            .field("output_name", &self.output_name)
            .field("primary", &self.primary)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum MainBarInput {
    Source(sources::Msg),
    Media(MediaAction),
    ToggleBluetooth,
    CyclePowerProfile,
    ToggleNotificationCenter,
    Request(request::PendingRequest),
}

impl From<sources::Msg> for MainBarInput {
    fn from(msg: sources::Msg) -> Self {
        Self::Source(msg)
    }
}

#[derive(Debug)]
pub enum MediaAction {
    Previous,
    PlayPause,
    Next,
}

#[shell_macros::model]
pub struct MainBar {
    _osd: Option<AsyncController<OsdWindow>>,
    _request_server: Option<request::RequestServer>,
    _child_bars: Vec<AsyncController<MainBar>>,
    _audio_osd_ready: bool,
    _brightness_osd_ready: bool,
    output_name: Option<String>,

    #[source(workspaces(output_name.clone()))]
    project_labels: Vec<WorkspaceNode>,

    #[source(selected_workspace_windows(output_name.clone()))]
    window_tiles: Vec<WindowNode>,

    #[source(bzbus_status())]
    bzbus: BzBusView,

    #[source(battery_status())]
    battery: BatteryView,

    #[source(network_status())]
    network: NetworkView,

    #[source(power_profile_status())]
    power_profile: PowerProfileView,

    #[source(audio_status())]
    audio: AudioView,

    #[source(brightness_status())]
    brightness: BrightnessView,

    #[source(mpris_status())]
    mpris: MprisView,

    #[source(bluetooth_status())]
    bluetooth: BluetoothView,

    #[source(systray_items())]
    tray_items: Vec<systray::TrayItemNode>,

    #[source(sys_stats())]
    system_stats: SysStatsView,

    #[source(clock())]
    clock: ClockView,

    #[source(has_notification_items())]
    has_notifications: bool,

    #[source(source_error_count())]
    source_error_count: u64,

    #[source(source_error_items())]
    source_error_items: Vec<SourceError>,
}

#[shell_macros::component(model = MainBar)]
#[relm4::component(pub, async)]
impl SimpleAsyncComponent for MainBar {
    type Init = MainBarInit;
    type Input = MainBarInput;
    type Output = ();

    view! {
        #[root]
        gtk::Window {
            add_css_class: "bar-window",

            gtk::CenterBox {
                set_widget_name: "rsynapse-bar",
                add_css_class: "bar",
                set_orientation: gtk::Orientation::Horizontal,

                #[wrap(Some)]
                set_start_widget = &gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,

                    #[bind_list(project_labels, row = ProjectLabel)]
                    project_labels -> gtk::Box {
                        set_widget_name: "project-labels",
                        add_css_class: "projects-widget",
                        add_css_class: "workspaces-widget",
                        add_css_class: "projects-list",
                        add_css_class: "workspaces-list",
                        set_halign: gtk::Align::Center,
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 4,
                    },

                    gtk::Overlay {
                        add_css_class: "bzbus-progress-frame",
                        #[watch]
                        set_tooltip_text: Some(model.bzbus.tooltip.as_str()),

                        gtk::Box {
                            #[watch]
                            set_css_classes: &model.bzbus.classes,
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 4,

                            gtk::Image {
                                add_css_class: "materialicon",
                                #[watch]
                                set_icon_name: Some(material_icon::icon_name(model.bzbus.icon).as_str()),
                            }
                        },

                        add_overlay = &gtk::DrawingArea {
                            #[watch]
                            set_visible: model.bzbus.progress_visible,
                            set_css_classes: bzbus::progress_track_classes(),
                            set_halign: gtk::Align::Fill,
                            set_valign: gtk::Align::Fill,
                            set_hexpand: true,
                            set_vexpand: true,
                            set_can_target: false,
                            set_draw_func: bzbus::progress_track_draw_func(),
                        },

                        add_overlay = &gtk::DrawingArea {
                            #[watch]
                            set_visible: model.bzbus.progress_visible,
                            #[watch]
                            set_css_classes: &model.bzbus.progress_level_classes,
                            set_halign: gtk::Align::Fill,
                            set_valign: gtk::Align::Fill,
                            set_hexpand: true,
                            set_vexpand: true,
                            set_can_target: false,
                            #[watch]
                            set_draw_func: bzbus::progress_level_draw_func(model.bzbus.progress_percent),
                        }
                    }
                },

                #[wrap(Some)]
                set_center_widget = &gtk::Box {
                    set_halign: gtk::Align::Center,
                    set_orientation: gtk::Orientation::Horizontal,

                    #[bind_list(window_tiles, row = WindowTile)]
                    window_tiles -> gtk::Box {
                        set_widget_name: "workspace-window-list",
                        add_css_class: "workspace-window-list",
                        set_halign: gtk::Align::Center,
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 4,
                        set_valign: gtk::Align::Fill,
                        set_vexpand: true,
                    }
                },

                #[wrap(Some)]
                set_end_widget = &gtk::Box {
                    add_css_class: "system-cluster",
                    set_halign: gtk::Align::End,
                    set_orientation: gtk::Orientation::Horizontal,

                    #[name = "mpris_group"]
                    gtk::Box {
                        #[watch]
                        set_css_classes: &mpris_classes(&model.mpris),
                        #[watch]
                        set_visible: model.mpris.visible,
                        #[watch]
                        set_tooltip_text: Some(model.mpris.tooltip.as_str()),
                        set_halign: gtk::Align::End,
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 4,

                        #[name = "mpris_details_revealer"]
                        gtk::Revealer {
                            set_reveal_child: false,
                            set_transition_type: gtk::RevealerTransitionType::SlideRight,

                            gtk::Label {
                                add_css_class: "mpris-label",
                                set_ellipsize: gtk::pango::EllipsizeMode::End,
                                set_max_width_chars: 30,
                                #[watch]
                                set_label: model.mpris.metadata.as_str(),
                            }
                        },

                        #[name = "mpris_previous_button"]
                        gtk::Button {
                            add_css_class: "flat",
                            add_css_class: "circular",
                            add_css_class: "mpris-control",
                            #[watch]
                            set_sensitive: model.mpris.can_go_previous,

                            gtk::Image {
                                add_css_class: "materialicon",
                                set_icon_name: Some(material_icon::icon_name("skip_previous").as_str()),
                            }
                        },

                        #[name = "mpris_play_pause_button"]
                        gtk::Button {
                            add_css_class: "flat",
                            add_css_class: "circular",
                            add_css_class: "mpris-control",
                            #[watch]
                            set_sensitive: model.mpris.can_play_pause,

                            gtk::Image {
                                add_css_class: "materialicon",
                                #[watch]
                                set_icon_name: Some(material_icon::icon_name(model.mpris.play_pause_icon).as_str()),
                            }
                        },

                        #[name = "mpris_next_button"]
                        gtk::Button {
                            add_css_class: "flat",
                            add_css_class: "circular",
                            add_css_class: "mpris-control",
                            #[watch]
                            set_sensitive: model.mpris.can_go_next,

                            gtk::Image {
                                add_css_class: "materialicon",
                                set_icon_name: Some(material_icon::icon_name("skip_next").as_str()),
                            }
                        }
                    },

                    gtk::Box {
                        add_css_class: "barblock",
                        add_css_class: BACKGROUND_BLUR_CLASS,
                        add_css_class: "panel-widget",
                        set_halign: gtk::Align::End,
                        set_orientation: gtk::Orientation::Horizontal,
                        #[watch]
                        set_tooltip_text: Some(system_stats::tooltip(&model.system_stats).as_str()),
                        set_spacing: 4,

                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 0,

                            gtk::Overlay {
                                #[watch]
                                set_css_classes: &system_stats::arc_root_classes(),

                                add_overlay = &gtk::DrawingArea {
                                    set_css_classes: level_indicator::TRACK_CLASSES,
                                    set_content_width: 8,
                                    set_content_height: 8,
                                    set_draw_func: system_stats::track_draw_func(ArcSide::End),
                                },

                                add_overlay = &gtk::DrawingArea {
                                    #[watch]
                                    set_css_classes: &system_stats::level_classes(model.system_stats.cpu),
                                    set_content_width: 8,
                                    set_content_height: 8,
                                    #[watch]
                                    set_draw_func: system_stats::level_draw_func(model.system_stats.cpu, ArcSide::End),
                                }
                            },

                            #[name = "power_profile_button"]
                            gtk::Button {
                                add_css_class: "flat",
                                add_css_class: "circular",
                                add_css_class: "power-profile-button",
                                #[watch]
                                set_visible: model.power_profile.visible,
                                #[watch]
                                set_tooltip_text: Some(model.power_profile.tooltip.as_str()),

                                gtk::Image {
                                    add_css_class: "materialicon",
                                    add_css_class: "power-profile-icon",
                                    #[watch]
                                    set_icon_name: Some(material_icon::icon_name(model.power_profile.icon).as_str()),
                                }
                            },

                            gtk::Overlay {
                                #[watch]
                                set_css_classes: &system_stats::arc_root_classes(),

                                add_overlay = &gtk::DrawingArea {
                                    set_css_classes: level_indicator::TRACK_CLASSES,
                                    set_content_width: 8,
                                    set_content_height: 8,
                                    set_draw_func: system_stats::track_draw_func(ArcSide::Start),
                                },

                                add_overlay = &gtk::DrawingArea {
                                    #[watch]
                                    set_css_classes: &system_stats::level_classes(model.system_stats.ram),
                                    set_content_width: 8,
                                    set_content_height: 8,
                                    #[watch]
                                    set_draw_func: system_stats::level_draw_func(model.system_stats.ram, ArcSide::Start),
                                }
                            }
                        }
                    },

                    gtk::Box {
                        add_css_class: "barblock",
                        add_css_class: BACKGROUND_BLUR_CLASS,
                        add_css_class: "system-indicators",
                        set_halign: gtk::Align::End,
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 0,

                        #[bind_list(tray_items, row = TrayItem)]
                        tray_items -> gtk::Box {
                            add_css_class: "tray-widget",
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 0,
                        },

                        // TODO(rsynapse-shell): split this into a right-cluster
                        // component once single child component ownership is
                        // cleaned up; the inline view preserves source macro
                        // binding for now but makes this file too large.
                        #[name = "bluetooth_group"]
                        gtk::Box {
                            add_css_class: "button-subgroup-expand-left",
                            add_css_class: "bt-widget",
                            set_orientation: gtk::Orientation::Horizontal,

                            #[name = "bluetooth_revealer"]
                            gtk::Revealer {
                                set_reveal_child: false,
                                set_transition_type: gtk::RevealerTransitionType::SlideLeft,

                                gtk::Box {
                                    add_css_class: "button-subgroup",
                                    set_orientation: gtk::Orientation::Horizontal,

                                    #[name = "bluetooth_keyboard_button"]
                                    gtk::MenuButton {
                                        #[watch]
                                        set_css_classes: &bluetooth::group_classes(&model.bluetooth.keyboard),
                                        #[watch]
                                        set_visible: model.bluetooth.keyboard.visible,
                                        #[watch]
                                        set_tooltip_text: Some(model.bluetooth.keyboard.tooltip.as_str()),

                                        #[wrap(Some)]
                                        set_popover = &gtk::Popover {
                                            add_css_class: "menu",
                                        },

                                        #[wrap(Some)]
                                        set_child = &gtk::Box {
                                            set_orientation: gtk::Orientation::Horizontal,

                                            gtk::Image {
                                                add_css_class: "materialicon",
                                                #[watch]
                                                set_icon_name: Some(material_icon::icon_name(model.bluetooth.keyboard.icon.as_str()).as_str()),
                                            },

                                            gtk::Overlay {
                                                set_css_classes: &bluetooth::battery_root_classes(),
                                                #[watch]
                                                set_visible: model.bluetooth.keyboard.battery.is_some(),

                                                add_overlay = &gtk::DrawingArea {
                                                    set_css_classes: bluetooth::battery_track_classes(),
                                                    set_content_width: 8,
                                                    set_content_height: 8,
                                                    set_draw_func: bluetooth::battery_track_draw_func(),
                                                },

                                                add_overlay = &gtk::DrawingArea {
                                                    #[watch]
                                                    set_css_classes: &bluetooth::battery_level_classes(&model.bluetooth.keyboard),
                                                    set_content_width: 8,
                                                    set_content_height: 8,
                                                    #[watch]
                                                    set_draw_func: bluetooth::battery_level_draw_func(&model.bluetooth.keyboard),
                                                }
                                            }
                                        }
                                    },

                                    #[name = "bluetooth_audio_button"]
                                    gtk::MenuButton {
                                        #[watch]
                                        set_css_classes: &bluetooth::group_classes(&model.bluetooth.audio),
                                        #[watch]
                                        set_visible: model.bluetooth.audio.visible,
                                        #[watch]
                                        set_tooltip_text: Some(model.bluetooth.audio.tooltip.as_str()),

                                        #[wrap(Some)]
                                        set_popover = &gtk::Popover {
                                            add_css_class: "menu",
                                        },

                                        #[wrap(Some)]
                                        set_child = &gtk::Box {
                                            set_orientation: gtk::Orientation::Horizontal,

                                            gtk::Image {
                                                add_css_class: "materialicon",
                                                #[watch]
                                                set_icon_name: Some(material_icon::icon_name(model.bluetooth.audio.icon.as_str()).as_str()),
                                            },

                                            gtk::Overlay {
                                                set_css_classes: &bluetooth::battery_root_classes(),
                                                #[watch]
                                                set_visible: model.bluetooth.audio.battery.is_some(),

                                                add_overlay = &gtk::DrawingArea {
                                                    set_css_classes: bluetooth::battery_track_classes(),
                                                    set_content_width: 8,
                                                    set_content_height: 8,
                                                    set_draw_func: bluetooth::battery_track_draw_func(),
                                                },

                                                add_overlay = &gtk::DrawingArea {
                                                    #[watch]
                                                    set_css_classes: &bluetooth::battery_level_classes(&model.bluetooth.audio),
                                                    set_content_width: 8,
                                                    set_content_height: 8,
                                                    #[watch]
                                                    set_draw_func: bluetooth::battery_level_draw_func(&model.bluetooth.audio),
                                                }
                                            }
                                        }
                                    },

                                    #[name = "bluetooth_pointer_button"]
                                    gtk::MenuButton {
                                        #[watch]
                                        set_css_classes: &bluetooth::group_classes(&model.bluetooth.pointer),
                                        #[watch]
                                        set_visible: model.bluetooth.pointer.visible,
                                        #[watch]
                                        set_tooltip_text: Some(model.bluetooth.pointer.tooltip.as_str()),

                                        #[wrap(Some)]
                                        set_popover = &gtk::Popover {
                                            add_css_class: "menu",
                                        },

                                        #[wrap(Some)]
                                        set_child = &gtk::Box {
                                            set_orientation: gtk::Orientation::Horizontal,

                                            gtk::Image {
                                                add_css_class: "materialicon",
                                                #[watch]
                                                set_icon_name: Some(material_icon::icon_name(model.bluetooth.pointer.icon.as_str()).as_str()),
                                            },

                                            gtk::Overlay {
                                                set_css_classes: &bluetooth::battery_root_classes(),
                                                #[watch]
                                                set_visible: model.bluetooth.pointer.battery.is_some(),

                                                add_overlay = &gtk::DrawingArea {
                                                    set_css_classes: bluetooth::battery_track_classes(),
                                                    set_content_width: 8,
                                                    set_content_height: 8,
                                                    set_draw_func: bluetooth::battery_track_draw_func(),
                                                },

                                                add_overlay = &gtk::DrawingArea {
                                                    #[watch]
                                                    set_css_classes: &bluetooth::battery_level_classes(&model.bluetooth.pointer),
                                                    set_content_width: 8,
                                                    set_content_height: 8,
                                                    #[watch]
                                                    set_draw_func: bluetooth::battery_level_draw_func(&model.bluetooth.pointer),
                                                }
                                            }
                                        }
                                    }
                                }
                            },

                            #[name = "bluetooth_power_button"]
                            gtk::Button {
                                add_css_class: "flat",
                                add_css_class: "circular",
                                add_css_class: "panel-widget",
                                add_css_class: "button-subgroup-main",
                                #[watch]
                                set_tooltip_text: Some(bluetooth::status_tooltip(&model.bluetooth.status).as_str()),

                                gtk::Overlay {
                                    gtk::Image {
                                        add_css_class: "materialicon",
                                        #[watch]
                                        set_icon_name: Some(material_icon::icon_name(model.bluetooth.status.icon.as_str()).as_str()),
                                    },

                                    add_overlay = &gtk::Label {
                                        add_css_class: "bt-count",
                                        #[watch]
                                        set_visible: model.bluetooth.status.connected_count > 0,
                                        #[watch]
                                        set_label: bluetooth::status_count(&model.bluetooth.status).as_str(),
                                    }
                                }
                            }
                        },

                        #[name = "audio_route_button"]
                        gtk::MenuButton {
                            add_css_class: "flat",
                            add_css_class: "circular",
                            add_css_class: "panel-widget",
                            #[watch]
                            set_visible: model.audio.visible,
                            #[watch]
                            set_tooltip_text: Some(audio::route_popover_tooltip(&model.audio)),

                            #[wrap(Some)]
                            set_child = &gtk::Image {
                                add_css_class: "audio-icon",
                                #[watch]
                                set_icon_name: Some(model.audio.icon.as_str()),
                            }
                        },

                        gtk::Image {
                            add_css_class: "panel-widget",
                            add_css_class: "network-icon",
                            add_css_class: "ethernet-icon",
                            #[watch]
                            set_visible: model.network.ethernet.visible,
                            #[watch]
                            set_tooltip_text: Some(model.network.ethernet.tooltip.as_str()),
                            #[watch]
                            set_icon_name: Some(model.network.ethernet.icon.as_str()),
                        },

                        gtk::Image {
                            add_css_class: "panel-widget",
                            add_css_class: "network-icon",
                            add_css_class: "wifi-icon",
                            #[watch]
                            set_visible: model.network.wifi.visible,
                            #[watch]
                            set_tooltip_text: Some(model.network.wifi.tooltip.as_str()),
                            #[watch]
                            set_icon_name: Some(model.network.wifi.icon.as_str()),
                        },

                        gtk::Image {
                            add_css_class: "panel-widget",
                            add_css_class: "battery-icon",
                            #[watch]
                            set_visible: model.battery.present,
                            #[watch]
                            set_tooltip_text: Some(battery_tooltip(&model.battery).as_str()),
                            #[watch]
                            set_icon_name: Some(battery_icon_name(&model.battery).as_str()),
                        }
                    },

                    gtk::MenuButton {
                        add_css_class: "barblock",
                        add_css_class: BACKGROUND_BLUR_CLASS,
                        add_css_class: "flat",
                        add_css_class: "circular",
                        add_css_class: "source-error-widget",
                        #[watch]
                        set_visible: model.source_error_count > 0,
                        #[watch]
                        set_tooltip_text: Some(source_error_tooltip(model.source_error_count).as_str()),

                        #[wrap(Some)]
                        set_child = &gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 4,

                            gtk::Image {
                                add_css_class: "materialicon",
                                add_css_class: "source-error-icon",
                                set_icon_name: Some(material_icon::icon_name("error").as_str()),
                            },

                            gtk::Label {
                                add_css_class: "source-error-count",
                                #[watch]
                                set_label: source_error_count_label(model.source_error_count).as_str(),
                            }
                        },

                        #[wrap(Some)]
                        set_popover = &gtk::Popover {
                            add_css_class: "menu",

                            gtk::Box {
                                add_css_class: "source-error-popover",
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 8,

                                #[bind_list(source_error_items, row = SourceErrorRow)]
                                source_error_items -> gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_spacing: 8,
                                }
                            }
                        }
                    },

                    #[name = "clock_button"]
                    gtk::Button {
                        add_css_class: "barblock",
                        add_css_class: BACKGROUND_BLUR_CLASS,
                        add_css_class: "panel-button",
                        add_css_class: "flat",
                        add_css_class: "circular",
                        add_css_class: "clock-widget",
                        #[watch]
                        set_tooltip_text: Some(model.clock.date.as_str()),

                        gtk::Overlay {
                            gtk::Label {
                                add_css_class: "clock-label",
                                #[watch]
                                set_label: model.clock.time.as_str(),
                            },

                            add_overlay = &gtk::Box {
                                add_css_class: "notification-dot",
                                #[watch]
                                set_visible: model.has_notifications,
                                set_halign: gtk::Align::End,
                                set_valign: gtk::Align::Start,
                            }
                        }
                    }
                }
            }
        }
    }

    async fn init(
        init: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let monitors = if init.primary {
            available_monitors()
        } else {
            Vec::new()
        };
        let monitor = init.monitor.clone().or_else(|| monitors.first().cloned());
        let output_name = init
            .output_name
            .clone()
            .or_else(|| monitor_output_name(monitor.as_ref()));
        log_bar_monitor(init.primary, monitor.as_ref(), output_name.as_deref());

        window::apply_layer_shell_config(&root, bar_window_config());
        if let Some(monitor) = monitor.as_ref() {
            root.set_monitor(Some(monitor));
        }
        root.set_title(Some(init.title));

        let osd = if init.primary {
            let osd_builder = OsdWindow::builder();
            relm4::main_application().add_window(&osd_builder.root);
            Some(
                osd_builder
                    .launch(OsdInit {
                        title: "Rsynapse OSD",
                    })
                    .detach(),
            )
        } else {
            None
        };

        let request_server = if init.primary {
            let request_sender = sender.input_sender().clone();
            match request::start_server(request::RequestTarget::Shell, move |request| {
                request_sender.emit(MainBarInput::Request(request));
            }) {
                Ok(server) => Some(server),
                Err(error) => {
                    eprintln!("[request] failed to start request server: {error}");
                    None
                }
            }
        } else {
            None
        };

        let child_bars = if init.primary {
            launch_secondary_bars(init.title, monitors.into_iter().skip(1))
        } else {
            Vec::new()
        };

        let model = MainBar::new(osd, request_server, child_bars, false, false, output_name);
        let widgets = view_output!();
        let input_sender = sender.input_sender().clone();
        widgets.clock_button.connect_clicked(move |_| {
            input_sender.emit(MainBarInput::ToggleNotificationCenter);
        });
        let input_sender = sender.input_sender().clone();
        widgets.mpris_previous_button.connect_clicked(move |_| {
            input_sender.emit(MainBarInput::Media(MediaAction::Previous));
        });
        let input_sender = sender.input_sender().clone();
        widgets.mpris_play_pause_button.connect_clicked(move |_| {
            input_sender.emit(MainBarInput::Media(MediaAction::PlayPause));
        });
        let input_sender = sender.input_sender().clone();
        widgets.mpris_next_button.connect_clicked(move |_| {
            input_sender.emit(MainBarInput::Media(MediaAction::Next));
        });
        let mpris_motion = gtk::EventControllerMotion::new();
        let mpris_revealer = widgets.mpris_details_revealer.clone();
        mpris_motion.connect_enter(move |_, _, _| {
            mpris_revealer.set_reveal_child(true);
        });
        let mpris_revealer = widgets.mpris_details_revealer.clone();
        mpris_motion.connect_leave(move |_| {
            mpris_revealer.set_reveal_child(false);
        });
        widgets.mpris_group.add_controller(mpris_motion);
        let input_sender = sender.input_sender().clone();
        widgets.bluetooth_power_button.connect_clicked(move |_| {
            input_sender.emit(MainBarInput::ToggleBluetooth);
        });
        let input_sender = sender.input_sender().clone();
        widgets.power_profile_button.connect_clicked(move |_| {
            input_sender.emit(MainBarInput::CyclePowerProfile);
        });
        let audio_route_popover = gtk::Popover::new();
        audio_route_popover.add_css_class("menu");
        audio_route_popover.add_css_class("audio-route-popover");
        let audio_route_mount = gtk::Box::new(gtk::Orientation::Vertical, 0);
        audio_route_popover.set_child(Some(&audio_route_mount));
        widgets
            .audio_route_button
            .set_popover(Some(&audio_route_popover));
        let audio_route_controller = Rc::new(RefCell::new(None));
        mount_popover_component::<AudioRoutePopover>(
            &audio_route_popover,
            &audio_route_mount,
            &audio_route_controller,
            (),
        );
        let audio_route_controller = audio_route_controller.clone();
        let audio_route_popover_for_signal = audio_route_popover.clone();
        let audio_route_mount_for_signal = audio_route_mount.clone();
        audio_route_popover.connect_visible_notify(move |_| {
            mount_popover_component::<AudioRoutePopover>(
                &audio_route_popover_for_signal,
                &audio_route_mount_for_signal,
                &audio_route_controller,
                (),
            );
        });

        let keyboard_popover = widgets
            .bluetooth_keyboard_button
            .popover()
            .expect("Bluetooth keyboard menu button should have a popover");
        let audio_popover = widgets
            .bluetooth_audio_button
            .popover()
            .expect("Bluetooth audio menu button should have a popover");
        let pointer_popover = widgets
            .bluetooth_pointer_button
            .popover()
            .expect("Bluetooth pointer menu button should have a popover");
        mount_bluetooth_group_popover(&keyboard_popover, BluetoothDeviceGroup::Keyboard);
        mount_bluetooth_group_popover(&audio_popover, BluetoothDeviceGroup::Audio);
        mount_bluetooth_group_popover(&pointer_popover, BluetoothDeviceGroup::Pointer);

        let bluetooth_hovered = Rc::new(Cell::new(false));
        let update_bluetooth_revealed = Rc::new({
            let bluetooth_hovered = bluetooth_hovered.clone();
            let bluetooth_revealer = widgets.bluetooth_revealer.clone();
            let bluetooth_power_button = widgets.bluetooth_power_button.clone();
            let keyboard_popover = keyboard_popover.clone();
            let audio_popover = audio_popover.clone();
            let pointer_popover = pointer_popover.clone();

            move || {
                let revealed = bluetooth_hovered.get()
                    || keyboard_popover.is_visible()
                    || audio_popover.is_visible()
                    || pointer_popover.is_visible();
                bluetooth_revealer.set_reveal_child(revealed);
                if revealed {
                    bluetooth_power_button.add_css_class("opened");
                } else {
                    bluetooth_power_button.remove_css_class("opened");
                }
            }
        });

        let bluetooth_motion = gtk::EventControllerMotion::new();
        let update_revealed = update_bluetooth_revealed.clone();
        let hovered = bluetooth_hovered.clone();
        bluetooth_motion.connect_enter(move |_, _, _| {
            hovered.set(true);
            update_revealed();
        });
        let update_revealed = update_bluetooth_revealed.clone();
        let hovered = bluetooth_hovered.clone();
        bluetooth_motion.connect_leave(move |_| {
            hovered.set(false);
            update_revealed();
        });
        widgets.bluetooth_group.add_controller(bluetooth_motion);
        for popover in [keyboard_popover, audio_popover, pointer_popover] {
            let update_revealed = update_bluetooth_revealed.clone();
            popover.connect_visible_notify(move |_| update_revealed());
        }

        AsyncComponentParts { model, widgets }
    }

    async fn update(&mut self, msg: Self::Input, _sender: AsyncComponentSender<Self>) {
        let _span = tracing::trace_span!("bar.main_bar_update", input = main_bar_input_name(&msg))
            .entered();
        match msg {
            MainBarInput::Source(msg) => {
                let previous_audio = self.audio.clone();
                let previous_brightness = self.brightness.clone();
                MainBar::update(self, msg);
                self.maybe_show_audio_osd(previous_audio);
                self.maybe_show_brightness_osd(previous_brightness);
            }
            MainBarInput::Media(action) => launch_playerctl(action, &self.mpris.playerctl_name),
            MainBarInput::ToggleBluetooth => bluetooth::toggle_power(&self.bluetooth.status),
            MainBarInput::CyclePowerProfile => {
                power_profile::cycle_power_profile(&self.power_profile.profile)
            }
            MainBarInput::ToggleNotificationCenter => request_notification_center_toggle(),
            MainBarInput::Request(request) => handle_request(request),
        }
    }
}

impl MainBar {
    fn maybe_show_audio_osd(&mut self, previous_audio: AudioView) {
        if self.audio == previous_audio {
            return;
        }

        if self._audio_osd_ready && self.audio.visible {
            if let Some(osd) = &self._osd {
                osd.sender().emit(OsdInput::ShowAudio(OsdAudioView {
                    icon: self.audio.icon.clone(),
                    percent: self.audio.percent,
                }));
            }
        }
        self._audio_osd_ready = true;
    }

    fn maybe_show_brightness_osd(&mut self, previous_brightness: BrightnessView) {
        if self.brightness == previous_brightness {
            return;
        }

        if self._brightness_osd_ready && self.brightness.visible {
            if let Some(osd) = &self._osd {
                osd.sender()
                    .emit(OsdInput::ShowBrightness(OsdBrightnessView {
                        icon: self.brightness.icon.to_owned(),
                        percent: self.brightness.percent,
                    }));
            }
        }
        self._brightness_osd_ready = true;
    }
}

fn main_bar_input_name(msg: &MainBarInput) -> &'static str {
    match msg {
        MainBarInput::Source(_) => "source",
        MainBarInput::Media(_) => "media",
        MainBarInput::ToggleBluetooth => "toggle-bluetooth",
        MainBarInput::CyclePowerProfile => "cycle-power-profile",
        MainBarInput::ToggleNotificationCenter => "toggle-notification-center",
        MainBarInput::Request(_) => "request",
    }
}

fn handle_request(request: request::PendingRequest) {
    let response = match request.request {
        request::ShellRequest::SchemeToggle => theme::toggle_color_scheme()
            .map(|_| request::RequestResponse::Ok)
            .unwrap_or_else(request::RequestResponse::Error),
        request::ShellRequest::FrostMode(mode) => theme::set_frost_mode(mode.is_frosted())
            .map(|_| request::RequestResponse::Ok)
            .unwrap_or_else(request::RequestResponse::Error),
        request::ShellRequest::Hints(action) => {
            hints::apply(action);
            request::RequestResponse::Ok
        }
        request::ShellRequest::Notifications(_) => request::RequestResponse::Error(
            "notification requests are handled by rsynapse-notifications".to_owned(),
        ),
    };
    request.respond(response);
}

fn request_notification_center_toggle() {
    thread::spawn(|| {
        match request::send_notification_center_action(request::NotificationCenterAction::Toggle) {
            Ok(request::RequestResponse::Ok) => {}
            Ok(request::RequestResponse::Error(error)) => {
                eprintln!("[notifications/request] {error}");
            }
            Err(error) => {
                eprintln!("[notifications/request] {error}");
            }
        }
    });
}

fn available_monitors() -> Vec<gtk::gdk::Monitor> {
    let Some(display) = gtk::gdk::Display::default() else {
        return Vec::new();
    };
    let monitors = display.monitors();
    (0..monitors.n_items())
        .filter_map(|index| monitors.item(index))
        .filter_map(|item| item.downcast::<gtk::gdk::Monitor>().ok())
        .collect()
}

fn launch_secondary_bars(
    title: &'static str,
    monitors: impl Iterator<Item = gtk::gdk::Monitor>,
) -> Vec<AsyncController<MainBar>> {
    monitors
        .map(|monitor| {
            let builder = MainBar::builder();
            let root = builder.root.clone();
            relm4::main_application().add_window(&root);
            let controller = builder
                .launch(MainBarInit::secondary(title, monitor))
                .detach();
            root.present();
            controller
        })
        .collect()
}

fn monitor_output_name(monitor: Option<&gtk::gdk::Monitor>) -> Option<String> {
    monitor
        .and_then(|monitor| monitor.connector())
        .and_then(|connector| non_empty_string(connector.as_str()))
}

fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn log_bar_monitor(primary: bool, monitor: Option<&gtk::gdk::Monitor>, output_name: Option<&str>) {
    let role = if primary { "primary" } else { "secondary" };
    let connector = monitor.and_then(|monitor| monitor.connector());
    let geometry = monitor.map(|monitor| monitor.geometry());
    eprintln!(
        "[bar] launching {role} bar: gtk_connector={:?} geometry={:?} niri_output_filter={:?}",
        connector.as_deref(),
        geometry,
        output_name
    );
}

fn bar_window_config() -> WindowConfig {
    WindowConfig::new(Layer::Top)
        .with_anchors(
            Anchors::NONE
                .with_edge(Edge::Bottom)
                .with_edge(Edge::Right)
                .with_edge(Edge::Left),
        )
        .with_auto_exclusive_zone()
        .with_background_blur_region(BackgroundEffectRegion::RoundedCssClasses {
            classes: BAR_BACKGROUND_BLUR_CLASSES,
            radius: BAR_BACKGROUND_BLUR_RADIUS,
        })
        .with_namespace("rsynapse-bar")
}

fn mount_popover_component<C>(
    popover: &gtk::Popover,
    mount: &gtk::Box,
    controller: &Rc<RefCell<Option<Controller<C>>>>,
    init: C::Init,
) where
    C: Component,
    C::Init: Clone,
    C::Root: AsRef<gtk::Widget> + Clone + std::fmt::Debug,
{
    if popover.is_visible() {
        if controller.borrow().is_none() {
            let launched = C::builder().launch(init).detach();
            let widget = <C::Root as AsRef<gtk::Widget>>::as_ref(launched.widget()).clone();
            mount.append(&widget);
            *controller.borrow_mut() = Some(launched);
        }
        return;
    }

    if let Some(launched) = controller.borrow_mut().take() {
        let widget = <C::Root as AsRef<gtk::Widget>>::as_ref(launched.widget()).clone();
        mount.remove(&widget);
    }
}

fn mount_bluetooth_group_popover(popover: &gtk::Popover, group: BluetoothDeviceGroup) {
    let mount = gtk::Box::new(gtk::Orientation::Vertical, 0);
    popover.set_child(Some(&mount));
    let controller = Rc::new(RefCell::new(None));
    mount_popover_component::<BluetoothGroupPopover>(popover, &mount, &controller, group);

    let popover_for_signal = popover.clone();
    let popover_for_mount = popover.clone();
    let mount_for_mount = mount.clone();
    popover_for_signal.connect_visible_notify(move |_| {
        mount_popover_component::<BluetoothGroupPopover>(
            &popover_for_mount,
            &mount_for_mount,
            &controller,
            group,
        );
    });
}

fn battery_tooltip(battery: &BatteryView) -> String {
    format!("{}%", battery.percent)
}

fn battery_icon_name(battery: &BatteryView) -> String {
    if !battery.present {
        return "battery-missing-symbolic".to_owned();
    }

    let charging = battery.state.is_charging();

    if battery.percent >= 95 && charging {
        return "battery-level-100-charged-symbolic".to_owned();
    }

    let level = (((battery.percent.min(100) as u16) + 5) / 10) * 10;
    let state = if charging { "-charging" } else { "" };

    format!("battery-level-{level}{state}-symbolic")
}

fn source_error_tooltip(count: u64) -> String {
    format!("{count} source error(s) caught")
}

fn source_error_count_label(count: u64) -> String {
    count.to_string()
}

fn mpris_classes(mpris: &MprisView) -> Vec<&'static str> {
    let mut classes = vec!["barblock", BACKGROUND_BLUR_CLASS, "mpris-widget"];
    if !mpris.state_class.is_empty() {
        classes.push(mpris.state_class);
    }
    classes
}

fn launch_playerctl(action: MediaAction, player_name: &str) {
    let player_name = player_name.trim().to_owned();
    let command = match action {
        MediaAction::Previous => "previous",
        MediaAction::PlayPause => "play-pause",
        MediaAction::Next => "next",
    };

    thread::spawn(move || {
        let mut playerctl = Command::new("playerctl");
        if !player_name.is_empty() {
            playerctl.arg("--player").arg(player_name);
        }
        let _ = playerctl.arg(command).status();
    });
}
