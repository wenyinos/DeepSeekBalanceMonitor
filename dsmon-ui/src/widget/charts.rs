//! The two drawings the widget owns: the balance curve and the activity grid.
//!
//! Neither carries an axis, a legend or a tooltip: the figures above them
//! already say what the numbers are, and a panel this size has no room for
//! anything else.

use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate};
use dsmon_core::widget_api::{Day, Point};
use egui::{CornerRadius, Rect, Sense};

use crate::theme::Palette;

/// Height of the balance curve.
pub const CURVE_HEIGHT: f32 = 34.0;

/// Weeks shown in the activity grid, matching the payload's window.
const WEEKS: i64 = 12;
const CELL: f32 = 10.0;
const GAP: f32 = 3.0;
const ROWS: i64 = 7;

/// The balance curve, from the points the application sent.
pub fn balance_curve(ui: &mut egui::Ui, palette: &Palette, points: &[Point]) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), CURVE_HEIGHT),
        Sense::hover(),
    );

    if points.len() < 2 {
        // One reading is not a curve. Saying nothing is better than drawing a
        // flat line that looks like a measurement.
        return;
    }

    let (min, max) = points
        .iter()
        .fold((f64::MAX, f64::MIN), |(min, max), point| {
            (min.min(point.v), max.max(point.v))
        });
    // A flat series has no range to scale against; give it one so the line
    // lands in the middle instead of on an edge.
    let span = (max - min).max(f64::EPSILON);
    let (first, last) = (points[0].t, points[points.len() - 1].t);
    let span_t = (last - first).max(1) as f64;

    let at = |point: &Point| {
        let x = rect.left() + rect.width() * ((point.t - first) as f64 / span_t) as f32;
        let y = rect.bottom() - rect.height() * ((point.v - min) / span) as f32;
        egui::pos2(
            x.clamp(rect.left(), rect.right()),
            y.clamp(rect.top(), rect.bottom()),
        )
    };

    let stroke = egui::Stroke::new(1.6, palette.accent);
    let shape = egui::epaint::PathShape::line(points.iter().map(at).collect(), stroke);
    ui.painter().add(shape);

    for end in [points.first(), points.last()].into_iter().flatten() {
        ui.painter().circle_filled(at(end), 2.0, palette.accent);
    }
}

/// The activity grid: a column per week, a row per weekday.
///
/// The days themselves come from the payload with their weekday already worked
/// out, so this only has to place them — two dates apart is all the arithmetic
/// it does.
///
/// One grid is on screen at a time, so it is scaled to its own busiest day:
/// a small subscription shown on a large one's scale would read as a blank
/// month, which is not what its own pattern looks like.
pub fn activity_grid(ui: &mut egui::Ui, palette: &Palette, days: &[Day]) {
    let width = WEEKS as f32 * (CELL + GAP) - GAP;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, ROWS as f32 * (CELL + GAP) - GAP),
        Sense::hover(),
    );

    let today = Local::now().date_naive();
    let first = today
        - ChronoDuration::days((WEEKS - 1) * 7 + i64::from(today.weekday().num_days_from_monday()));

    let used_on = |date: NaiveDate| {
        days.iter()
            .find(|day| NaiveDate::parse_from_str(&day.date, "%Y-%m-%d") == Ok(date))
            .map(|day| day.used)
    };
    let most = days.iter().map(|day| day.used).fold(0.0_f64, f64::max);

    for column in 0..WEEKS {
        for row in 0..ROWS {
            let date = first + ChronoDuration::days(column * 7 + row);
            if date > today {
                continue;
            }
            let cell = Rect::from_min_size(
                rect.min + egui::vec2(column as f32 * (CELL + GAP), row as f32 * (CELL + GAP)),
                egui::vec2(CELL, CELL),
            );
            let colour = match used_on(date) {
                // Nothing burned that day: the cell is drawn as an empty slot
                // rather than left out, so the grid keeps its shape.
                Some(used) if most > 0.0 => palette.accent.gamma_multiply(level(used / most)),
                _ => palette.bg_input,
            };
            ui.painter()
                .rect_filled(cell, CornerRadius::same(2), colour);
        }
    }

    let _ = palette.text_primary;
}

/// Maps a share of the busiest day onto one of four steps.
fn level(share: f64) -> f32 {
    if share >= 0.75 {
        1.0
    } else if share >= 0.5 {
        0.68
    } else if share >= 0.25 {
        0.42
    } else {
        0.22
    }
}
