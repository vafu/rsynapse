use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    source::StateSignal,
};

use super::model::{
    NotificationAction, NotificationView, notification_card_classes, notification_icon_name,
};

const NOTIFICATION_CARD_WIDTH: i32 = 400;
const NOTIFICATION_TEXT_WIDTH_CHARS: i32 = 32;

/// Structural row descriptor. Live notification fields are subscribed from
/// `notification`, so same-id replacements update the row in place.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NotificationRowInit {
    pub(crate) id: u32,
    pub(crate) notification: StateSignal<NotificationView>,
}

impl NotificationRowInit {
    pub(crate) fn row_key(&self) -> u32 {
        self.id
    }
}

#[derive(Debug)]
#[shell_macros::model(module = notification_card_sources)]
pub(crate) struct NotificationCard {
    id: u32,
    notification: StateSignal<NotificationView>,
    actions_box: gtk::Box,
    content_image: gtk::Image,
    applied_actions: Vec<NotificationAction>,
    applied_image: Option<(Option<String>, String)>,

    #[source(notification.observable())]
    view: NotificationView,
}

#[derive(Debug)]
pub(crate) enum NotificationCardInput {
    Source(Box<notification_card_sources::Msg>),
    Dismiss,
    Action(String),
}

impl From<notification_card_sources::Msg> for NotificationCardInput {
    fn from(message: notification_card_sources::Msg) -> Self {
        Self::Source(Box::new(message))
    }
}

#[derive(Debug)]
pub enum NotificationCardOutput {
    Dismiss { id: u32 },
    ActionInvoked { id: u32, key: String },
}

#[shell_macros::component(module = notification_card_sources, model = NotificationCard)]
#[relm4::component(pub(crate))]
impl SimpleComponent for NotificationCard {
    type Init = NotificationRowInit;
    type Input = NotificationCardInput;
    type Output = NotificationCardOutput;

    view! {
        #[root]
        gtk::Revealer {
            set_reveal_child: false,
            set_transition_type: gtk::RevealerTransitionType::SlideDown,
            set_transition_duration: 160,

            gtk::Box {
                #[watch]
                set_css_classes: &notification_card_classes(&model.view),
                set_orientation: gtk::Orientation::Vertical,
                set_hexpand: false,
                set_width_request: NOTIFICATION_CARD_WIDTH,

                gtk::Box {
                    add_css_class: "notification-header",
                    set_orientation: gtk::Orientation::Horizontal,

                    gtk::Image {
                        add_css_class: "notification-app-icon",
                        #[watch]
                        set_icon_name: Some(notification_icon_name(&model.view)),
                        set_pixel_size: 18,
                    },

                    gtk::Label {
                        add_css_class: "notification-app-name",
                        set_hexpand: true,
                        set_halign: gtk::Align::Start,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        #[watch]
                        set_label: model.view.display_app_name(),
                    },

                    gtk::Label {
                        add_css_class: "notification-time",
                        set_halign: gtk::Align::End,
                        #[watch]
                        set_label: model.view.created_at.as_str(),
                    },

                    gtk::Button {
                        add_css_class: "notification-close",
                        add_css_class: "flat",
                        set_tooltip_text: Some("Dismiss notification"),
                        connect_clicked => NotificationCardInput::Dismiss,

                        gtk::Image {
                            set_icon_name: Some("window-close-symbolic"),
                        }
                    }
                },

                gtk::Box {
                    add_css_class: "notification-content",
                    set_orientation: gtk::Orientation::Horizontal,

                    #[name = "content_image"]
                    gtk::Image {
                        add_css_class: "notification-image",
                        set_valign: gtk::Align::Start,
                        #[watch]
                        set_visible: model.view.has_image(),
                        set_pixel_size: 42,
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_hexpand: true,

                        gtk::Label {
                            add_css_class: "notification-summary",
                            set_halign: gtk::Align::Start,
                            set_xalign: 0.0,
                            set_wrap: true,
                            set_wrap_mode: gtk::pango::WrapMode::WordChar,
                            set_width_chars: NOTIFICATION_TEXT_WIDTH_CHARS,
                            set_max_width_chars: NOTIFICATION_TEXT_WIDTH_CHARS,
                            #[watch]
                            set_label: model.view.summary.as_str(),
                        },

                        gtk::Label {
                            add_css_class: "notification-body",
                            set_halign: gtk::Align::Start,
                            set_xalign: 0.0,
                            #[watch]
                            set_visible: model.view.has_body(),
                            set_wrap: true,
                            set_wrap_mode: gtk::pango::WrapMode::WordChar,
                            set_width_chars: NOTIFICATION_TEXT_WIDTH_CHARS,
                            set_max_width_chars: NOTIFICATION_TEXT_WIDTH_CHARS,
                            #[watch]
                            set_label: model.view.body.as_str(),
                        }
                    },
                },

                #[name = "actions_box"]
                gtk::Box {
                    add_css_class: "notification-actions",
                    set_orientation: gtk::Orientation::Horizontal,
                    #[watch]
                    set_visible: model.view.has_actions(),
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = NotificationCard::new(
            init.id,
            init.notification,
            gtk::Box::new(gtk::Orientation::Horizontal, 0),
            gtk::Image::new(),
            Vec::new(),
            None,
        );
        model.view = model.notification.get();
        let widgets = view_output!();
        model.actions_box = widgets.actions_box.clone();
        model.content_image = widgets.content_image.clone();

        sync_image(&mut model);
        sync_actions(&mut model, &sender);

        gtk::glib::idle_add_local_once(move || root.set_reveal_child(true));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            NotificationCardInput::Source(message) => {
                NotificationCard::update(self, *message);
                if self.changed(notification_card_sources::Field::View) {
                    sync_image(self);
                    sync_actions(self, &sender);
                }
            }
            NotificationCardInput::Dismiss => {
                let _ = sender.output(NotificationCardOutput::Dismiss { id: self.id });
            }
            NotificationCardInput::Action(key) => {
                let _ = sender.output(NotificationCardOutput::ActionInvoked { id: self.id, key });
            }
        }
    }
}

fn sync_image(card: &mut NotificationCard) {
    let image_key = (card.view.image_path.clone(), card.view.app_icon.clone());
    if card.applied_image.as_ref() == Some(&image_key) {
        return;
    }
    card.applied_image = Some(image_key);
    match card.view.image_path.as_deref() {
        Some(path) => card.content_image.set_from_file(Some(path)),
        None => card
            .content_image
            .set_icon_name(Some(notification_icon_name(&card.view))),
    }
}

fn sync_actions(card: &mut NotificationCard, sender: &ComponentSender<NotificationCard>) {
    if card.applied_actions == card.view.actions {
        return;
    }
    card.applied_actions.clone_from(&card.view.actions);

    while let Some(child) = card.actions_box.first_child() {
        card.actions_box.remove(&child);
    }

    for action in &card.view.actions {
        let button = notification_action_button(action);
        let sender = sender.clone();
        let key = action.key.clone();
        button.connect_clicked(move |_| {
            let _ = sender.input(NotificationCardInput::Action(key.clone()));
        });
        card.actions_box.append(&button);
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
