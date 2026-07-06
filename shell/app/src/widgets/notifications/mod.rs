mod card;
mod model;
mod service;

use std::time::Duration;

use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    list::{ComponentListBoxExt, ComponentListUpdate},
    source::{self, Observable},
    window::{self, Anchors, Edge, Layer, SurfaceMargins, WindowConfig},
};

use crate::request;

use super::BACKGROUND_BLUR_CLASS;
use card::NotificationCard;
use model::NotificationView;
pub use model::{NotificationClosedReason, NotificationRequest};

const NOTIFICATION_BACKGROUND_BLUR_CLASSES: &[&str] = &[BACKGROUND_BLUR_CLASS];
const NOTIFICATION_BACKGROUND_BLUR_RADIUS: i32 = 12;
const NOTIFICATION_PANEL_WIDTH: i32 = 432;
const NOTIFICATION_CONTENT_WIDTH: i32 = 400;
const NOTIFICATION_CENTER_MAX_HEIGHT: i32 = 520;

pub(crate) fn has_notification_items() -> Observable<bool> {
    source::once(false)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationsInit {
    pub title: &'static str,
}

pub struct NotificationsWindow {
    center_visible: bool,
    _request_server: Option<request::RequestServer>,
    _dbus_server: Option<tokio::task::JoinHandle<()>>,
    generation: u64,
    notifications: Vec<NotificationView>,
    popup_notifications: Vec<NotificationView>,
}

#[derive(Debug)]
pub enum NotificationsInput {
    Request(request::PendingRequest),
    Show(NotificationRequest),
    Close {
        id: u32,
        reason: NotificationClosedReason,
    },
    Expire {
        id: u32,
        generation: u64,
    },
    Clear,
}

#[relm4::component(pub, async)]
impl SimpleAsyncComponent for NotificationsWindow {
    type Init = NotificationsInit;
    type Input = NotificationsInput;
    type Output = ();

    view! {
        #[root]
        gtk::Window {
            add_css_class: "notifications-window",
            #[watch]
            set_visible: model.window_visible(),

            gtk::Box {
                add_css_class: "notification-window-body",
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::End,
                set_width_request: NOTIFICATION_PANEL_WIDTH,

                gtk::Revealer {
                    set_transition_type: gtk::RevealerTransitionType::SlideUp,
                    set_transition_duration: 160,
                    #[watch]
                    set_visible: !model.center_visible && !model.popup_notifications.is_empty(),
                    #[watch]
                    set_reveal_child: !model.center_visible && !model.popup_notifications.is_empty(),

                    #[name = "popup_notifications"]
                    gtk::Box {
                        add_css_class: "notifications-stack",
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 8,
                        set_width_request: NOTIFICATION_PANEL_WIDTH,
                        #[watch]
                        set_component_list: ComponentListUpdate::<NotificationCard>::new(
                            &model.popup_notifications
                        ),
                    },
                },

                gtk::Revealer {
                    #[watch]
                    set_visible: model.center_visible,
                    #[watch]
                    set_reveal_child: model.center_visible,
                    set_transition_type: gtk::RevealerTransitionType::SlideUp,
                    set_transition_duration: 180,

                    gtk::Box {
                        add_css_class: "notification-center",
                        add_css_class: BACKGROUND_BLUR_CLASS,
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 8,
                        set_width_request: NOTIFICATION_PANEL_WIDTH,

                        gtk::Box {
                            add_css_class: "notification-center-header",
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 8,

                            gtk::Label {
                                add_css_class: "notification-center-title",
                                set_hexpand: true,
                                set_halign: gtk::Align::Start,
                                set_label: "Notifications",
                            },

                            #[name = "clear_button"]
                            gtk::Button {
                                add_css_class: "notification-center-control",
                                add_css_class: "flat",
                                set_tooltip_text: Some("Clear notifications"),
                                #[watch]
                                set_visible: !model.notifications.is_empty(),

                                gtk::Image {
                                    add_css_class: "materialicon",
                                    set_icon_name: Some("edit-clear-symbolic"),
                                }
                            }
                        },

                        gtk::Label {
                            add_css_class: "notification-empty",
                            #[watch]
                            set_visible: model.notifications.is_empty(),
                            set_label: "No notifications",
                        },

                        gtk::ScrolledWindow {
                            add_css_class: "notification-center-scroll",
                            set_min_content_width: NOTIFICATION_CONTENT_WIDTH,
                            set_max_content_width: NOTIFICATION_CONTENT_WIDTH,
                            set_max_content_height: NOTIFICATION_CENTER_MAX_HEIGHT,
                            set_propagate_natural_height: true,
                            #[watch]
                            set_visible: !model.notifications.is_empty(),

                            #[name = "notifications"]
                            gtk::Box {
                                add_css_class: "notification-center-list",
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 8,
                                #[watch]
                                set_component_list: ComponentListUpdate::<NotificationCard>::new(
                                    &model.notifications
                                ),
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
        window::apply_layer_shell_config(&root, notifications_window_config());
        root.set_title(Some(init.title));

        let request_sender = sender.input_sender().clone();
        let request_server =
            match request::start_server(request::RequestTarget::Notifications, move |request| {
                request_sender.emit(NotificationsInput::Request(request));
            }) {
                Ok(server) => Some(server),
                Err(error) => {
                    eprintln!("[request] failed to start notifications request server: {error}");
                    None
                }
            };

        let dbus_server = Some(service::start(sender.input_sender().clone()));
        let model = NotificationsWindow {
            center_visible: false,
            _request_server: request_server,
            _dbus_server: dbus_server,
            generation: 0,
            notifications: Vec::new(),
            popup_notifications: Vec::new(),
        };
        let widgets = view_output!();

        let input_sender = sender.input_sender().clone();
        widgets.clear_button.connect_clicked(move |_| {
            input_sender.emit(NotificationsInput::Clear);
        });

        AsyncComponentParts { model, widgets }
    }

    async fn update(&mut self, msg: Self::Input, sender: AsyncComponentSender<Self>) {
        match msg {
            NotificationsInput::Request(request) => self.handle_request(request),
            NotificationsInput::Show(request) => self.show_notification(request, &sender),
            NotificationsInput::Close { id, reason } => self.close_notification(id, reason),
            NotificationsInput::Expire { id, generation } => {
                self.expire_popup(id, generation);
            }
            NotificationsInput::Clear => self.clear_notifications(),
        }
    }
}

impl NotificationsWindow {
    fn window_visible(&self) -> bool {
        self.center_visible || !self.popup_notifications.is_empty()
    }

    fn handle_request(&mut self, request: request::PendingRequest) {
        let response = match request.request {
            request::ShellRequest::Notifications(action) => {
                self.center_visible = match action {
                    request::NotificationCenterAction::Set(visible) => visible,
                    request::NotificationCenterAction::Toggle => !self.center_visible,
                };
                request::RequestResponse::Ok
            }
            request::ShellRequest::SchemeToggle
            | request::ShellRequest::FrostMode(_)
            | request::ShellRequest::Hints(_) => request::RequestResponse::Error(
                "shell requests are handled by rsynapse-shell".to_owned(),
            ),
        };
        request.respond(response);
    }

    fn show_notification(
        &mut self,
        request: NotificationRequest,
        sender: &AsyncComponentSender<Self>,
    ) {
        self.generation = self.generation.wrapping_add(1);
        let expire_timeout_ms = request.expire_timeout_ms;
        let view = request.into_view(self.generation);
        let id = view.id;
        let generation = view.generation;

        upsert_latest(&mut self.notifications, view.clone());
        upsert_latest(&mut self.popup_notifications, view);

        if expire_timeout_ms > 0 {
            let input = sender.input_sender().clone();
            relm4::spawn_local(async move {
                gtk::glib::timeout_future(Duration::from_millis(expire_timeout_ms as u64)).await;
                input.emit(NotificationsInput::Expire { id, generation });
            });
        }
    }

    fn close_notification(&mut self, id: u32, _reason: NotificationClosedReason) {
        remove_notification(&mut self.notifications, id);
        remove_notification(&mut self.popup_notifications, id);
    }

    fn expire_popup(&mut self, id: u32, generation: u64) {
        self.popup_notifications
            .retain(|notification| notification.id != id || notification.generation != generation);
    }

    fn clear_notifications(&mut self) {
        self.notifications.clear();
        self.popup_notifications.clear();
    }
}

fn upsert_latest(notifications: &mut Vec<NotificationView>, notification: NotificationView) {
    remove_notification(notifications, notification.id);
    notifications.insert(0, notification);
}

fn remove_notification(notifications: &mut Vec<NotificationView>, id: u32) {
    notifications.retain(|notification| notification.id != id);
}

const fn notifications_window_config() -> WindowConfig {
    WindowConfig::new(Layer::Overlay)
        .with_anchors(Anchors::NONE.with_edge(Edge::Bottom).with_edge(Edge::Right))
        .with_surface_margins(SurfaceMargins {
            top: 0,
            right: 16,
            bottom: 0,
            left: 0,
        })
        .with_namespace("rsynapse-notifications")
        .with_rounded_background_blur_for_css_classes(
            NOTIFICATION_BACKGROUND_BLUR_CLASSES,
            NOTIFICATION_BACKGROUND_BLUR_RADIUS,
        )
}
