use shell_core::source::{self, Observable, rx::Observable as _};
use shell_rx_macros::combine_latest;

use crate::desktop_icon;

use super::super::{
    WindowNode,
    icon_resolver::{
        IconChoice, IconEvidence, IconEvidenceKind, IconPolicy, IconRequest, IconResolution,
        resolve_icon,
    },
};
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
    pub(super) icon: IconResolution,
    pub(super) tooltip: String,
    pub(super) active: bool,
    pub(super) urgent: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaseViewModel {
    kind: Kind,
    tooltip: String,
    active: bool,
    urgent: bool,
    icon_request: IconRequest,
}

pub(super) fn window_tile_vm(window: WindowNode) -> Observable<Option<ViewModel>> {
    let window_id = window.id();
    let app_id = window.app_id().map(|app_id| app_id.and_then(non_empty));
    let active = window.focused();
    let urgent = window.urgent();
    let agent = agent::agent_for_window(window.clone());
    let app_instance = source::switch_map(window.id().box_it(), app_instance_for_window);

    let base = combine_latest!(
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
                BaseViewModel {
                    tooltip: window_tooltip(&app_label, agent.as_ref()),
                    kind: window_kind(&app_id, agent),
                    icon_request: window_icon_request(&app_id, &app_instance),
                    active,
                    urgent,
                }
            },
    )
    .distinct_until_changed()
    .box_it();

    source::switch_map(base, |base| {
        let request = base.icon_request.clone();
        resolve_icon(request)
            .map(move |icon| Some(base.view_model(icon)))
            .distinct_until_changed()
            .box_it()
    })
    .distinct_until_changed()
    .box_it()
}

impl BaseViewModel {
    fn view_model(&self, icon: IconResolution) -> ViewModel {
        ViewModel {
            kind: self.kind.clone(),
            icon,
            tooltip: self.tooltip.clone(),
            active: self.active,
            urgent: self.urgent,
        }
    }
}

fn window_icon_request(app_id: &str, app_instance: &AppInstance) -> IconRequest {
    let desktop_icon = app_instance
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
        .or_else(|| non_empty(desktop_icon::icon_for_app_id(app_id)));

    let mut evidence = Vec::new();
    push_evidence(&mut evidence, IconEvidenceKind::AppId, app_id);
    if let Some(name) = app_instance.name.as_deref() {
        push_evidence(&mut evidence, IconEvidenceKind::DesktopName, name);
    }
    if let Some(icon) = app_instance.icon.as_deref() {
        push_evidence(&mut evidence, IconEvidenceKind::DesktopIcon, icon);
    }
    if let Some(icon) = desktop_icon.as_deref() {
        push_evidence(&mut evidence, IconEvidenceKind::DesktopIcon, icon);
    }

    IconRequest::new(
        "window-app-icon",
        IconChoice::application_fallback(),
        IconPolicy::window_app(),
        evidence,
    )
}

fn push_evidence(evidence: &mut Vec<IconEvidence>, kind: IconEvidenceKind, value: &str) {
    if let Some(value) = IconEvidence::new(kind, value) {
        evidence.push(value);
    }
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
