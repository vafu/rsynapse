mod card;
mod model;
mod policy;
mod service;
#[cfg(test)]
mod test;

use std::time::Duration;

use futures_util::StreamExt;

use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    list::{ComponentListBoxExt, ComponentListUpdate},
    source::{
        Observable,
        dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
        rx::Observable as _,
    },
    window::{self, Anchors, Edge, Layer, SurfaceMargins, WindowConfig},
};

use crate::{request, session};

use super::BACKGROUND_BLUR_CLASS;
use card::NotificationCard;
use model::NotificationView;
pub use model::{NotificationClosedReason, NotificationRequest};
use policy::{NotificationCenterContext, NotificationCenterPolicy};

const NOTIFICATION_PANEL_WIDTH: i32 = 432;
const NOTIFICATION_CONTENT_WIDTH: i32 = 400;
const NOTIFICATION_CENTER_MAX_HEIGHT: i32 = 520;
const EPHEMERAL_CENTER_TIMEOUT_MS: u64 = 30_000;

pub(crate) fn has_notification_items() -> Observable<bool> {
    dbus::property_or(
        PropertyDescriptor::new(notification_state_object(), "HasItems"),
        false,
    )
}

fn notification_state_object() -> ObjectDescriptor {
    ObjectDescriptor::parse(
        Bus::Session,
        service::NOTIFICATIONS_BUS_NAME,
        service::NOTIFICATIONS_OBJECT_PATH,
        service::RSYNAPSE_NOTIFICATIONS_INTERFACE,
    )
    .expect("static notification state D-Bus descriptor should be valid")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationsInit {
    pub title: &'static str,
}

pub struct NotificationsWindow {
    center_visible: bool,
    _request_server: Option<request::RequestServer>,
    center_policy: NotificationCenterPolicy,
    dbus_service: Option<service::NotificationsService>,
    session_locked: bool,
    _session_lock_task: Option<tokio::task::JoinHandle<()>>,
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
    ExpirePopup {
        id: u32,
        generation: u64,
    },
    SessionLocked(bool),
    Clear,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PopupExpiry {
    id: u32,
    generation: u64,
    timeout_ms: u64,
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
                        set_orientation: gtk::Orientation::Vertical,
                        set_width_request: NOTIFICATION_PANEL_WIDTH,

                        gtk::Box {
                            add_css_class: "notification-center-header",
                            set_orientation: gtk::Orientation::Horizontal,

                            gtk::Label {
                                add_css_class: "notification-center-title",
                                add_css_class: BACKGROUND_BLUR_CLASS,
                                set_hexpand: true,
                                set_halign: gtk::Align::Start,
                                set_label: "Notifications",
                            },

                            #[name = "clear_button"]
                            gtk::Button {
                                add_css_class: "notification-center-control",
                                add_css_class: BACKGROUND_BLUR_CLASS,
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
                            add_css_class: BACKGROUND_BLUR_CLASS,
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

        let dbus_service = Some(service::start(sender.input_sender().clone()));
        let session_lock_task = Some(spawn_session_lock_subscription(
            sender.input_sender().clone(),
        ));
        let model = NotificationsWindow {
            center_visible: false,
            _request_server: request_server,
            center_policy: NotificationCenterPolicy::load(),
            dbus_service,
            session_locked: true,
            _session_lock_task: session_lock_task,
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
            NotificationsInput::ExpirePopup { id, generation } => {
                self.expire_popup(id, generation);
            }
            NotificationsInput::SessionLocked(locked) => self.set_session_locked(locked),
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
        let Some(expiry) = self.apply_notification(request) else {
            return;
        };

        let input = sender.input_sender().clone();
        relm4::spawn_local(async move {
            gtk::glib::timeout_future(Duration::from_millis(expiry.timeout_ms)).await;
            input.emit(NotificationsInput::ExpirePopup {
                id: expiry.id,
                generation: expiry.generation,
            });
        });
    }

    fn apply_notification(&mut self, request: NotificationRequest) -> Option<PopupExpiry> {
        self.generation = self.generation.wrapping_add(1);
        let expire_timeout_ms = request.expire_timeout_ms;
        let view = request.into_view(self.generation);

        if self.session_locked {
            self.store_notification(view);
            return None;
        }

        let stays_in_center = self
            .center_policy
            .should_store(&view, self.center_context());
        let expiry = (expire_timeout_ms > 0 || stays_in_center).then_some(PopupExpiry {
            id: view.id,
            generation: view.generation,
            timeout_ms: center_expire_timeout_ms(expire_timeout_ms),
        });
        upsert_latest(&mut self.popup_notifications, view);
        expiry
    }

    fn close_notification(&mut self, id: u32, _reason: NotificationClosedReason) {
        remove_notification(&mut self.notifications, id);
        remove_notification(&mut self.popup_notifications, id);
        self.publish_has_items();
    }

    fn expire_popup(&mut self, id: u32, generation: u64) {
        let notification = remove_generation(&mut self.popup_notifications, id, generation);
        if let Some(notification) = notification {
            self.store_notification(notification);
        }
    }

    fn set_session_locked(&mut self, locked: bool) {
        if self.session_locked == locked {
            return;
        }

        self.session_locked = locked;
        if locked {
            self.store_popup_notifications();
        }
    }

    fn store_popup_notifications(&mut self) {
        let notifications = std::mem::take(&mut self.popup_notifications);
        for notification in notifications.into_iter().rev() {
            self.store_notification(notification);
        }
    }

    fn store_notification(&mut self, notification: NotificationView) {
        if !self
            .center_policy
            .should_store(&notification, self.center_context())
        {
            return;
        }

        upsert_latest(&mut self.notifications, notification);
        self.publish_has_items();
    }

    fn center_context(&self) -> NotificationCenterContext {
        NotificationCenterContext {
            session_locked: self.session_locked,
        }
    }

    fn clear_notifications(&mut self) {
        self.notifications.clear();
        self.popup_notifications.clear();
        self.publish_has_items();
    }

    fn publish_has_items(&self) {
        if let Some(service) = &self.dbus_service {
            service.set_has_items(!self.notifications.is_empty());
        }
    }
}

fn spawn_session_lock_subscription(
    input_sender: relm4::Sender<NotificationsInput>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut stream = session::locked().into_stream();
        while let Some(item) = stream.next().await {
            let locked = match item {
                Ok(locked) => locked,
                Err(error) => {
                    eprintln!("[notifications/session] lock source failed: {error}");
                    return;
                }
            };
            if input_sender
                .send(NotificationsInput::SessionLocked(locked))
                .is_err()
            {
                return;
            }
        }
    })
}

fn upsert_latest(notifications: &mut Vec<NotificationView>, notification: NotificationView) {
    remove_notification(notifications, notification.id);
    notifications.insert(0, notification);
}

fn remove_notification(notifications: &mut Vec<NotificationView>, id: u32) {
    notifications.retain(|notification| notification.id != id);
}

fn remove_generation(
    notifications: &mut Vec<NotificationView>,
    id: u32,
    generation: u64,
) -> Option<NotificationView> {
    let removed = notifications
        .iter()
        .find(|notification| notification.id == id && notification.generation == generation)
        .cloned();
    notifications
        .retain(|notification| notification.id != id || notification.generation != generation);
    removed
}

fn center_expire_timeout_ms(expire_timeout_ms: i32) -> u64 {
    if expire_timeout_ms > 0 {
        expire_timeout_ms as u64
    } else {
        EPHEMERAL_CENTER_TIMEOUT_MS
    }
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
}
