mod config;
mod error;

use std::{fs, io, path::PathBuf};

use regex::Regex;

use self::{
    config::{ActionPresenceConfig, ActionPresenceName, PolicyConfig, RuleConfig},
    error::{PolicyConfigError, PolicyLoadError},
};
use super::model::{NotificationUrgency, NotificationView};

#[derive(Clone, Debug)]
pub(crate) struct NotificationCenterPolicy {
    rules: Vec<NotificationCenterRule>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct NotificationCenterContext {
    pub(crate) session_locked: bool,
}

#[derive(Clone, Debug)]
struct NotificationCenterRule {
    keep: bool,
    matcher: NotificationMatcher,
}

#[derive(Clone, Debug, Default)]
struct NotificationMatcher {
    is_urgent: Option<bool>,
    session_locked: Option<bool>,
    title_match: Option<Regex>,
    content_match: Option<Regex>,
    app_name_match: Option<Regex>,
    app_icon_match: Option<Regex>,
    actions: Option<ActionPresence>,
    action_key_match: Option<Regex>,
    action_label_match: Option<Regex>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActionPresence {
    Any,
    None,
}

impl NotificationCenterPolicy {
    pub(crate) fn load() -> Self {
        match Self::from_path(config::path()) {
            Ok(policy) => policy,
            Err(PolicyLoadError::NotFound) => Self::default(),
            Err(error) => {
                eprintln!("[notifications/policy] {error}; using default policy");
                Self::default()
            }
        }
    }

    pub(crate) fn from_toml(input: &str) -> Result<Self, PolicyConfigError> {
        let config: PolicyConfig = toml::from_str(input).map_err(PolicyConfigError::Toml)?;
        let mut rules = Vec::with_capacity(config.rules.len());
        for (index, rule) in config.rules.into_iter().enumerate() {
            rules.push(NotificationCenterRule::from_config(index, rule)?);
        }
        Ok(Self { rules })
    }

    pub(crate) fn should_store(
        &self,
        notification: &NotificationView,
        context: NotificationCenterContext,
    ) -> bool {
        self.rules
            .iter()
            .find(|rule| rule.matches(notification, context))
            .map(|rule| rule.keep)
            .unwrap_or(false)
    }

    fn from_path(path: PathBuf) -> Result<Self, PolicyLoadError> {
        let input = match fs::read_to_string(&path) {
            Ok(input) => input,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(PolicyLoadError::NotFound);
            }
            Err(error) => {
                return Err(PolicyLoadError::Io {
                    path,
                    source: error,
                });
            }
        };
        Self::from_toml(&input).map_err(|source| PolicyLoadError::Config { path, source })
    }
}

impl Default for NotificationCenterPolicy {
    fn default() -> Self {
        Self {
            rules: vec![
                NotificationCenterRule::new(
                    true,
                    NotificationMatcher {
                        session_locked: Some(true),
                        ..NotificationMatcher::default()
                    },
                ),
                NotificationCenterRule::new(
                    true,
                    NotificationMatcher {
                        is_urgent: Some(true),
                        ..NotificationMatcher::default()
                    },
                ),
                NotificationCenterRule::new(
                    true,
                    NotificationMatcher {
                        actions: Some(ActionPresence::Any),
                        ..NotificationMatcher::default()
                    },
                ),
            ],
        }
    }
}

impl NotificationCenterRule {
    fn new(keep: bool, matcher: NotificationMatcher) -> Self {
        Self { keep, matcher }
    }

    fn from_config(index: usize, config: RuleConfig) -> Result<Self, PolicyConfigError> {
        let matcher = NotificationMatcher {
            is_urgent: config.is_urgent,
            session_locked: config.session_locked,
            title_match: compile_regex(index, "title_match", config.title_match)?,
            content_match: compile_regex(index, "content_match", config.content_match)?,
            app_name_match: compile_regex(index, "app_name_match", config.app_name_match)?,
            app_icon_match: compile_regex(index, "app_icon_match", config.app_icon_match)?,
            actions: config.actions.map(ActionPresence::from),
            action_key_match: compile_regex(index, "action_key_match", config.action_key_match)?,
            action_label_match: compile_regex(
                index,
                "action_label_match",
                config.action_label_match,
            )?,
        };
        Ok(Self::new(config.keep.unwrap_or(true), matcher))
    }

    fn matches(&self, notification: &NotificationView, context: NotificationCenterContext) -> bool {
        self.matcher.matches(notification, context)
    }
}

impl NotificationMatcher {
    fn matches(&self, notification: &NotificationView, context: NotificationCenterContext) -> bool {
        if let Some(is_urgent) = self.is_urgent {
            if (notification.urgency == NotificationUrgency::Critical) != is_urgent {
                return false;
            }
        }
        if let Some(session_locked) = self.session_locked {
            if context.session_locked != session_locked {
                return false;
            }
        }

        if !matches_text(&self.title_match, notification.summary.as_str()) {
            return false;
        }
        if !matches_text(&self.content_match, notification.body.as_str()) {
            return false;
        }
        if !matches_text(&self.app_name_match, notification.app_name.as_str()) {
            return false;
        }
        if !matches_text(&self.app_icon_match, notification.app_icon.as_str()) {
            return false;
        }

        if let Some(actions) = self.actions {
            if !actions.matches(notification) {
                return false;
            }
        }
        if !matches_action(&self.action_key_match, notification, |action| {
            action.key.as_str()
        }) {
            return false;
        }
        if !matches_action(&self.action_label_match, notification, |action| {
            action.label.as_str()
        }) {
            return false;
        }

        true
    }
}

impl ActionPresence {
    fn matches(self, notification: &NotificationView) -> bool {
        match self {
            Self::Any => notification.has_actions(),
            Self::None => !notification.has_actions(),
        }
    }
}

impl From<ActionPresenceConfig> for ActionPresence {
    fn from(config: ActionPresenceConfig) -> Self {
        match config {
            ActionPresenceConfig::Bool(true)
            | ActionPresenceConfig::Named(ActionPresenceName::Any) => Self::Any,
            ActionPresenceConfig::Bool(false)
            | ActionPresenceConfig::Named(ActionPresenceName::None) => Self::None,
        }
    }
}

fn compile_regex(
    rule_index: usize,
    field: &'static str,
    pattern: Option<String>,
) -> Result<Option<Regex>, PolicyConfigError> {
    pattern
        .map(|pattern| {
            Regex::new(pattern.as_str()).map_err(|source| PolicyConfigError::Regex {
                rule_index,
                field,
                source,
            })
        })
        .transpose()
}

fn matches_text(regex: &Option<Regex>, value: &str) -> bool {
    match regex {
        Some(regex) => regex.is_match(value),
        None => true,
    }
}

fn matches_action(
    regex: &Option<Regex>,
    notification: &NotificationView,
    value: impl Fn(&super::model::NotificationAction) -> &str,
) -> bool {
    match regex {
        Some(regex) => notification
            .actions
            .iter()
            .any(|action| regex.is_match(value(action))),
        None => true,
    }
}

#[cfg(test)]
mod test;
