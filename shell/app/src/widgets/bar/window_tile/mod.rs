pub(in crate::widgets::bar) mod agent;
mod source;

use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use self::{
    agent::{Agent, State as AgentState},
    source::{Kind, ViewModel, window_tile_vm},
};
use super::{PANEL_ICON_SIZE, WindowNode, app_icon, bzbus};
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
        gtk::Overlay {
            #[watch]
            set_visible: model.vm.is_some(),

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

                    // Build status icon rendering is paused while the status is moved to a new surface.
                    // #[local_ref]
                    // build_indicator -> gtk::Image {
                    //     #[watch]
                    //     set_build_indicator_state: agent_build_indicator_state(&model.vm),
                    // }
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
            },

            add_overlay = &gtk::DrawingArea {
                #[watch]
                set_visible: build_progress_visible(&model.vm),
                set_css_classes: bzbus::progress_track_classes(),
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,
                set_hexpand: true,
                set_vexpand: true,
                set_can_target: false,
                set_draw_func: bzbus::progress_track_draw_func(),
            },

            add_overlay = &gtk::DrawingArea {
                #[watch]
                set_visible: build_progress_visible(&model.vm),
                #[watch]
                set_css_classes: &build_progress_level_classes(&model.vm),
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,
                set_hexpand: true,
                set_vexpand: true,
                set_can_target: false,
                #[watch]
                set_draw_func: bzbus::progress_level_draw_func(build_progress_percent(&model.vm)),
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = WindowTile::new(init);
        // Build status icon rendering is paused while the status is moved to a new surface.
        // let build_indicator = build_indicator::image();
        // build_indicator.set_build_indicator_state(agent_build_indicator_state(&model.vm));
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
        has_build = vm.as_ref().is_some_and(|vm| vm.build.is_some()),
    )
    .entered();
    window_tile_classes(vm)
}

fn window_tile_classes(vm: &Option<ViewModel>) -> Vec<&'static str> {
    let mut classes = vec!["bar-indicator"];
    if build_progress_visible(vm) {
        classes.push("build-progress");
    }

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
        // Build status icon rendering is paused while the status is moved to a new surface.
        // Kind::Build(build) => {
        //     classes.push("workspace-window-build");
        //     classes.extend(build_state_classes(build));
        // }
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

fn window_icon_name(vm: &Option<ViewModel>) -> Option<String> {
    vm.as_ref().map(|vm| match &vm.kind {
        Kind::Agent(agent) => agent_icon(agent, &vm.icon),
        // Build status icon rendering is paused while the status is moved to a new surface.
        // Kind::Build(build) => material_icon::icon_name(build.icon),
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

// Build status icon rendering is paused while the status is moved to a new surface.
// fn agent_build_indicator_state(vm: &Option<ViewModel>) -> BuildIndicatorState {
//     let Some(vm) = vm else {
//         return BuildIndicatorState::None;
//     };
//     if !matches!(vm.kind, Kind::Agent(_)) {
//         return BuildIndicatorState::None;
//     }
//     vm.build
//         .as_ref()
//         .map(build_indicator_state)
//         .unwrap_or_default()
// }
//
// fn build_indicator_state(build: &bzbus::BzBusView) -> BuildIndicatorState {
//     if build_has_state(build, "failed") {
//         BuildIndicatorState::Failed
//     } else if build_has_state(build, "running") {
//         BuildIndicatorState::Running
//     } else if build_has_state(build, "finished") {
//         BuildIndicatorState::Finished
//     } else {
//         BuildIndicatorState::None
//     }
// }

// Build status icon rendering is paused while the status is moved to a new surface.
// fn build_state_classes(build: &bzbus::BzBusView) -> impl Iterator<Item = &'static str> + '_ {
//     build.classes.iter().copied().filter(|class| {
//         matches!(
//             *class,
//             "idle" | "offline" | "running" | "failed" | "finished"
//         )
//     })
// }
//
// fn build_has_state(build: &bzbus::BzBusView, state: &str) -> bool {
//     build.classes.iter().any(|class| class == &state)
// }

fn build_progress_visible(_vm: &Option<ViewModel>) -> bool {
    // Build status icon rendering is paused while the status is moved to a new surface.
    // _vm.as_ref().is_some_and(|vm| match &vm.kind {
    //     Kind::Build(build) => build.progress_visible,
    //     Kind::Agent(_) | Kind::Plain | Kind::Neovim => false,
    // })
    false
}

fn build_progress_percent(_vm: &Option<ViewModel>) -> u8 {
    // Build status icon rendering is paused while the status is moved to a new surface.
    // _vm.as_ref()
    //     .and_then(|vm| match &vm.kind {
    //         Kind::Build(build) => Some(build.progress_percent),
    //         Kind::Agent(_) | Kind::Plain | Kind::Neovim => None,
    //     })
    //     .unwrap_or(0)
    0
}

fn build_progress_level_classes(_vm: &Option<ViewModel>) -> Vec<&'static str> {
    // Build status icon rendering is paused while the status is moved to a new surface.
    // _vm.as_ref()
    //     .and_then(|vm| match &vm.kind {
    //         Kind::Build(build) => Some(build.progress_level_classes.clone()),
    //         Kind::Agent(_) | Kind::Plain | Kind::Neovim => None,
    //     })
    //     .unwrap_or_else(|| vec!["level", "idle"])
    vec!["level", "idle"]
}
