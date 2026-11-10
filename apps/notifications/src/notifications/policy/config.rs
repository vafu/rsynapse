use std::path::PathBuf;

use serde::Deserialize;

const CONFIG_ENV: &str = "RSYNAPSE_NOTIFICATION_CENTER_CONFIG";
const CONFIG_PATH: &str = "rsynapse/notification-center.toml";

pub(super) fn path() -> PathBuf {
    std::env::var_os(CONFIG_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| config_home().join(CONFIG_PATH))
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct PolicyConfig {
    pub(super) rules: Vec<RuleConfig>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct RuleConfig {
    pub(super) keep: Option<bool>,
    pub(super) is_urgent: Option<bool>,
    pub(super) session_locked: Option<bool>,
    pub(super) title_match: Option<String>,
    pub(super) content_match: Option<String>,
    pub(super) app_name_match: Option<String>,
    pub(super) app_icon_match: Option<String>,
    pub(super) actions: Option<ActionPresenceConfig>,
    pub(super) action_key_match: Option<String>,
    pub(super) action_label_match: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum ActionPresenceConfig {
    Bool(bool),
    Named(ActionPresenceName),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ActionPresenceName {
    Any,
    None,
}
