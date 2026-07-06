mod source;

use shell_core::source::Observable;

use crate::widgets::level_indicator::{
    self, ArcStyle, CurveDirection, LevelRenderStyle, LevelStage,
};

const LEVEL_MIN: f64 = 0.0;
const LEVEL_MAX: f64 = 100.0;
const STAGES: &[LevelStage] = &[
    LevelStage {
        level: 0.0,
        class: "normal",
    },
    LevelStage {
        level: 35.0,
        class: "warn",
    },
    LevelStage {
        level: 50.0,
        class: "high",
    },
    LevelStage {
        level: 80.0,
        class: "danger",
    },
    LevelStage {
        level: 90.0,
        class: "critical",
    },
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SysStatsView {
    pub(super) cpu: u8,
    pub(super) ram: u8,
}

pub(super) fn sys_stats() -> Observable<SysStatsView> {
    source::sys_stats()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ArcSide {
    Start,
    End,
}

pub(super) fn arc_root_classes() -> Vec<&'static str> {
    level_indicator::root_classes(["arc", "battery"])
}

pub(super) fn level_classes(level: u8) -> Vec<&'static str> {
    level_indicator::level_classes(f64::from(level), LEVEL_MIN, STAGES)
}

pub(super) fn tooltip(stats: &SysStatsView) -> String {
    format!("CPU {}% · RAM {}%", stats.cpu, stats.ram)
}

pub(super) fn track_draw_func(
    side: ArcSide,
) -> impl Fn(&shell_core::gtk::DrawingArea, &shell_core::gtk::cairo::Context, i32, i32) + 'static {
    level_indicator::track_draw_func(style(side))
}

pub(super) fn level_draw_func(
    level: u8,
    side: ArcSide,
) -> impl Fn(&shell_core::gtk::DrawingArea, &shell_core::gtk::cairo::Context, i32, i32) + 'static {
    level_indicator::level_draw_func(f64::from(level), LEVEL_MIN, LEVEL_MAX, style(side))
}

fn style(side: ArcSide) -> LevelRenderStyle {
    let curve_direction = match side {
        ArcSide::Start => CurveDirection::Start,
        ArcSide::End => CurveDirection::End,
    };
    LevelRenderStyle::Arc(ArcStyle::vertical(curve_direction))
}
