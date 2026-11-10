use shell_core::gtk;

use super::source::{Kind, ViewModel};
use crate::widgets::level_indicator::{self, LevelRenderStyle, LevelStage, LineStyle};

const STYLE: LevelRenderStyle = LevelRenderStyle::Line(LineStyle::vertical(3.0));
const STAGES: &[LevelStage] = &[
    LevelStage {
        level: 0.0,
        class: "normal",
    },
    LevelStage {
        level: 80.0,
        class: "warn",
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

pub(super) fn percent(vm: &Option<ViewModel>) -> Option<u8> {
    match &vm.as_ref()?.kind {
        Kind::Agent(agent) => agent.context_pct,
        Kind::Plain | Kind::Neovim => None,
    }
}

pub(super) fn root_classes() -> Vec<&'static str> {
    level_indicator::root_classes(["line", "agent-context-level"])
}

pub(super) fn level_classes(vm: &Option<ViewModel>) -> Vec<&'static str> {
    level_indicator::level_classes(f64::from(percent(vm).unwrap_or(0)), 0.0, STAGES)
}

pub(super) fn track_draw_func()
-> impl Fn(&gtk::DrawingArea, &gtk::cairo::Context, i32, i32) + 'static {
    level_indicator::track_draw_func(STYLE)
}

pub(super) fn level_draw_func(
    vm: &Option<ViewModel>,
) -> impl Fn(&gtk::DrawingArea, &gtk::cairo::Context, i32, i32) + 'static {
    level_indicator::level_draw_func(f64::from(percent(vm).unwrap_or(0)), 0.0, 100.0, STYLE)
}
