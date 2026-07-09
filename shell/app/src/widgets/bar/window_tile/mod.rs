pub(in crate::widgets::bar) mod agent;
mod source;

use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use self::{
    agent::{Agent, State as AgentState},
    source::{Kind, ViewModel, window_tile_vm},
};
use super::WindowNode;
use crate::widgets::{
    BACKGROUND_BLUR_CLASS,
    level_indicator::{self, LevelRenderStyle, LevelStage, LineStyle},
    material_icon,
};

const CONTEXT_STYLE: LevelRenderStyle = LevelRenderStyle::Line(LineStyle::vertical(3.0));
const CONTEXT_STAGES: &[LevelStage] = &[
    LevelStage {
        level: 0.0,
        class: "normal",
    },
    LevelStage {
        level: 50.0,
        class: "warn",
    },
    LevelStage {
        level: 75.0,
        class: "high",
    },
    LevelStage {
        level: 90.0,
        class: "danger",
    },
    LevelStage {
        level: 95.0,
        class: "critical",
    },
];

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
        gtk::Overlay {
            #[watch]
            set_visible: model.vm.is_some(),

            add_css_class: "workspace-window-frame",
            add_css_class: BACKGROUND_BLUR_CLASS,

            set_halign: gtk::Align::Center,
            set_valign: gtk::Align::Fill,
            set_vexpand: true,

            #[watch]
            set_tooltip_text: model.vm.as_ref().map(|vm| vm.tooltip.as_str()),

            gtk::Box {
                #[watch]
                set_css_classes: &traced_window_tile_classes(&model.vm),
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Fill,
                set_vexpand: true,

                gtk::Image {
                    #[watch]
                    set_visible: !is_agent(&model.vm),

                    #[watch]
                    set_icon_name: window_icon_name(&model.vm).as_deref(),
                },

                gtk::Box {
                    add_css_class: "agent-inner",
                    set_valign: gtk::Align::Fill,
                    set_vexpand: true,

                    #[watch]
                    set_visible: is_agent(&model.vm),

                    gtk::Image {
                        add_css_class: "materialicon",

                        #[watch]
                        set_icon_name: window_icon_name(&model.vm).as_deref(),
                    },

                    gtk::Overlay {
                        #[watch]
                        set_css_classes: &context_indicator_root_classes(),
                        set_valign: gtk::Align::Fill,
                        set_vexpand: true,

                        add_overlay = &gtk::DrawingArea {
                            set_css_classes: level_indicator::TRACK_CLASSES,
                            set_content_width: 8,
                            set_vexpand: true,
                            set_valign: gtk::Align::Fill,
                            set_draw_func: level_indicator::track_draw_func(CONTEXT_STYLE),
                        },

                        add_overlay = &gtk::DrawingArea {
                            #[watch]
                            set_css_classes: &context_indicator_level_classes(context_pct(&model.vm)),
                            set_content_width: 8,
                            set_vexpand: true,
                            set_valign: gtk::Align::Fill,
                            #[watch]
                            set_draw_func: level_indicator::level_draw_func(
                                f64::from(context_pct(&model.vm)),
                                0.0,
                                100.0,
                                CONTEXT_STYLE,
                            ),
                        }
                    }
                }
            },

            add_overlay = &gtk::Box {
                add_css_class: "barblock-badge",
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
    let Some(vm) = vm else {
        return vec![
            "workspace-window-content",
            "workspace-window-tile",
            "workspace-window-plain",
        ];
    };

    let mut classes = vec!["workspace-window-content", "workspace-window-tile"];
    classes.push(match vm.kind {
        Kind::Plain => "workspace-window-plain",
        Kind::Neovim => "workspace-window-neovim",
        Kind::Agent(_) => "workspace-window-agent",
    });

    if let Kind::Agent(agent) = &vm.kind {
        classes.push("agent-window");
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

    if vm.active {
        classes.push("active");
    }
    if vm.urgent {
        classes.push("urgent");
    }

    classes
}

fn window_icon_name(vm: &Option<ViewModel>) -> Option<String> {
    vm.as_ref().map(|vm| match &vm.kind {
        Kind::Agent(agent) => agent_icon(agent, &vm.icon),
        Kind::Plain | Kind::Neovim => vm.icon.clone(),
    })
}

fn agent_icon(agent: &Agent, fallback: &str) -> String {
    if agent.icon.is_empty() {
        fallback.to_owned()
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

fn context_pct(vm: &Option<ViewModel>) -> u32 {
    vm.as_ref()
        .and_then(|vm| match &vm.kind {
            Kind::Agent(agent) => Some(agent.context_pct),
            Kind::Plain | Kind::Neovim => None,
        })
        .unwrap_or(0)
}

fn context_indicator_root_classes() -> Vec<&'static str> {
    level_indicator::root_classes(["line", "agent-context-indicator"])
}

fn context_indicator_level_classes(context_pct: u32) -> Vec<&'static str> {
    level_indicator::level_classes(f64::from(context_pct), 0.0, CONTEXT_STAGES)
}
