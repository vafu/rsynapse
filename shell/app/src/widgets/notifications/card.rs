use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use super::model::{
    NotificationAction, NotificationView, notification_card_classes, notification_icon_name,
};

const NOTIFICATIONS_BUS_NAME: &str = "org.freedesktop.Notifications";
const NOTIFICATIONS_OBJECT_PATH: &str = "/org/freedesktop/Notifications";
const NOTIFICATION_CARD_WIDTH: i32 = 400;
const NOTIFICATION_TEXT_WIDTH_CHARS: i32 = 32;

#[derive(Debug)]
pub(crate) struct NotificationCard {
    notification: NotificationView,
}

#[relm4::component(pub(crate))]
impl SimpleComponent for NotificationCard {
    type Init = NotificationView;
    type Input = ();
    type Output = ();

    view! {
        #[root]
        gtk::Box {
            set_css_classes: &notification_card_classes(&model.notification),
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 8,
            set_hexpand: false,
            set_width_request: NOTIFICATION_CARD_WIDTH,

            gtk::Box {
                add_css_class: "notification-header",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                gtk::Image {
                    add_css_class: "notification-app-icon",
                    set_icon_name: Some(notification_icon_name(&model.notification)),
                    set_pixel_size: 18,
                },

                gtk::Label {
                    add_css_class: "notification-app-name",
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_label: model.notification.display_app_name(),
                },

                gtk::Label {
                    add_css_class: "notification-time",
                    set_halign: gtk::Align::End,
                    set_label: model.notification.created_at.as_str(),
                },

                #[name = "close_button"]
                gtk::Button {
                    add_css_class: "notification-close",
                    add_css_class: "flat",
                    set_tooltip_text: Some("Dismiss notification"),

                    gtk::Image {
                        set_icon_name: Some("window-close-symbolic"),
                    }
                }
            },

            gtk::Box {
                add_css_class: "notification-content",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 10,

                #[name = "content_image"]
                gtk::Image {
                    add_css_class: "notification-image",
                    set_valign: gtk::Align::Start,
                    set_visible: model.notification.has_image(),
                    set_pixel_size: 42,
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_hexpand: true,
                    set_spacing: 3,

                    gtk::Label {
                        add_css_class: "notification-summary",
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                        set_wrap: true,
                        set_wrap_mode: gtk::pango::WrapMode::WordChar,
                        set_width_chars: NOTIFICATION_TEXT_WIDTH_CHARS,
                        set_max_width_chars: NOTIFICATION_TEXT_WIDTH_CHARS,
                        set_label: model.notification.summary.as_str(),
                    },

                    gtk::Label {
                        add_css_class: "notification-body",
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                        set_visible: model.notification.has_body(),
                        set_wrap: true,
                        set_wrap_mode: gtk::pango::WrapMode::WordChar,
                        set_width_chars: NOTIFICATION_TEXT_WIDTH_CHARS,
                        set_max_width_chars: NOTIFICATION_TEXT_WIDTH_CHARS,
                        set_label: model.notification.body.as_str(),
                    }
                },
            },

            #[name = "actions_box"]
            gtk::Box {
                add_css_class: "notification-actions",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_visible: model.notification.has_actions(),
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = NotificationCard { notification: init };
        let widgets = view_output!();
        let notification_id = model.notification.id;

        widgets.close_button.connect_clicked(move |_| {
            request_close_notification(notification_id);
        });

        if let Some(image_path) = model.notification.image_path.as_deref() {
            widgets.content_image.set_from_file(Some(image_path));
        }

        for action in &model.notification.actions {
            let button = notification_action_button(action);
            let action_id = model.notification.id;
            let action_key = action.key.clone();

            button.connect_clicked(move |_| {
                request_action_invoked(action_id, action_key.clone());
            });

            widgets.actions_box.append(&button);
        }

        ComponentParts { model, widgets }
    }
}

fn notification_action_button(action: &NotificationAction) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("notification-action");
    button.set_hexpand(true);
    button.set_label(&action.label);

    if action.key == "default" {
        button.add_css_class("default");
    }

    button
}

fn request_close_notification(id: u32) {
    relm4::spawn_local(async move {
        if let Err(error) = close_notification(id).await {
            eprintln!("[notifications] failed to close notification {id}: {error}");
        }
    });
}

async fn close_notification(id: u32) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(
        &connection,
        NOTIFICATIONS_BUS_NAME,
        NOTIFICATIONS_OBJECT_PATH,
        NOTIFICATIONS_BUS_NAME,
    )
    .await?;
    proxy.call_method("CloseNotification", &(id)).await?;
    Ok(())
}

fn request_action_invoked(id: u32, action_key: String) {
    relm4::spawn_local(async move {
        if let Err(error) = action_invoked(id, action_key.as_str()).await {
            eprintln!("[notifications] failed to invoke notification action {id}: {error}");
        }
    });
}

async fn action_invoked(id: u32, action_key: &str) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    connection
        .emit_signal(
            None::<&str>,
            NOTIFICATIONS_OBJECT_PATH,
            NOTIFICATIONS_BUS_NAME,
            "ActionInvoked",
            &(id, action_key),
        )
        .await
}
