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
    pub(crate) image_path: Option<String>,
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
    pub(crate) image_path: Option<String>,
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
        let image_path = image_path_from_inputs(app_icon.as_str(), &hints);
        let app_icon = app_icon_from_hints(app_icon, &hints);
        Self {
            id,
            app_name,
            app_icon,
            image_path,
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
            image_path: self.image_path,
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

    pub(crate) fn has_image(&self) -> bool {
        self.image_path.is_some()
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
    if let Some(app_icon) = non_empty_string(app_icon) {
        if image_path_from_candidate(app_icon.as_str()).is_none() {
            return app_icon;
        }
    }

    string_hint(hints, "desktop-entry").unwrap_or_default()
}

fn image_path_from_inputs(app_icon: &str, hints: &HashMap<String, OwnedValue>) -> Option<String> {
    image_path_from_hints(hints).or_else(|| image_path_from_candidate(app_icon))
}

fn image_path_from_hints(hints: &HashMap<String, OwnedValue>) -> Option<String> {
    ["image-path", "image_path"]
        .into_iter()
        .filter_map(|key| string_hint(hints, key))
        .find_map(|value| image_path_from_candidate(value.as_str()))
}

fn image_path_from_candidate(value: &str) -> Option<String> {
    let value = value.trim();
    if value.starts_with('/') {
        return Some(value.to_owned());
    }

    value
        .strip_prefix("file://")
        .and_then(|path| path.strip_prefix("localhost").or(Some(path)))
        .filter(|path| path.starts_with('/'))
        .map(percent_decode)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Some(byte) = hex_byte(bytes[index + 1], bytes[index + 2]) {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_byte(high: u8, low: u8) -> Option<u8> {
    Some(hex_digit(high)? * 16 + hex_digit(low)?)
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn string_hint(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    hints
        .get(key)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| String::try_from(value).ok())
        .and_then(non_empty_string)
}

fn non_empty_string(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn notification_actions(actions: Vec<String>) -> Vec<NotificationAction> {
    actions
        .chunks_exact(2)
        .filter_map(|action| notification_action(action[0].as_str(), action[1].as_str()))
        .collect()
}

fn notification_action(key: &str, label: &str) -> Option<NotificationAction> {
    let key = key.trim();
    let label = label.trim();
    if key.is_empty() || label.is_empty() {
        return None;
    }

    Some(NotificationAction {
        key: key.to_owned(),
        label: label.to_owned(),
    })
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
