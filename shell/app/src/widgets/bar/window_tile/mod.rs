pub(in crate::widgets::bar) mod agent;
mod app_instance;
mod source;

use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use self::{
    agent::{Agent, State as AgentState},
    source::{Kind, ViewModel, window_tile_vm},
};
use super::{
    WindowNode,
    bar_indicator::{self, BarIndicatorExt},
};
use crate::widgets::nerd_icon::{NerdIcon, NerdIconLabelExt};
use crate::widgets::nerd_icon::{dev, md, seti};

#[derive(Debug)]
#[shell_macros::model(module = window_tile_sources)]
pub(super) struct WindowTile {
    pub window: WindowNode,

    #[source(window_tile_vm(window.clone()))]
    pub vm: Option<ViewModel>,
}

#[shell_macros::component(
    module = window_tile_sources,
    model = WindowTile
)]
#[relm4::component(pub(crate))]
impl SimpleComponent for WindowTile {
    type Init = WindowNode;
    type Input = window_tile_sources::Msg;
    type Output = ();

    view! {
        gtk::Revealer {
            #[watch]
            set_reveal_child: window_visible(&model.vm),
            set_transition_type: gtk::RevealerTransitionType::FadeSlideRight,
            set_transition_duration: 140,
            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Center,
            set_vexpand: false,

            gtk::Overlay {
                set_bar_indicator_size: bar_indicator::SIZE,
                #[watch]
                set_css_classes: &traced_window_tile_classes(&model.vm),

                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,
                set_vexpand: false,

                #[watch]
                set_tooltip_text: model.vm.as_ref().map(|vm| vm.tooltip.as_str()),

                gtk::Label {
                    set_css_classes: &["bar-indicator-icon", "nerdicon"],

                    #[watch]
                    set_nerd_icon: window_icon(&model.vm),
                },

                add_overlay = &gtk::Box {
                    add_css_class: "bar-badge",
                    add_css_class: "agent-unseen-badge",
                    set_can_target: false,
                    set_width_request: 8,
                    set_height_request: 8,
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::Start,

                    #[watch]
                    set_visible: agent_unseen_visible(&model.vm),
                }
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = WindowTile::new(init);
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }
}

fn traced_window_tile_classes(vm: &Option<ViewModel>) -> Vec<&'static str> {
    let _span = tracing::trace_span!(
        "bar.window_tile_classes",
        active = vm.as_ref().is_some_and(|vm| vm.active),
        urgent = vm.as_ref().is_some_and(|vm| vm.urgent),
        has_agent = vm
            .as_ref()
            .is_some_and(|vm| matches!(vm.kind, Kind::Agent(_))),
    )
    .entered();
    window_tile_classes(vm)
}

fn window_tile_classes(vm: &Option<ViewModel>) -> Vec<&'static str> {
    let mut classes = vec!["bar-indicator"];

    let Some(vm) = vm else {
        return classes;
    };

    match &vm.kind {
        Kind::Agent(agent) => {
            classes.push("workspace-window-agent");
            if agent.attention {
                classes.push("attention");
            }
            match agent.state {
                AgentState::None => {}
                AgentState::Idle => classes.push("idle"),
                AgentState::Thinking => classes.push("thinking"),
                AgentState::ToolUse => classes.push("tool-use"),
                AgentState::Compacting => classes.push("compacting"),
            }
        }
        Kind::Plain | Kind::Neovim => {}
    }

    if vm.active {
        classes.push("active");
    }
    if vm.urgent {
        classes.push("urgent");
    }

    classes
}

fn window_visible(vm: &Option<ViewModel>) -> bool {
    vm.is_some()
}

fn window_icon(vm: &Option<ViewModel>) -> NerdIcon {
    vm.as_ref()
        .map(|vm| {
            let resolved = vm.icon.selected_nerd_icon();
            match &vm.kind {
                Kind::Agent(agent) => agent_icon(agent, &resolved),
                Kind::Plain | Kind::Neovim => resolved,
            }
        })
        .unwrap_or_else(NerdIcon::application)
}

fn agent_icon(agent: &Agent, fallback: &NerdIcon) -> NerdIcon {
    if agent.icon.is_empty() {
        fallback.clone()
    } else {
        [agent.icon.as_str(), agent.name.as_str()]
            .into_iter()
            .filter(|hint| !hint.trim().is_empty())
            .find_map(agent_hint_icon)
            .unwrap_or_else(|| NerdIcon::new(md::MD_ROBOT))
    }
}

fn agent_hint_icon(hint: &str) -> Option<NerdIcon> {
    let hint = hint.trim().to_ascii_lowercase();
    let icon = if hint.contains("chrome") || hint.contains("chromium") {
        NerdIcon::new(md::MD_GOOGLE_CHROME)
    } else if hint.contains("slack") {
        NerdIcon::new(dev::DEV_SLACK)
    } else if hint.contains("neovim") || hint.contains("nvim") {
        NerdIcon::new(seti::CUSTOM_NEOVIM)
    } else if hint.contains("ghostty") || hint.contains("terminal") || hint.contains("term") {
        NerdIcon::new(dev::DEV_TERMINAL)
    } else if hint.contains("codex") || hint.contains("agent") || hint.contains("cognition") {
        NerdIcon::new(md::MD_ROBOT)
    } else if hint.contains("workspace") || hint == "workspaces" {
        NerdIcon::workspace()
    } else if hint.contains("folder") || hint.contains("project") {
        NerdIcon::folder()
    } else if hint.contains("git") || hint.contains("branch") || hint.contains("account tree") {
        NerdIcon::branch()
    } else if hint.contains("application") || hint.contains("executable") || hint.contains("window")
    {
        NerdIcon::application()
    } else {
        return None;
    };
    Some(icon)
}

fn agent_unseen_visible(vm: &Option<ViewModel>) -> bool {
    vm.as_ref().is_some_and(|vm| match &vm.kind {
        Kind::Agent(agent) => agent.unseen,
        Kind::Plain | Kind::Neovim => false,
    })
}
