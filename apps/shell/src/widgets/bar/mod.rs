mod audio;
mod bar_indicator;
mod bar_item;
mod battery;
mod bluetooth;
mod brightness;
mod bzbus;
mod context_zone;
mod format;
mod github_status;
mod goals;
mod icon_resolver;
mod layout_indicator;
mod media;
mod mist;
mod monitors;
mod network;
mod overlays;
mod power_profile;
mod project;
mod project_label;
mod secondary;
mod selected_project;
mod source_errors;
mod system_stats;
mod systray;
mod time;
mod window_source;
mod window_tile;
mod workspace;
mod workspace_bar;
mod workspaces;

use std::thread;

use data_sources::SourceError;
use relm4::component::ComponentController;
use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    gtk4_layer_shell::LayerShell,
    window::{self, Anchors, Edge, Layer, WindowConfig},
};

use crate::widgets::nerd_icon::md;
use crate::widgets::nerd_icon::{NerdIcon, NerdIconLabelExt};

use self::audio::{AudioView, audio_status};
use self::battery::BatteryView;
use self::battery::battery_status;
use self::bluetooth::BluetoothIndicator;
use self::brightness::{BrightnessView, brightness_status};
use self::format::{
    battery_icon_name, battery_tooltip, source_error_count_label, source_error_tooltip,
};
use self::media::{MediaView, media_status};
use self::mist::MistIndicator;
use self::monitors::{primary_output, secondary_bar_changes};

use self::goals::GoalIndicator;
use self::network::NetworkIndicators;
use self::power_profile::{PowerProfileView, power_profile_status};
use self::secondary::{
    SecondaryBar, available_monitors, bar_window_config, launch_secondary_bar,
    launch_secondary_bars, launch_workspace_bar, log_bar_monitor, monitor_output_name,
    watch_monitors,
};
use self::selected_project::{
    SelectedProjectItem, SelectedProjectItemInput, SelectedProjectItemOutput,
    SelectedWorkspaceView, selected_project_status,
};
use self::source_errors::{SourceErrorRow, source_error_count, source_error_items};
use self::system_stats::{ArcSide, SysStatsView, sys_stats};
use self::systray::{TrayItem, systray_items};
use self::time::{ClockView, clock};
use self::window_tile::WindowTile;
use self::workspace_bar::{WorkspaceBar, WorkspaceBarInit};
use self::workspaces::{WorkspaceNode, selected_workspace_windows};
use super::{
    OsdAudioView, OsdBrightnessView, OsdInit, OsdInput, OsdWindow, WorkspaceSelector,
    WorkspaceSelectorInput,
};
use crate::request;
use shell_core::list::{ComponentListBoxExt, ComponentListUpdate};

type WindowNode = workspace::WindowObject;
pub(super) const WORKSPACE_RAIL_WIDTH: i32 = bar_indicator::SIZE;
const SYSTEM_STATS_ARC_WIDTH: i32 = 10;
const SYSTEM_STATS_ARC_HEIGHT: i32 = 18;
const DISK_STATS_ARC_WIDTH: i32 = 10;
const DISK_STATS_ARC_HEIGHT: i32 = 18;

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
    ToggleMedia,
    CyclePowerProfile,
    ToggleNotificationCenter,
    CopyBranch(String),
    MonitorsChanged,
    OpenProjectPicker,
    OpenWorkspaceSelector,
    WorkspaceProject(String),
    Request(request::PendingRequest),
}

impl From<sources::Msg> for MainBarInput {
    fn from(msg: sources::Msg) -> Self {
        Self::Source(msg)
    }
}

#[shell_macros::model]
pub struct MainBar {
    _workspace_selector: Option<Controller<WorkspaceSelector>>,
    _media_overlay: Option<Controller<crate::widgets::MprisWindow>>,
    _osd: Option<AsyncController<OsdWindow>>,
    _request_server: Option<request::RequestServer>,
    _workspace_bar: Option<Controller<WorkspaceBar>>,
    _selected_project_item: Option<Controller<SelectedProjectItem>>,
    _goal_indicator: Option<Controller<GoalIndicator>>,
    _bluetooth_indicator: Option<Controller<BluetoothIndicator>>,
    _network_indicators: Option<Controller<NetworkIndicators>>,
    _mist_indicator: Option<Controller<MistIndicator>>,
    _child_bars: Vec<SecondaryBar>,
    _monitor_list: Option<gtk::gio::ListModel>,
    _audio_osd_ready: bool,
    _brightness_osd_ready: bool,
    output_name: Option<String>,

    output_scope: data_sources::StateSignal<Option<String>>,
    _window: gtk::glib::WeakRef<gtk::Window>,

    #[source(data_sources::switch_map(output_scope.observable(), selected_workspace_windows))]
    windows: Vec<WindowNode>,

    #[source(data_sources::switch_map(output_scope.observable(), selected_project_status))]
    selected_project: SelectedWorkspaceView,

    #[source(data_sources::switch_map(output_scope.observable(), layout_indicator::status))]
    layout: layout_indicator::LayoutIndicatorView,

    #[source(battery_status())]
    battery: BatteryView,

    #[source(power_profile_status())]
    power_profile: PowerProfileView,

    #[source(audio_status())]
    audio: AudioView,

    #[source(media_status())]
    media: MediaView,

    #[source(brightness_status())]
    brightness: BrightnessView,

    #[source(systray_items())]
    tray_items: Vec<systray::TrayItemNode>,

    #[source(sys_stats())]
    system_stats: SysStatsView,

    #[source(clock())]
    clock: ClockView,

    #[source(data_sources::notifications::has_notification_items())]
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
                set_shrink_center_last: true,

                #[wrap(Some)]
                set_start_widget = &gtk::Box {
                    add_css_class: "bar-zone",
                    add_css_class: "bar-zone-start",
                    add_css_class: "bar-zone-windows",
                    add_css_class: "bar-indicator-list-horizontal",
                    set_halign: gtk::Align::Start,
                    set_orientation: gtk::Orientation::Horizontal,
                    set_valign: gtk::Align::Center,
                    set_vexpand: false,

                    gtk::CenterBox {
                        add_css_class: "bar-indicator",
                        add_css_class: "bar-corner-indicator",
                        #[watch]
                        set_tooltip_text: Some(selected_project::tooltip(&model.selected_project).as_str()),
                        set_width_request: WORKSPACE_RAIL_WIDTH,
                        set_halign: gtk::Align::Start,
                        set_valign: gtk::Align::Center,
                        set_orientation: gtk::Orientation::Horizontal,

                        #[wrap(Some)]
                        set_center_widget = &gtk::Label {
                            set_css_classes: &["bar-corner-icon", "nerdicon"],
                            set_halign: gtk::Align::Center,
                            set_valign: gtk::Align::Center,
                            #[watch]
                            set_nerd_icon: model.layout.icon.clone(),
                            #[watch]
                            set_tooltip_text: Some(model.layout.tooltip),
                        }
                    },

                    #[bind_list(windows, row = WindowTile)]
                    windows -> gtk::Box {
                        set_widget_name: "workspace-window-list",
                        set_halign: gtk::Align::Start,
                        set_orientation: gtk::Orientation::Horizontal,
                        set_valign: gtk::Align::Center,
                        set_vexpand: false,
                    }
                },

                #[wrap(Some)]
                set_center_widget = &gtk::CenterBox {
                    set_widget_name: "bar-center-block",
                    add_css_class: "bar-zone",
                    add_css_class: "bar-zone-title",
                    set_halign: gtk::Align::Fill,
                    set_orientation: gtk::Orientation::Horizontal,
                    set_valign: gtk::Align::Fill,
                    set_hexpand: true,
                    set_vexpand: false,
                    set_shrink_center_last: true,

                    #[wrap(Some)]
                    #[local_ref]
                    set_start_widget = &goal_indicator_root -> gtk::Overlay {},

                    #[wrap(Some)]
                    #[local_ref]
                    set_center_widget = &selected_project_item_root -> gtk::Box {},
                },

                #[wrap(Some)]
                set_end_widget = &gtk::Box {
                    add_css_class: "bar-zone",
                    add_css_class: "bar-zone-status",
                    add_css_class: "system-cluster",
                    set_halign: gtk::Align::End,
                    set_orientation: gtk::Orientation::Horizontal,
                    set_valign: gtk::Align::Fill,
                    set_vexpand: true,

                    #[local_ref]
                    system_stats_item -> gtk::Box {
                        set_halign: gtk::Align::End,
                        set_orientation: gtk::Orientation::Horizontal,
                        #[watch]
                        set_tooltip_text: Some(system_stats::tooltip(&model.system_stats).as_str()),

                        #[name = "power_profile_button"]
                        gtk::Button {
                            set_css_classes: &bar_item::action_classes(&[
                                "system-stats-button",
                                "power-profile-button",
                            ]),
                            #[watch]
                            set_visible: model.power_profile.visible,
                            #[watch]
                            set_tooltip_text: Some(model.power_profile.tooltip.as_str()),

                            gtk::Box {
                                add_css_class: "system-stats-content",
                                set_halign: gtk::Align::Center,
                                set_valign: gtk::Align::Center,
                                set_orientation: gtk::Orientation::Horizontal,

                                gtk::Overlay {
                                    #[watch]
                                    set_css_classes: &system_stats::arc_root_classes(),
                                    set_halign: gtk::Align::Center,
                                    set_valign: gtk::Align::Center,
                                    set_width_request: SYSTEM_STATS_ARC_WIDTH,
                                    set_height_request: SYSTEM_STATS_ARC_HEIGHT,

                                    add_overlay = &gtk::DrawingArea {
                                        set_css_classes: system_stats::track_classes(),
                                        set_can_target: false,
                                        set_content_width: SYSTEM_STATS_ARC_WIDTH,
                                        set_content_height: SYSTEM_STATS_ARC_HEIGHT,
                                        set_draw_func: system_stats::track_draw_func(ArcSide::End),
                                    },

                                    add_overlay = &gtk::DrawingArea {
                                        #[watch]
                                        set_css_classes: &system_stats::level_classes(model.system_stats.cpu),
                                        set_can_target: false,
                                        set_content_width: SYSTEM_STATS_ARC_WIDTH,
                                        set_content_height: SYSTEM_STATS_ARC_HEIGHT,
                                        #[watch]
                                        set_draw_func: system_stats::level_draw_func(model.system_stats.cpu, ArcSide::End),
                                    }
                                },

                                gtk::Label {
                                    set_css_classes: &[
                                        "nerdicon",
                                        bar_item::ICON_CLASS,
                                        "power-profile-icon",
                                    ],
                                    #[watch]
                                    set_nerd_icon: model.power_profile.icon.clone(),
                                },

                                gtk::Overlay {
                                    #[watch]
                                    set_css_classes: &system_stats::arc_root_classes(),
                                    set_halign: gtk::Align::Center,
                                    set_valign: gtk::Align::Center,
                                    set_width_request: SYSTEM_STATS_ARC_WIDTH,
                                    set_height_request: SYSTEM_STATS_ARC_HEIGHT,

                                    add_overlay = &gtk::DrawingArea {
                                        set_css_classes: system_stats::track_classes(),
                                        set_can_target: false,
                                        set_content_width: SYSTEM_STATS_ARC_WIDTH,
                                        set_content_height: SYSTEM_STATS_ARC_HEIGHT,
                                        set_draw_func: system_stats::track_draw_func(ArcSide::Start),
                                    },

                                    add_overlay = &gtk::DrawingArea {
                                        #[watch]
                                        set_css_classes: &system_stats::level_classes(model.system_stats.ram),
                                        set_can_target: false,
                                        set_content_width: SYSTEM_STATS_ARC_WIDTH,
                                        set_content_height: SYSTEM_STATS_ARC_HEIGHT,
                                        #[watch]
                                        set_draw_func: system_stats::level_draw_func(model.system_stats.ram, ArcSide::Start),
                                    }
                                },
                            }
                        }
                    },

                    #[local_ref]
                    disk_stats_item -> gtk::Box {
                        add_css_class: "bar-item",
                        set_halign: gtk::Align::End,
                        set_valign: gtk::Align::Center,
                        set_orientation: gtk::Orientation::Horizontal,
                        #[watch]
                        set_tooltip_text: Some(system_stats::disk_tooltip(&model.system_stats.disk).as_str()),

                        gtk::Overlay {
                            #[watch]
                            set_css_classes: &system_stats::arc_root_classes(),
                            set_halign: gtk::Align::Center,
                            set_valign: gtk::Align::Center,
                            set_width_request: DISK_STATS_ARC_WIDTH,
                            set_height_request: DISK_STATS_ARC_HEIGHT,

                            add_overlay = &gtk::DrawingArea {
                                set_css_classes: system_stats::track_classes(),
                                set_can_target: false,
                                set_content_width: DISK_STATS_ARC_WIDTH,
                                set_content_height: DISK_STATS_ARC_HEIGHT,
                                set_draw_func: system_stats::track_draw_func(ArcSide::End),
                            },

                            add_overlay = &gtk::DrawingArea {
                                #[watch]
                                set_css_classes: &system_stats::level_classes(model.system_stats.disk.busy),
                                set_can_target: false,
                                set_content_width: DISK_STATS_ARC_WIDTH,
                                set_content_height: DISK_STATS_ARC_HEIGHT,
                                #[watch]
                                set_draw_func: system_stats::level_draw_func(model.system_stats.disk.busy, ArcSide::End),
                            }
                        },

                        gtk::Label {
                            set_css_classes: &[
                                "nerdicon",
                                bar_item::ICON_CLASS,
                                "disk-stats-icon",
                            ],
                            set_nerd_icon: system_stats::disk_icon(),
                        },

                        gtk::Overlay {
                            #[watch]
                            set_css_classes: &system_stats::arc_root_classes(),
                            set_halign: gtk::Align::Center,
                            set_valign: gtk::Align::Center,
                            set_width_request: DISK_STATS_ARC_WIDTH,
                            set_height_request: DISK_STATS_ARC_HEIGHT,

                            add_overlay = &gtk::DrawingArea {
                                set_css_classes: system_stats::track_classes(),
                                set_can_target: false,
                                set_content_width: DISK_STATS_ARC_WIDTH,
                                set_content_height: DISK_STATS_ARC_HEIGHT,
                                set_draw_func: system_stats::track_draw_func(ArcSide::Start),
                            },

                            add_overlay = &gtk::DrawingArea {
                                #[watch]
                                set_css_classes: &system_stats::level_classes(model.system_stats.disk.percent),
                                set_can_target: false,
                                set_content_width: DISK_STATS_ARC_WIDTH,
                                set_content_height: DISK_STATS_ARC_HEIGHT,
                                #[watch]
                                set_draw_func: system_stats::level_draw_func(model.system_stats.disk.percent, ArcSide::Start),
                            }
                        }
                    },


                    gtk::Box {
                        add_css_class: "system-indicators",
                        set_halign: gtk::Align::End,
                        set_orientation: gtk::Orientation::Horizontal,

                        #[bind_list(tray_items, row = TrayItem)]
                        tray_items -> gtk::Box {
                            add_css_class: "tray-widget",
                            set_orientation: gtk::Orientation::Horizontal,
                        },

                        #[local_ref]
                        bluetooth_indicator -> gtk::Box {},

                        #[local_ref]
                        mist_indicator -> gtk::MenuButton {},

                        gtk::Box {
                            set_css_classes: &bar_item::classes(&[
                                bar_item::SQUARE_CLASS,
                                "audio-route-item",
                            ]),
                            #[watch]
                            set_visible: model.audio.visible,
                            #[watch]
                            set_tooltip_text: Some(model.audio.tooltip.as_str()),

                            gtk::Image {
                                add_css_class: bar_item::ICON_CLASS,
                                add_css_class: "audio-icon",
                                set_halign: gtk::Align::Center,
                                set_hexpand: true,
                                #[watch]
                                set_icon_name: Some(model.audio.icon.as_str()),
                            }
                        },

                        gtk::Box {
                            set_css_classes: &bar_item::classes(&[
                                bar_item::SQUARE_CLASS,
                                "media-item",
                            ]),
                            #[watch]
                            set_visible: model.media.visible,
                            #[watch]
                            set_tooltip_text: Some(model.media.tooltip),

                            gtk::Button {
                                set_css_classes: &bar_item::action_classes(&["media-button"]),
                                connect_clicked => MainBarInput::ToggleMedia,

                                gtk::Image {
                                    set_css_classes: &[bar_item::ICON_CLASS, "media-icon"],
                                    set_halign: gtk::Align::Center,
                                    #[watch]
                                    set_icon_name: Some(model.media.icon),
                                }
                            }
                        },

                        #[local_ref]
                        network_indicators -> gtk::Box {},

                        #[local_ref]
                        battery_item -> gtk::Box {
                            #[watch]
                            set_visible: model.battery.present,
                            #[watch]
                            set_tooltip_text: Some(battery_tooltip(&model.battery).as_str()),

                            gtk::Image {
                                set_css_classes: &[bar_item::ICON_CLASS, "battery-icon"],
                                set_halign: gtk::Align::Center,
                                set_hexpand: true,
                                #[watch]
                                set_icon_name: Some(battery_icon_name(&model.battery).as_str()),
                            }
                        }
                    },

                    gtk::Box {
                        set_css_classes: &bar_item::classes(&["source-error-widget"]),
                        #[watch]
                        set_visible: model.source_error_count > 0,
                        #[watch]
                        set_tooltip_text: Some(source_error_tooltip(model.source_error_count).as_str()),

                        gtk::MenuButton {
                            set_css_classes: &bar_item::action_classes(&["source-error-button"]),

                            #[wrap(Some)]
                            set_child = &gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 4,

                                gtk::Label {
                                    set_css_classes: &[
                                        "nerdicon",
                                        bar_item::ICON_CLASS,
                                        "source-error-icon",
                                    ],
                                    set_nerd_icon: NerdIcon::new(md::MD_ALERT),
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

                                    #[name = "source_error_list"]
                                    gtk::Box {
                                        set_orientation: gtk::Orientation::Vertical,
                                        set_spacing: 8,
                                        #[track(model.changed(sources::Field::SourceErrorItems))]
                                        set_component_list: ComponentListUpdate::<SourceErrorRow>::new(&model.source_error_items),
                                    }
                                }
                            }
                        }
                    },

                    gtk::Box {
                        set_css_classes: &bar_item::classes(&["clock-widget"]),
                        #[watch]
                        set_tooltip_text: Some(model.clock.date.as_str()),

                        #[name = "clock_button"]
                        gtk::Button {
                            set_css_classes: &bar_item::action_classes(&["clock-button"]),

                            gtk::Overlay {
                                gtk::Label {
                                    add_css_class: "clock-label",
                                    set_halign: gtk::Align::Center,
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
            let action = gtk::gio::SimpleAction::new(
                "workspace-project",
                Some(gtk::glib::VariantTy::STRING),
            );
            let action_sender = sender.input_sender().clone();
            action.connect_activate(move |_, parameter| {
                if let Some(project) = parameter.and_then(|value| value.get::<String>()) {
                    action_sender.emit(MainBarInput::WorkspaceProject(project));
                }
            });
            relm4::main_application().add_action(&action);
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

        let workspace_bar = launch_workspace_bar(
            "Rsynapse Workspace Rail",
            monitor.clone(),
            output_name.clone(),
        );
        let child_bars = if init.primary {
            launch_secondary_bars(init.title, monitors.into_iter().skip(1))
        } else {
            Vec::new()
        };

        let monitor_list = init
            .primary
            .then(|| watch_monitors(sender.input_sender().clone()))
            .flatten();

        let mut model = MainBar::new(
            None,
            None,
            osd,
            request_server,
            workspace_bar,
            None,
            None,
            None,
            None,
            None,
            child_bars,
            monitor_list,
            false,
            false,
            output_name.clone(),
            data_sources::StateSignal::new(output_name),
            root.downgrade(),
        );
        let system_stats_item = bar_item::container(&["system-stats-widget"]);
        let disk_stats_item = bar_item::container(&["disk-stats-widget"]);
        let battery_item = bar_item::container(&[bar_item::SQUARE_CLASS, "battery-item"]);

        let selected_project_item_builder = SelectedProjectItem::builder();
        let selected_project_item_root = selected_project_item_builder.root.clone();
        let selected_project_item = selected_project_item_builder
            .launch(model.output_scope.clone())
            .forward(sender.input_sender(), |message| match message {
                SelectedProjectItemOutput::OpenWorkspaceSelector => {
                    MainBarInput::OpenWorkspaceSelector
                }
                SelectedProjectItemOutput::CopyBranch(branch) => MainBarInput::CopyBranch(branch),
            });
        model._selected_project_item = Some(selected_project_item);

        let goal_indicator_builder = GoalIndicator::builder();
        let goal_indicator_root = goal_indicator_builder.root.clone();
        let goal_indicator = goal_indicator_builder
            .launch(model.output_scope.clone())
            .detach();
        model._goal_indicator = Some(goal_indicator);

        let bluetooth_indicator_builder = BluetoothIndicator::builder();
        let bluetooth_indicator = bluetooth_indicator_builder.root.clone();
        model._bluetooth_indicator = Some(bluetooth_indicator_builder.launch(()).detach());

        let network_indicators_builder = NetworkIndicators::builder();
        let network_indicators = network_indicators_builder.root.clone();
        model._network_indicators = Some(network_indicators_builder.launch(()).detach());

        let mist_indicator_builder = MistIndicator::builder();
        let mist_indicator = mist_indicator_builder.root.clone();
        model._mist_indicator = Some(mist_indicator_builder.launch(()).detach());

        let widgets = view_output!();
        let input_sender = sender.input_sender().clone();
        widgets.clock_button.connect_clicked(move |_| {
            input_sender.emit(MainBarInput::ToggleNotificationCenter);
        });
        let input_sender = sender.input_sender().clone();
        widgets.power_profile_button.connect_clicked(move |_| {
            input_sender.emit(MainBarInput::CyclePowerProfile);
        });
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
            MainBarInput::ToggleMedia => self.mpris_action(request::ToggleAction::Toggle, false),
            MainBarInput::CyclePowerProfile => {
                power_profile::cycle_power_profile(&self.power_profile.profile)
            }
            MainBarInput::MonitorsChanged => self.reconcile_secondary_bars(),
            MainBarInput::OpenProjectPicker => {
                if let Some(indicator) = &self._selected_project_item {
                    indicator.emit(SelectedProjectItemInput::OpenProjectPicker);
                }
            }
            MainBarInput::OpenWorkspaceSelector => {
                self.workspace_selector_action(request::ToggleAction::Set(true))
            }
            MainBarInput::WorkspaceProject(_project) => {}
            MainBarInput::ToggleNotificationCenter => request_notification_center_toggle(),
            MainBarInput::CopyBranch(branch) => copy_branch_to_clipboard(branch),
            MainBarInput::Request(request) => self.handle_request(request),
        }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        if let Some(workspace_bar) = self._workspace_bar.take() {
            workspace_bar.widget().close();
            relm4::main_application().remove_window(workspace_bar.widget());
        }
        for bar in self._child_bars.drain(..) {
            bar.controller.widget().close();
            relm4::main_application().remove_window(bar.controller.widget());
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

    fn reconcile_secondary_bars(&mut self) {
        let monitors = available_monitors();
        let connected = monitors
            .iter()
            .filter_map(|monitor| monitor_output_name(Some(monitor)))
            .collect::<Vec<_>>();
        let selected = primary_output(self.output_name.as_deref(), &connected);
        if let Some(monitor) = monitors
            .iter()
            .find(|monitor| monitor_output_name(Some(monitor)) == selected)
        {
            // Rebind even when a replacement display reuses the connector name:
            // its GDK monitor object is a new lifetime.
            if let Some(window) = self._window.upgrade() {
                window.set_monitor(Some(monitor));
            }
            if self.output_name != selected {
                if let Some(rail) = self._workspace_bar.take() {
                    rail.widget().close();
                    relm4::main_application().remove_window(rail.widget());
                }
                self._workspace_bar = launch_workspace_bar(
                    "Rsynapse Workspace Rail",
                    Some(monitor.clone()),
                    selected.clone(),
                );
            } else if let Some(rail) = &self._workspace_bar {
                rail.widget().set_monitor(Some(monitor));
            }
        }
        self.output_name = selected.clone();
        self.output_scope.set(selected.clone());
        let existing = self
            ._child_bars
            .iter()
            .map(|bar| bar.output_name.clone())
            .collect::<Vec<_>>();
        let (removed, added) = secondary_bar_changes(
            &existing,
            &connected,
            selected.as_deref().unwrap_or_default(),
        );

        for output_name in removed {
            if let Some(index) = self
                ._child_bars
                .iter()
                .position(|bar| bar.output_name == output_name)
            {
                let bar = self._child_bars.remove(index);
                bar.controller.widget().close();
                relm4::main_application().remove_window(bar.controller.widget());
            }
        }

        for output_name in added {
            if let Some(monitor) = monitors
                .iter()
                .find(|monitor| monitor_output_name(Some(monitor)).as_deref() == Some(&output_name))
            {
                self._child_bars
                    .push(launch_secondary_bar("Rsynapse Shell", monitor.clone()));
            }
        }
    }
}

fn main_bar_input_name(msg: &MainBarInput) -> &'static str {
    match msg {
        MainBarInput::Source(_) => "source",
        MainBarInput::ToggleMedia => "toggle-media",
        MainBarInput::CyclePowerProfile => "cycle-power-profile",
        MainBarInput::ToggleNotificationCenter => "toggle-notification-center",
        MainBarInput::CopyBranch(_) => "copy-branch",
        MainBarInput::MonitorsChanged => "monitors_changed",
        MainBarInput::OpenProjectPicker => "open-project-picker",
        MainBarInput::OpenWorkspaceSelector => "open-workspace-selector",
        MainBarInput::WorkspaceProject(_) => "workspace-project",
        MainBarInput::Request(_) => "request",
    }
}

impl MainBar {
    fn handle_request(&mut self, request: request::PendingRequest) {
        let response = match *request.request() {
            request::ShellRequest::ProjectInit => {
                if let Some(indicator) = &self._selected_project_item {
                    indicator.emit(SelectedProjectItemInput::OpenProjectPicker);
                    request::RequestResponse::Ok
                } else {
                    request::RequestResponse::Error("Project picker is unavailable".to_owned())
                }
            }
            request::ShellRequest::WorkspaceSelector(action) => {
                self.workspace_selector_action(action);
                request::RequestResponse::Ok
            }
            request::ShellRequest::Mpris { action, hold } => {
                self.mpris_action(action, hold);
                request::RequestResponse::Ok
            }
            request::ShellRequest::Notifications(_) => request::RequestResponse::Error(
                "notification requests are handled by rsynapse-notifications".to_owned(),
            ),
        };
        request.respond(response);
    }
}

fn request_notification_center_toggle() {
    thread::spawn(|| {
        match request::send_notification_center_action(request::ToggleAction::Toggle) {
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

fn copy_branch_to_clipboard(branch: String) {
    data_sources::write::copy_to_clipboard(branch);
}

#[cfg(test)]
mod tests {
    use super::secondary_bar_changes;

    #[test]
    fn secondary_bar_changes_adds_and_removes_disconnected_outputs() {
        let existing = vec!["DP-1".to_owned()];
        let connected = vec!["eDP-1".to_owned(), "DP-1".to_owned(), "DP-2".to_owned()];
        assert_eq!(
            secondary_bar_changes(&existing, &connected, "eDP-1"),
            (Vec::<String>::new(), vec!["DP-2".to_owned()])
        );

        let disconnected = vec!["eDP-1".to_owned()];
        assert_eq!(
            secondary_bar_changes(&existing, &disconnected, "eDP-1"),
            (vec!["DP-1".to_owned()], Vec::<String>::new())
        );

        let existing = vec!["eDP-1".to_owned()];
        let connected = vec!["eDP-1".to_owned()];
        assert_eq!(
            secondary_bar_changes(&existing, &connected, "DP-3"),
            (vec!["eDP-1".to_owned()], Vec::<String>::new())
        );
    }
}
