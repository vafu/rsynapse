use std::f64::consts::PI;

use shell_core::gtk::{self, prelude::*};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum LevelOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum LevelDirection {
    Standard,
    Inverted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CurveDirection {
    Start,
    End,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LevelStage {
    pub(crate) level: f64,
    pub(crate) class: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum LevelRenderStyle {
    Line(LineStyle),
    Arc(ArcStyle),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LineStyle {
    pub(crate) orientation: LevelOrientation,
    pub(crate) direction: LevelDirection,
    pub(crate) thickness: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ArcStyle {
    pub(crate) orientation: LevelOrientation,
    pub(crate) direction: LevelDirection,
    pub(crate) curve_direction: CurveDirection,
    pub(crate) thickness: f64,
    pub(crate) radius: f64,
}

impl LineStyle {
    pub(crate) const fn vertical(thickness: f64) -> Self {
        Self {
            orientation: LevelOrientation::Vertical,
            direction: LevelDirection::Standard,
            thickness,
        }
    }
}

impl ArcStyle {
    pub(crate) const fn vertical(curve_direction: CurveDirection) -> Self {
        Self {
            orientation: LevelOrientation::Vertical,
            direction: LevelDirection::Standard,
            curve_direction,
            thickness: 3.0,
            radius: 16.0,
        }
    }
}

pub(crate) const TRACK_CLASSES: &[&str] = &["track"];

pub(crate) fn root_classes(extra: impl IntoIterator<Item = &'static str>) -> Vec<&'static str> {
    let mut classes = vec!["levelindicator"];
    classes.extend(extra);
    classes
}

pub(crate) fn level_classes(level: f64, min: f64, stages: &[LevelStage]) -> Vec<&'static str> {
    vec!["level", stage_class(level, min, stages)]
}

pub(crate) fn track_draw_func(
    style: LevelRenderStyle,
) -> impl Fn(&gtk::DrawingArea, &gtk::cairo::Context, i32, i32) + 'static {
    move |area, cr, width, height| draw(area, cr, width, height, 1.0, style)
}

pub(crate) fn level_draw_func(
    level: f64,
    min: f64,
    max: f64,
    style: LevelRenderStyle,
) -> impl Fn(&gtk::DrawingArea, &gtk::cairo::Context, i32, i32) + 'static {
    move |area, cr, width, height| {
        let fraction = fraction(level, min, max);
        draw(area, cr, width, height, fraction, style);
    }
}

fn stage_class(level: f64, min: f64, stages: &[LevelStage]) -> &'static str {
    let value = level.clamp(min, f64::MAX) - min;
    stages
        .iter()
        .filter(|stage| value >= stage.level)
        .map(|stage| stage.class)
        .last()
        .unwrap_or("default")
}

fn fraction(level: f64, min: f64, max: f64) -> f64 {
    let range = max - min;
    let clamped = level.clamp(min, max);
    if range > 0.0 {
        ((clamped - min) / range).clamp(0.0, 1.0)
    } else if clamped >= max {
        1.0
    } else {
        0.0
    }
}

fn draw(
    area: &gtk::DrawingArea,
    cr: &gtk::cairo::Context,
    width: i32,
    height: i32,
    fraction: f64,
    style: LevelRenderStyle,
) {
    if width <= 0 || height <= 0 {
        return;
    }

    let color = area.style_context().color();
    match style {
        LevelRenderStyle::Line(style) => draw_line(cr, width, height, fraction, style, &color),
        LevelRenderStyle::Arc(style) => draw_arc(cr, width, height, fraction, style, &color),
    }
}

fn draw_line(
    cr: &gtk::cairo::Context,
    width: i32,
    height: i32,
    fraction: f64,
    style: LineStyle,
    color: &gtk::gdk::RGBA,
) {
    if fraction <= 0.0 {
        return;
    }

    let width = f64::from(width);
    let height = f64::from(height);
    let thickness = style.thickness.max(1.0);
    let half_thickness = thickness / 2.0;
    let (x1, y1, x2, y2, level_length) = match style.orientation {
        LevelOrientation::Horizontal => {
            let track_start = half_thickness;
            let track_end = width - half_thickness;
            if track_start >= track_end {
                return;
            }

            let y = height / 2.0;
            let length = track_end - track_start;
            let level_length = length * fraction;
            match style.direction {
                LevelDirection::Standard => {
                    (track_start, y, track_start + level_length, y, level_length)
                }
                LevelDirection::Inverted => {
                    (track_end - level_length, y, track_end, y, level_length)
                }
            }
        }
        LevelOrientation::Vertical => {
            let track_start = half_thickness;
            let track_end = height - half_thickness;
            if track_start >= track_end {
                return;
            }

            let x = width / 2.0;
            let length = track_end - track_start;
            let level_length = length * fraction;
            match style.direction {
                LevelDirection::Inverted => {
                    (x, track_start, x, track_start + level_length, level_length)
                }
                LevelDirection::Standard => {
                    (x, track_end - level_length, x, track_end, level_length)
                }
            }
        }
    };

    if level_length <= 0.0 {
        return;
    }

    cr.set_line_width(thickness);
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    set_source_rgba(cr, color);
    cr.move_to(x1, y1);
    cr.line_to(x2, y2);
    let _ = cr.stroke();
}

fn draw_arc(
    cr: &gtk::cairo::Context,
    width: i32,
    height: i32,
    fraction: f64,
    style: ArcStyle,
    color: &gtk::gdk::RGBA,
) {
    if fraction <= 0.0 {
        return;
    }

    let width = f64::from(width);
    let height = f64::from(height);
    let radius = style.radius.max(1.0);
    let thickness = style.thickness.max(1.0);
    let curve_direction_modifier = match style.curve_direction {
        CurveDirection::Start => -1.0,
        CurveDirection::End => 1.0,
    };
    let direction_modifier = match style.direction {
        LevelDirection::Standard => 1.0,
        LevelDirection::Inverted => -1.0,
    };
    let sweep_direction = curve_direction_modifier * direction_modifier;
    let half_length = match style.orientation {
        LevelOrientation::Horizontal => width / 2.0 - thickness,
        LevelOrientation::Vertical => height / 2.0 - thickness,
    }
    .max(0.0);
    let asin_arg = (half_length / radius).clamp(-1.0, 1.0);
    let arc_span = if radius <= half_length {
        PI
    } else {
        2.0 * asin_arg.asin()
    };
    if !arc_span.is_finite() || arc_span <= 0.0 {
        return;
    }

    let (center_x, center_y) = match style.orientation {
        LevelOrientation::Horizontal => (
            width / 2.0,
            height / 2.0 + radius * curve_direction_modifier,
        ),
        LevelOrientation::Vertical => (
            width / 2.0 + radius * curve_direction_modifier,
            height / 2.0,
        ),
    };
    let mut start_angle = match style.orientation {
        LevelOrientation::Horizontal => 0.5 * PI,
        LevelOrientation::Vertical => 0.0,
    };
    if style.curve_direction == CurveDirection::End {
        start_angle += PI;
    }

    let half_arc = (arc_span / 2.0) * sweep_direction;
    let arc_start = start_angle - half_arc;
    let level_end = arc_start + arc_span * fraction * sweep_direction;

    cr.set_line_width(thickness);
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    set_source_rgba(cr, color);
    if sweep_direction > 0.0 {
        cr.arc(center_x, center_y, radius, arc_start, level_end);
    } else {
        cr.arc_negative(center_x, center_y, radius, arc_start, level_end);
    }
    let _ = cr.stroke();
}

fn set_source_rgba(cr: &gtk::cairo::Context, color: &gtk::gdk::RGBA) {
    cr.set_source_rgba(
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        f64::from(color.alpha()),
    );
}
