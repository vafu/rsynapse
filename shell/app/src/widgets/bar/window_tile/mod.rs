pub(in crate::widgets::bar) mod agent;
mod app_instance;
mod source;

use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use self::{
    agent::{Agent, State as AgentState},
    source::{Kind, ViewModel, window_tile_vm},
};
use super::{PANEL_ICON_SIZE, WindowNode, app_icon};
use crate::widgets::material_icon;

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
                #[watch]
                set_css_classes: &traced_window_tile_classes(&model.vm),

                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,
                set_vexpand: false,

                #[watch]
                set_tooltip_text: model.vm.as_ref().map(|vm| vm.tooltip.as_str()),

                gtk::Box {
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    set_vexpand: false,
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 1,

                    gtk::Image {
                        add_css_class: "bar-indicator-icon",
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        set_hexpand: true,
                        set_pixel_size: PANEL_ICON_SIZE,

                        #[watch]
                        set_visible: !is_agent(&model.vm),

                        #[watch]
                        set_icon_name: window_icon_name(&model.vm).as_deref(),
                    },

                    gtk::Box {
                        add_css_class: "agent-inner",
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        set_vexpand: false,
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 1,

                        #[watch]
                        set_visible: is_agent(&model.vm),

                        gtk::Image {
                            add_css_class: "materialicon",
                            add_css_class: "bar-indicator-icon",
                            set_halign: gtk::Align::Center,
                            set_valign: gtk::Align::Center,
                            set_pixel_size: PANEL_ICON_SIZE,

                            #[watch]
                            set_icon_name: window_icon_name(&model.vm).as_deref(),
                        },
                    }
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

fn window_icon_name(vm: &Option<ViewModel>) -> Option<String> {
    vm.as_ref().map(|vm| match &vm.kind {
        Kind::Agent(agent) => agent_icon(agent, &vm.icon),
        Kind::Plain | Kind::Neovim => app_icon::icon_name(&vm.icon),
    })
}

fn agent_icon(agent: &Agent, fallback: &str) -> String {
    if agent.icon.is_empty() {
        app_icon::icon_name(fallback)
    } else {
        material_icon::icon_name(&agent.icon)
    }
}

fn is_agent(vm: &Option<ViewModel>) -> bool {
    vm.as_ref()
        .is_some_and(|vm| matches!(vm.kind, Kind::Agent(_)))
}

fn agent_unseen_visible(vm: &Option<ViewModel>) -> bool {
    vm.as_ref().is_some_and(|vm| match &vm.kind {
        Kind::Agent(agent) => agent.unseen,
        Kind::Plain | Kind::Neovim => false,
    })
}
