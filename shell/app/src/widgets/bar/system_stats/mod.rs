mod source;

use shell_core::source::Observable;

use crate::widgets::{
    level_indicator::{self, ArcStyle, CurveDirection, LevelRenderStyle, LevelStage, LineStyle},
    nerd_icon::{NerdIcon, md},
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
pub(crate) struct DiskStats {
    pub(super) percent: u8,
    pub(super) used: u64,
    pub(super) free: u64,
    pub(super) total: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SysStatsView {
    pub(super) cpu: u8,
    pub(super) ram: u8,
    pub(super) disk: DiskStats,
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

pub(super) fn track_classes() -> &'static [&'static str] {
    level_indicator::TRACK_CLASSES
}

pub(super) fn level_classes(level: u8) -> Vec<&'static str> {
    level_indicator::level_classes(f64::from(level), LEVEL_MIN, STAGES)
}

pub(super) fn tooltip(stats: &SysStatsView) -> String {
    format!("CPU {}% · RAM {}%", stats.cpu, stats.ram)
}

pub(super) fn disk_icon() -> NerdIcon {
    NerdIcon::new(md::MD_HARDDISK)
}

pub(super) fn disk_tooltip(stats: &DiskStats) -> String {
    format!(
        "Disk {}%\nUsed: {}\nFree: {}\nTotal: {}",
        stats.percent,
        human_bytes(stats.used),
        human_bytes(stats.free),
        human_bytes(stats.total),
    )
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

pub(super) fn line_root_classes() -> Vec<&'static str> {
    level_indicator::root_classes(["line"])
}

pub(super) fn line_track_draw_func()
-> impl Fn(&shell_core::gtk::DrawingArea, &shell_core::gtk::cairo::Context, i32, i32) + 'static {
    level_indicator::track_draw_func(LevelRenderStyle::Line(LineStyle::vertical(3.0)))
}

pub(super) fn line_level_draw_func(
    level: u8,
) -> impl Fn(&shell_core::gtk::DrawingArea, &shell_core::gtk::cairo::Context, i32, i32) + 'static {
    level_indicator::level_draw_func(
        f64::from(level),
        LEVEL_MIN,
        LEVEL_MAX,
        LevelRenderStyle::Line(LineStyle::vertical(3.0)),
    )
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = UNITS[0];
    for next_unit in UNITS.iter().skip(1) {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next_unit;
    }

    if unit == "B" {
        format!("{} {unit}", bytes)
    } else {
        format!("{value:.1} {unit}")
    }
}

fn style(side: ArcSide) -> LevelRenderStyle {
    let curve_direction = match side {
        ArcSide::Start => CurveDirection::Start,
        ArcSide::End => CurveDirection::End,
    };
    LevelRenderStyle::Arc(ArcStyle::vertical(curve_direction))
}
