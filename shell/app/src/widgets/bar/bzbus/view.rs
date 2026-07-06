use std::{
    f64::consts::PI,
    time::{SystemTime, UNIX_EPOCH},
};

use shell_core::gtk::{self, prelude::*};

use crate::widgets::BACKGROUND_BLUR_CLASS;

const ACTIVE_STALE_MS: i64 = 2 * 60 * 60 * 1000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BzBusView {
    pub(in crate::widgets::bar) classes: Vec<&'static str>,
    pub(in crate::widgets::bar) tooltip: String,
    pub(in crate::widgets::bar) icon: &'static str,
    pub(in crate::widgets::bar) progress_level_classes: Vec<&'static str>,
    pub(in crate::widgets::bar) progress_percent: u8,
    pub(in crate::widgets::bar) progress_visible: bool,
}

impl Default for BzBusView {
    fn default() -> Self {
        Self {
            classes: classes_for(false, None),
            tooltip: "bzbus offline".to_owned(),
            icon: "cloud_off",
            progress_level_classes: progress_level_classes_for(false, None),
            progress_percent: 0,
            progress_visible: false,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Invocation {
    pub(super) id: String,
    pub(super) build_id: String,
    pub(super) component: String,
    pub(super) source: String,
    pub(super) last_sequence: i64,
    pub(super) status: String,
    pub(super) outcome: String,
    pub(super) command_name: String,
    pub(super) started_at_unix_ms: i64,
    pub(super) ended_at_unix_ms: i64,
    pub(super) progress_completed: u32,
    pub(super) progress_total: u32,
    pub(super) actions_completed: u32,
    pub(super) total_actions: u64,
    pub(super) actions_failed: u32,
    pub(super) running_actions: u32,
}

pub(super) fn view(active: bool, mut invocations: Vec<Invocation>) -> BzBusView {
    if !active {
        return BzBusView::default();
    }

    invocations.sort_by(compare_invocations);
    let invocation = invocations.first();
    BzBusView {
        classes: classes_for(true, invocation),
        tooltip: tooltip(invocation),
        icon: icon_for(active, invocation),
        progress_level_classes: progress_level_classes_for(true, invocation),
        progress_percent: progress_percent(invocation).unwrap_or(0),
        progress_visible: progress_percent(invocation).is_some(),
    }
}

fn compare_invocations(left: &Invocation, right: &Invocation) -> std::cmp::Ordering {
    is_active(right)
        .cmp(&is_active(left))
        .then_with(|| observed_time(right).cmp(&observed_time(left)))
        .then_with(|| right.last_sequence.cmp(&left.last_sequence))
        .then_with(|| right.id.cmp(&left.id))
}

fn is_active(invocation: &Invocation) -> bool {
    !is_ended(invocation)
        && !is_stale(invocation)
        && !matches!(
            normalized(invocation.status.as_str()).as_str(),
            "finished" | "failed"
        )
}

fn is_failed(invocation: &Invocation) -> bool {
    matches!(
        normalized(invocation.status.as_str()).as_str(),
        "failed" | "failure" | "error"
    ) || matches!(
        normalized(invocation.outcome.as_str()).as_str(),
        "failed" | "failure" | "error"
    )
}

fn is_finished(invocation: &Invocation) -> bool {
    matches!(
        normalized(invocation.status.as_str()).as_str(),
        "finished" | "success"
    ) || matches!(
        normalized(invocation.outcome.as_str()).as_str(),
        "finished" | "success"
    )
}

fn is_ended(invocation: &Invocation) -> bool {
    invocation.ended_at_unix_ms > 0 || is_failed(invocation) || is_finished(invocation)
}

fn is_stale(invocation: &Invocation) -> bool {
    let last_observed = observed_time(invocation);
    last_observed > 0 && now_unix_ms() - last_observed > ACTIVE_STALE_MS
}

fn observed_time(invocation: &Invocation) -> i64 {
    invocation
        .ended_at_unix_ms
        .max(invocation.started_at_unix_ms)
}

fn display_status(invocation: &Invocation) -> String {
    if is_failed(invocation) {
        return "failed".to_owned();
    }
    if is_finished(invocation) {
        return "finished".to_owned();
    }
    if invocation.ended_at_unix_ms > 0 {
        return "ended".to_owned();
    }
    if is_stale(invocation) {
        return "stale".to_owned();
    }
    non_empty(invocation.status.as_str())
        .unwrap_or("unknown")
        .to_owned()
}

fn progress_percent(invocation: Option<&Invocation>) -> Option<u8> {
    let invocation = invocation?;
    (invocation.progress_total > 0).then(|| {
        let percent =
            f64::from(invocation.progress_completed) * 100.0 / f64::from(invocation.progress_total);
        percent.round().clamp(0.0, 100.0) as u8
    })
}

fn tooltip(invocation: Option<&Invocation>) -> String {
    let Some(invocation) = invocation else {
        return "bzbus connected · no active build".to_owned();
    };

    let mut lines = vec![
        format!(
            "status: {} ({})",
            display_status(invocation),
            non_empty(invocation.outcome.as_str()).unwrap_or("unknown")
        ),
        format!("elapsed: {}", elapsed_text(invocation)),
        format!("command: {}", command_text(invocation)),
        format!(
            "actions: {} completed, {} total, {} failed, {} running",
            invocation.actions_completed,
            invocation.total_actions,
            invocation.actions_failed,
            invocation.running_actions
        ),
    ];
    if invocation.progress_total > 0 {
        lines.push(format!("progress: {}", progress_text(invocation)));
    }
    if let Some(component) = non_empty(invocation.component.as_str()) {
        lines.push(format!("component: {component}"));
    }
    if let Some(source) = non_empty(invocation.source.as_str()) {
        lines.push(format!("source: {source}"));
    }
    lines.push(format!("sequence: {}", invocation.last_sequence));
    lines.push(format!("invocation: {}", fallback(invocation.id.as_str())));
    lines.push(format!("build: {}", fallback(invocation.build_id.as_str())));
    lines.join("\n")
}

fn icon_for(active: bool, invocation: Option<&Invocation>) -> &'static str {
    let Some(invocation) = invocation else {
        return if active { "construction" } else { "cloud_off" };
    };
    if is_failed(invocation) {
        "error"
    } else if is_finished(invocation) {
        "check_circle"
    } else {
        "build_circle"
    }
}

fn classes_for(active: bool, invocation: Option<&Invocation>) -> Vec<&'static str> {
    let mut classes = vec!["barblock", BACKGROUND_BLUR_CLASS, "bzbus-widget"];
    classes.push(state_class_for(active, invocation));
    classes
}

fn progress_level_classes_for(active: bool, invocation: Option<&Invocation>) -> Vec<&'static str> {
    vec!["level", state_class_for(active, invocation)]
}

fn state_class_for(active: bool, invocation: Option<&Invocation>) -> &'static str {
    if !active {
        "offline"
    } else if let Some(invocation) = invocation {
        if is_failed(invocation) {
            "failed"
        } else if is_finished(invocation) {
            "finished"
        } else if is_ended(invocation) || is_stale(invocation) {
            "idle"
        } else {
            "running"
        }
    } else {
        "idle"
    }
}

fn command_text(invocation: &Invocation) -> &str {
    non_empty(invocation.command_name.as_str()).unwrap_or("unknown")
}

fn progress_text(invocation: &Invocation) -> String {
    let mut text = format!(
        "{}/{}",
        invocation.progress_completed, invocation.progress_total
    );
    if invocation.actions_completed > 0 {
        text.push_str(format!(" · {}a", invocation.actions_completed).as_str());
    }
    if invocation.running_actions > 0 {
        text.push_str(format!("/{}r", invocation.running_actions).as_str());
    }
    text
}

fn elapsed_text(invocation: &Invocation) -> String {
    if invocation.started_at_unix_ms <= 0 {
        return "unknown".to_owned();
    }
    duration_text(invocation_end(invocation) - invocation.started_at_unix_ms)
}

fn invocation_end(invocation: &Invocation) -> i64 {
    if invocation.ended_at_unix_ms > 0 {
        invocation.ended_at_unix_ms
    } else {
        now_unix_ms()
    }
}

const PROGRESS_TRACK_CLASSES: &[&str] = &["track"];
const PROGRESS_PERIMETER_THICKNESS: f64 = 2.0;
const PROGRESS_PERIMETER_RADIUS: f64 = 12.0;

pub(in crate::widgets::bar) fn progress_track_classes() -> &'static [&'static str] {
    PROGRESS_TRACK_CLASSES
}

pub(in crate::widgets::bar) fn progress_track_draw_func()
-> impl Fn(&gtk::DrawingArea, &gtk::cairo::Context, i32, i32) + 'static {
    move |area, cr, width, height| draw_progress_perimeter(area, cr, width, height, 1.0)
}

pub(in crate::widgets::bar) fn progress_level_draw_func(
    percent: u8,
) -> impl Fn(&gtk::DrawingArea, &gtk::cairo::Context, i32, i32) + 'static {
    move |area, cr, width, height| {
        draw_progress_perimeter(area, cr, width, height, f64::from(percent) / 100.0);
    }
}

fn draw_progress_perimeter(
    area: &gtk::DrawingArea,
    cr: &gtk::cairo::Context,
    width: i32,
    height: i32,
    fraction: f64,
) {
    if width <= 0 || height <= 0 || fraction <= 0.0 {
        return;
    }

    let points = perimeter_points(f64::from(width), f64::from(height));
    if points.len() < 2 {
        return;
    }

    let color = area.style_context().color();
    cr.set_line_width(PROGRESS_PERIMETER_THICKNESS);
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    cr.set_line_join(gtk::cairo::LineJoin::Round);
    set_source_rgba(cr, &color);
    draw_polyline_fraction(cr, &points, fraction.clamp(0.0, 1.0));
    let _ = cr.stroke();
}

fn perimeter_points(width: f64, height: f64) -> Vec<(f64, f64)> {
    let inset = PROGRESS_PERIMETER_THICKNESS / 2.0;
    let x0 = inset;
    let y0 = inset;
    let x1 = width - inset;
    let y1 = height - inset;
    if x1 <= x0 || y1 <= y0 {
        return Vec::new();
    }

    let radius = PROGRESS_PERIMETER_RADIUS
        .min((x1 - x0) / 2.0)
        .min((y1 - y0) / 2.0)
        .max(0.0);
    let mut points = Vec::new();
    push_point(&mut points, width / 2.0, y0);
    push_point(&mut points, x1 - radius, y0);
    push_arc(
        &mut points,
        x1 - radius,
        y0 + radius,
        radius,
        -PI / 2.0,
        0.0,
    );
    push_point(&mut points, x1, y1 - radius);
    push_arc(&mut points, x1 - radius, y1 - radius, radius, 0.0, PI / 2.0);
    push_point(&mut points, width / 2.0, y1);
    push_point(&mut points, x0 + radius, y1);
    push_arc(&mut points, x0 + radius, y1 - radius, radius, PI / 2.0, PI);
    push_point(&mut points, x0, y0 + radius);
    push_arc(
        &mut points,
        x0 + radius,
        y0 + radius,
        radius,
        PI,
        3.0 * PI / 2.0,
    );
    push_point(&mut points, width / 2.0, y0);
    points
}

fn push_arc(
    points: &mut Vec<(f64, f64)>,
    center_x: f64,
    center_y: f64,
    radius: f64,
    start: f64,
    end: f64,
) {
    if radius <= 0.0 {
        return;
    }

    const STEPS: u32 = 8;
    for step in 1..=STEPS {
        let fraction = f64::from(step) / f64::from(STEPS);
        let angle = start + (end - start) * fraction;
        push_point(
            points,
            center_x + angle.cos() * radius,
            center_y + angle.sin() * radius,
        );
    }
}

fn push_point(points: &mut Vec<(f64, f64)>, x: f64, y: f64) {
    if points
        .last()
        .is_some_and(|(last_x, last_y)| (last_x - x).abs() < 0.1 && (last_y - y).abs() < 0.1)
    {
        return;
    }
    points.push((x, y));
}

fn draw_polyline_fraction(cr: &gtk::cairo::Context, points: &[(f64, f64)], fraction: f64) {
    let total = polyline_length(points);
    let mut remaining = total * fraction;
    let Some(&(start_x, start_y)) = points.first() else {
        return;
    };

    cr.move_to(start_x, start_y);
    for segment in points.windows(2) {
        let (x0, y0) = segment[0];
        let (x1, y1) = segment[1];
        let length = (x1 - x0).hypot(y1 - y0);
        if length <= 0.0 {
            continue;
        }
        if remaining >= length {
            cr.line_to(x1, y1);
            remaining -= length;
            continue;
        }

        let segment_fraction = (remaining / length).clamp(0.0, 1.0);
        cr.line_to(
            x0 + (x1 - x0) * segment_fraction,
            y0 + (y1 - y0) * segment_fraction,
        );
        break;
    }
}

fn polyline_length(points: &[(f64, f64)]) -> f64 {
    points
        .windows(2)
        .map(|segment| {
            let (x0, y0) = segment[0];
            let (x1, y1) = segment[1];
            (x1 - x0).hypot(y1 - y0)
        })
        .sum()
}

fn set_source_rgba(cr: &gtk::cairo::Context, color: &gtk::gdk::RGBA) {
    cr.set_source_rgba(
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        f64::from(color.alpha()),
    );
}

fn duration_text(ms: i64) -> String {
    let total_seconds = (ms / 1000).max(0);
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn fallback(value: &str) -> &str {
    non_empty(value).unwrap_or("unknown")
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn normalized(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}
