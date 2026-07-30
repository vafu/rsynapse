use shell_core::source::{self, Observable, rx::Observable as _};
use shell_rx_macros::combine_latest;

use crate::desktop_icon;

use super::super::WindowNode;
use super::{
    agent::{self, Agent},
    app_instance::{AppInstance, app_instance_for_window},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::widgets::bar) enum Kind {
    Plain,
    Neovim,
    Agent(Agent),
}

impl Default for Kind {
    fn default() -> Self {
        Self::Plain
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::widgets::bar) struct ViewModel {
    pub(super) kind: Kind,
    pub(super) icon: String,
    pub(super) tooltip: String,
    pub(super) active: bool,
    pub(super) urgent: bool,
}

pub(super) fn window_tile_vm(window: WindowNode) -> Observable<Option<ViewModel>> {
    let window_id = window.id();
    let app_id = window.app_id().map(|app_id| app_id.and_then(non_empty));
    let active = window.focused();
    let urgent = window.urgent();
    let agent = agent::agent_for_window(window.clone());
    let app_instance = source::switch_map(window.id().box_it(), app_instance_for_window);

    combine_latest!(
        window_id,
        app_id,
        active,
        urgent,
        agent,
        app_instance
            => move |(window_id, app_id, active, urgent, agent, app_instance)| {
                let _span = tracing::trace_span!(
                    "bar.window_tile_vm",
                    window_id,
                    active,
                    urgent,
                    has_agent = agent.is_some(),
                )
                .entered();
                let app_id = app_id.unwrap_or_default();
                let app_label = app_instance
                    .name
                    .clone()
                    .unwrap_or_else(|| app_id.clone());
                Some(ViewModel {
                    tooltip: window_tooltip(&app_label, agent.as_ref()),
                    kind: window_kind(&app_label, agent),
                    icon: window_icon(&app_id, &app_instance),
                    active,
                    urgent,
                })
            },
    )
    .distinct_until_changed()
    .box_it()
}

fn window_icon(app_id: &str, app_instance: &AppInstance) -> String {
    app_instance
        .icon
        .clone()
        .and_then(non_empty)
        .or_else(|| {
            app_instance
                .name
                .as_deref()
                .map(desktop_icon::icon_for_app_id)
                .and_then(non_empty)
        })
        .or_else(|| non_empty(desktop_icon::icon_for_app_id(app_id)))
        .unwrap_or_default()
}

fn window_kind(app_id: &str, agent: Option<Agent>) -> Kind {
    if let Some(agent) = agent {
        return Kind::Agent(agent);
    }

    let app_id = app_id.to_ascii_lowercase();
    if app_id.contains("nvim") || app_id.contains("neovim") {
        Kind::Neovim
    } else {
        Kind::Plain
    }
}

fn window_tooltip(app_id: &str, agent: Option<&Agent>) -> String {
    let label = if app_id.is_empty() { "Window" } else { app_id };
    if let Some(agent) = agent {
        return [label.to_owned(), format!("Agent: {:?}", agent.state)].join("\n");
    }

    label.to_owned()
}

pub(super) fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}
