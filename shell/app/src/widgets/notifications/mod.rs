mod card;
mod model;
mod policy;
mod service;
#[cfg(test)]
mod test;

use std::time::Duration;

use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    list::{ComponentListBoxExt, ComponentListUpdate},
    source::{
        Observable,
        dbus::{self, Bus, ObjectDescriptor, PropertyDescriptor},
    },
    window::{self, Anchors, Edge, Layer, SurfaceMargins, WindowConfig},
};

use crate::request;

use card::{NotificationCard, NotificationCardOutput, NotificationRowInit};
pub use model::{NotificationClosedReason, NotificationRequest};
use model::{NotificationEntry, NotificationView};
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

#[shell_macros::model(module = notifications_sources)]
pub struct NotificationsWindow {
    center_visible: bool,
    _request_server: Option<request::RequestServer>,
    center_policy: NotificationCenterPolicy,
    dbus_service: Option<service::NotificationsService>,
    generation: u64,
    notifications: Vec<NotificationEntry>,
    popup_notifications: Vec<NotificationEntry>,
    popup_list: Option<gtk::Box>,
    center_list: Option<gtk::Box>,

    #[source(crate::session::locked())]
    session_locked: bool,
}

#[derive(Debug)]
pub enum NotificationsInput {
    Source(notifications_sources::Msg),
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
    Clear,
    NotificationRow(NotificationCardOutput),
}

impl From<notifications_sources::Msg> for NotificationsInput {
    fn from(message: notifications_sources::Msg) -> Self {
        Self::Source(message)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PopupExpiry {
    id: u32,
    generation: u64,
    timeout_ms: u64,
}

#[shell_macros::component(model = NotificationsWindow)]
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
        let mut model = NotificationsWindow::new(
            false,
            request_server,
            NotificationCenterPolicy::load(),
            dbus_service,
            0,
            Vec::new(),
            Vec::new(),
            None,
            None,
        );
        model.session_locked = true;
        let widgets = view_output!();
        model.popup_list = Some(widgets.popup_notifications.clone());
        model.center_list = Some(widgets.notifications.clone());

        let input_sender = sender.input_sender().clone();
        widgets.clear_button.connect_clicked(move |_| {
            input_sender.emit(NotificationsInput::Clear);
        });

        reconcile_lists(&model, &sender);

        AsyncComponentParts { model, widgets }
    }

    async fn update(&mut self, msg: Self::Input, sender: AsyncComponentSender<Self>) {
        let previous = membership(self);
        match msg {
            NotificationsInput::Request(request) => {
                self.handle_request(request);
            }
            NotificationsInput::Show(request) => {
                self.show_notification(request, &sender);
            }
            NotificationsInput::Close { id, reason } => {
                self.close_notification(id, reason);
            }
            NotificationsInput::ExpirePopup { id, generation } => {
                self.expire_popup(id, generation);
            }
            NotificationsInput::Clear => {
                self.clear_notifications();
            }
            NotificationsInput::NotificationRow(output) => {
                self.handle_card_output(output);
            }
            NotificationsInput::Source(message) => {
                NotificationsWindow::update(self, message);
                self.set_session_locked(self.session_locked);
            }
        }

        if membership(self) != previous {
            reconcile_lists(self, &sender);
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
                    request::ToggleAction::Set(visible) => visible,
                    request::ToggleAction::Toggle => !self.center_visible,
                };
                request::RequestResponse::Ok
            }
            request::ShellRequest::ProjectInit
            | request::ShellRequest::Workspace(_)
            | request::ShellRequest::WorkspaceSelector(_)
            | request::ShellRequest::Remarked(_)
            | request::ShellRequest::Mpris { .. } => request::RequestResponse::Error(
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

    fn handle_card_output(&mut self, output: NotificationCardOutput) {
        match output {
            NotificationCardOutput::Dismiss { id } => {
                service::request_close_notification(id);
            }
            NotificationCardOutput::ActionInvoked { id, key } => {
                service::request_action_invoked(id, key);
            }
        }
    }

    fn close_notification(&mut self, id: u32, _reason: NotificationClosedReason) {
        remove_notification(&mut self.notifications, id);
        remove_notification(&mut self.popup_notifications, id);
        self.publish_has_items();
    }

    fn expire_popup(&mut self, id: u32, generation: u64) {
        let entry = remove_generation(&mut self.popup_notifications, id, generation);
        if let Some(entry) = entry {
            self.store_entry(entry);
        }
    }

    fn set_session_locked(&mut self, locked: bool) {
        self.session_locked = locked;
        if locked {
            self.store_popup_notifications();
        }
    }

    fn store_popup_notifications(&mut self) {
        let notifications = std::mem::take(&mut self.popup_notifications);
        for entry in notifications.into_iter().rev() {
            self.store_entry(entry);
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

    fn store_entry(&mut self, entry: NotificationEntry) {
        let view = entry.current_view();
        if !self
            .center_policy
            .should_store(&view, self.center_context())
        {
            return;
        }

        upsert_entry(&mut self.notifications, entry);
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

fn reconcile_lists(
    model: &NotificationsWindow,
    sender: &AsyncComponentSender<NotificationsWindow>,
) {
    let center_rows = row_inits(&model.notifications);
    let popup_rows = row_inits(&model.popup_notifications);

    if let Some(center_list) = &model.center_list {
        center_list.set_component_list_by_key_forward(
            ComponentListUpdate::<NotificationCard>::new(&center_rows),
            NotificationRowInit::row_key,
            sender.input_sender(),
            NotificationsInput::NotificationRow,
        );
    }
    if let Some(popup_list) = &model.popup_list {
        popup_list.set_component_list_by_key_forward(
            ComponentListUpdate::<NotificationCard>::new(&popup_rows),
            NotificationRowInit::row_key,
            sender.input_sender(),
            NotificationsInput::NotificationRow,
        );
    }
}

fn row_inits(entries: &[NotificationEntry]) -> Vec<NotificationRowInit> {
    entries
        .iter()
        .map(|entry| NotificationRowInit {
            id: entry.id,
            notification: entry.view.clone(),
        })
        .collect()
}

fn membership(model: &NotificationsWindow) -> (Vec<u32>, Vec<u32>) {
    (
        model.notifications.iter().map(|entry| entry.id).collect(),
        model
            .popup_notifications
            .iter()
            .map(|entry| entry.id)
            .collect(),
    )
}

fn upsert_latest(notifications: &mut Vec<NotificationEntry>, notification: NotificationView) {
    upsert_entry(notifications, NotificationEntry::new(notification));
}

fn upsert_entry(notifications: &mut Vec<NotificationEntry>, entry: NotificationEntry) {
    let view = entry.current_view();
    if let Some(index) = notifications
        .iter()
        .position(|existing| existing.id == entry.id)
    {
        let mut existing = notifications.remove(index);
        existing.generation = entry.generation;
        existing.view.set(view);
        notifications.insert(0, existing);
        return;
    }

    notifications.insert(0, entry);
}

fn remove_notification(notifications: &mut Vec<NotificationEntry>, id: u32) {
    notifications.retain(|notification| notification.id != id);
}

fn remove_generation(
    notifications: &mut Vec<NotificationEntry>,
    id: u32,
    generation: u64,
) -> Option<NotificationEntry> {
    let index = notifications
        .iter()
        .position(|notification| notification.id == id && notification.generation == generation)?;
    Some(notifications.remove(index))
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
