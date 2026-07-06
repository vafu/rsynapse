use std::collections::HashMap;

use shell_core::gtk::glib;
use zvariant::OwnedValue;

use crate::widgets::BACKGROUND_BLUR_CLASS;

const DEFAULT_EXPIRE_TIMEOUT_MS: i32 = 5000;
const NEVER_EXPIRE_TIMEOUT_MS: i32 = 0;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationRequest {
    pub(crate) id: u32,
    pub(crate) app_name: String,
    pub(crate) app_icon: String,
    pub(crate) summary: String,
    pub(crate) body: String,
    pub(crate) actions: Vec<NotificationAction>,
    pub(crate) urgency: NotificationUrgency,
    pub(crate) expire_timeout_ms: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NotificationView {
    pub(crate) id: u32,
    pub(crate) app_name: String,
    pub(crate) app_icon: String,
    pub(crate) summary: String,
    pub(crate) body: String,
    pub(crate) actions: Vec<NotificationAction>,
    pub(crate) urgency: NotificationUrgency,
    pub(crate) created_at: String,
    pub(crate) generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NotificationAction {
    pub(crate) key: String,
    pub(crate) label: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum NotificationUrgency {
    Low,
    #[default]
    Normal,
    Critical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationClosedReason {
    Dismissed,
}

impl NotificationRequest {
    pub(crate) fn new(
        id: u32,
        app_name: String,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout_ms: i32,
    ) -> Self {
        Self {
            id,
            app_name,
            app_icon: app_icon_from_hints(app_icon, &hints),
            summary,
            body,
            actions: notification_actions(actions),
            urgency: urgency_from_hints(&hints),
            expire_timeout_ms: normalized_expire_timeout(expire_timeout_ms),
        }
    }

    pub(crate) fn into_view(self, generation: u64) -> NotificationView {
        NotificationView {
            id: self.id,
            app_name: self.app_name,
            app_icon: self.app_icon,
            summary: self.summary,
            body: self.body,
            actions: self.actions,
            urgency: self.urgency,
            created_at: notification_time_label(),
            generation,
        }
    }
}

impl NotificationView {
    pub(crate) fn has_body(&self) -> bool {
        !self.body.trim().is_empty()
    }

    pub(crate) fn has_app_name(&self) -> bool {
        !self.app_name.trim().is_empty()
    }

    pub(crate) fn has_actions(&self) -> bool {
        !self.actions.is_empty()
    }

    pub(crate) fn display_app_name(&self) -> &str {
        if self.has_app_name() {
            self.app_name.as_str()
        } else {
            "Notification"
        }
    }
}

impl NotificationClosedReason {
    pub(crate) const fn code(self) -> u32 {
        match self {
            Self::Dismissed => 2,
        }
    }
}

pub(crate) fn notification_card_classes(notification: &NotificationView) -> Vec<&'static str> {
    let mut classes = vec!["notification-card", BACKGROUND_BLUR_CLASS];
    if notification.urgency == NotificationUrgency::Critical {
        classes.push("critical");
    }
    classes
}

pub(crate) fn notification_icon_name(notification: &NotificationView) -> &str {
    if notification.app_icon.trim().is_empty() {
        "dialog-information-symbolic"
    } else {
        notification.app_icon.as_str()
    }
}

fn urgency_from_hints(hints: &HashMap<String, OwnedValue>) -> NotificationUrgency {
    match hints
        .get("urgency")
        .and_then(|value| u8::try_from(value).ok())
    {
        Some(0) => NotificationUrgency::Low,
        Some(2) => NotificationUrgency::Critical,
        _ => NotificationUrgency::Normal,
    }
}

fn app_icon_from_hints(app_icon: String, hints: &HashMap<String, OwnedValue>) -> String {
    if !app_icon.trim().is_empty() {
        return app_icon;
    }

    hints
        .get("desktop-entry")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| String::try_from(value).ok())
        .unwrap_or_default()
}

fn notification_actions(actions: Vec<String>) -> Vec<NotificationAction> {
    actions
        .chunks_exact(2)
        .map(|action| NotificationAction {
            key: action[0].clone(),
            label: action[1].clone(),
        })
        .collect()
}

fn normalized_expire_timeout(expire_timeout_ms: i32) -> i32 {
    match expire_timeout_ms {
        timeout if timeout < 0 => DEFAULT_EXPIRE_TIMEOUT_MS,
        NEVER_EXPIRE_TIMEOUT_MS => NEVER_EXPIRE_TIMEOUT_MS,
        timeout => timeout,
    }
}

fn notification_time_label() -> String {
    glib::DateTime::now_local()
        .and_then(|now| now.format("%H:%M"))
        .map(|time| time.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod test;
