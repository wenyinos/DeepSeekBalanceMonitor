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

/// Height of the status page's top-row cards: a floor every one of them meets,
/// so the three columns line up whatever each has to say.
///
/// The tallest is the balance card — the figure and two lines under it — at 146
/// points rendered the way the application renders (its own eight-point item
/// spacing, not egui's default three); the health card is 140 and the
/// connection card 113. A few points are added on top: the text comes from the
/// host's own font, and a metric that differs slightly would otherwise leave
/// that one card a hair taller than its neighbours.
pub const SUMMARY_CARD_HEIGHT: f32 = 152.0;

/// Height of a plan card's heading, which is the plan's name.
pub const SUBSCRIPTION_HEADING_HEIGHT: f32 = 43.0;

/// Height of one window inside a plan card: its label line, its bar, and the
/// pace line the bar carries.
pub const SUBSCRIPTION_WINDOW_HEIGHT: f32 = 79.0;

/// The height every plan card is drawn at: a heading plus one window per row,
/// counted over the plans on screen.
///
/// A card that sized itself to its contents made the page ragged — OpenCode Go
/// carries three windows and MiniMax two, so the card beside it stood shorter —
/// and the figure is the fullest card's own: 41 points of heading plus 77 per
/// window, measured from rendered cards rather than guessed, with the same
/// handful of points of slack for the host's font.
///
/// This is a minimum and not a fixed height: a card that ever needed more room
/// would take it, and stand taller than its neighbour — visible, rather than
/// clipped to fit.
pub fn subscription_card_height(windows: usize) -> f32 {
    SUBSCRIPTION_HEADING_HEIGHT + windows as f32 * SUBSCRIPTION_WINDOW_HEIGHT
}

/// Height shared by the two subscription charts, which differ by a line: only
/// Command Code carries the billing-day field.
pub const SUBSCRIPTION_CHART_HEIGHT: f32 = 232.0;

/// Panel with the standard card styling, returning the panel's own rectangle
/// so a caller can tell how much room it took.
pub fn card(
    ui: &mut egui::Ui,
    palette: &Palette,
    contents: impl FnOnce(&mut egui::Ui),
) -> egui::Rect {
    let rect = egui::Frame::NONE
        .fill(palette.bg_panel)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            contents(ui);
        })
        .response
        .rect;
    ui.add_space(10.0);
    rect
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
