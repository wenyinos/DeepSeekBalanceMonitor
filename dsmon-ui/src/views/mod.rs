//! The pages reachable from the sidebar.

pub mod history;
pub mod settings;
pub mod status;
pub mod subscriptions;

use egui::{Color32, CornerRadius, Sense};

use crate::i18n::tr;
use crate::theme::Palette;

/// Everything a page needs besides its own state.
pub struct View<'a> {
    pub palette: &'a Palette,
    pub lang: &'a str,
}

impl View<'_> {
    /// Looks up a label in the current language.
    pub fn text(&self, key: &str) -> &'static str {
        tr(self.lang, key)
    }
}

/// Height shared by the status page's top-row cards, so the three columns line
/// up with each other regardless of how much each one has to say.
pub const SUMMARY_CARD_HEIGHT: f32 = 122.0;

/// Panel with the standard card styling.
pub fn card(ui: &mut egui::Ui, palette: &Palette, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(palette.bg_panel)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            contents(ui);
        });
    ui.add_space(10.0);
}

/// A 3px progress line, filled up to `fraction`.
pub fn progress_line(ui: &mut egui::Ui, palette: &Palette, fraction: f32, fill: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 3.0), Sense::hover());
    let radius = CornerRadius::same(2);
    ui.painter().rect_filled(rect, radius, palette.bg_input);

    let fraction = fraction.clamp(0.0, 1.0);
    if fraction > 0.0 {
        let filled =
            egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * fraction, rect.height()));
        ui.painter().rect_filled(filled, radius, fill);
    }
}

/// Colour for a service-health level.
pub fn status_color(palette: &Palette, status: &str) -> Color32 {
    match status {
        "none" => palette.positive,
        "minor" | "maintenance" => palette.warning,
        "major" | "critical" => palette.destructive,
        _ => palette.text_secondary,
    }
}

/// Colour for a quota bar: the accent turns amber past 60% and red past 80%.
pub fn usage_color(palette: &Palette, percent: f32) -> Color32 {
    if percent >= 80.0 {
        palette.destructive
    } else if percent >= 60.0 {
        palette.warning
    } else {
        palette.accent
    }
}

/// A small filled dot, used for health indicators.
pub fn status_dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}
