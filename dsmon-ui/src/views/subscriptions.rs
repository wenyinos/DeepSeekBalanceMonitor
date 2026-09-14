//! Subscriptions page: quota windows for the paid plans, two per row.
//!
//! Cards are laid out in pairs so another provider only has to add an entry
//! here; the grid takes care of the rest.

use dsmon_core::monitor::{Snapshot, Subscription};
use dsmon_core::platforms::format_reset_seconds;
use egui::RichText;

use super::{card, progress_line, usage_color, View};
use crate::theme::Palette;

/// Draws the page.
pub fn show(ui: &mut egui::Ui, view: &View<'_>, snapshot: &Snapshot) {
    ui.columns(2, |columns| {
        opencode_go_card(&mut columns[0], view, snapshot);
        command_code_card(&mut columns[1], view, snapshot);
    });
}

fn opencode_go_card(ui: &mut egui::Ui, view: &View<'_>, snapshot: &Snapshot) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.label(
            RichText::new(view.text("og_title"))
                .size(16.0)
                .color(palette.text_primary)
                .strong(),
        );
        ui.add_space(10.0);

        match &snapshot.opencode_go {
            Subscription::Loaded(quota) => {
                for (label, window) in [
                    (view.text("og_window_5h"), quota.rolling.as_ref()),
                    (view.text("og_window_weekly"), quota.weekly.as_ref()),
                    (view.text("og_window_monthly"), quota.monthly.as_ref()),
                ] {
                    window_row(
                        ui,
                        palette,
                        label,
                        window.map(|w| (w.usage_percent, w.reset_in_sec)),
                    );
                }
            }
            Subscription::NotConfigured => {
                ui.label(
                    RichText::new(view.text("og_not_configured"))
                        .color(palette.text_secondary)
                        .size(12.0),
                );
            }
            Subscription::Failed(error) => {
                ui.label(
                    RichText::new(format!("{} {error}", view.text("og_refresh_failed")))
                        .color(palette.destructive)
                        .size(12.0),
                );
            }
        }
    });
}

fn command_code_card(ui: &mut egui::Ui, view: &View<'_>, snapshot: &Snapshot) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.label(
            RichText::new(view.text("group_cc"))
                .size(16.0)
                .color(palette.text_primary)
                .strong(),
        );
        ui.add_space(10.0);

        match &snapshot.command_code {
            Subscription::Loaded(quota) => {
                for (label, window) in [
                    (view.text("cc_window_5h"), quota.five_hour.as_ref()),
                    (view.text("cc_window_weekly"), quota.weekly.as_ref()),
                    (view.text("cc_window_monthly"), quota.monthly.as_ref()),
                ] {
                    window_row(ui, palette, label, window.map(window_from_cc));
                }
            }
            Subscription::NotConfigured => {
                ui.label(
                    RichText::new(view.text("cc_not_configured"))
                        .color(palette.text_secondary)
                        .size(12.0),
                );
            }
            Subscription::Failed(error) => {
                ui.label(
                    RichText::new(format!("{} {error}", view.text("cc_refresh_failed")))
                        .color(palette.destructive)
                        .size(12.0),
                );
            }
        }
    });
}

fn window_from_cc(window: &dsmon_core::model::CommandCodeWindow) -> (f64, i64) {
    let percent = if window.cap > 0.0 {
        (window.used / window.cap * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    (percent, window.reset_in_sec)
}

/// One quota window: label and figures on a line, the bar right beneath.
fn window_row(ui: &mut egui::Ui, palette: &Palette, label: &str, window: Option<(f64, i64)>) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(label)
                .color(palette.text_secondary)
                .size(12.0),
        );
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| match window {
                Some((percent, reset_in_sec)) => {
                    if reset_in_sec > 0 {
                        ui.label(
                            RichText::new(format_reset_seconds(reset_in_sec))
                                .color(palette.text_secondary)
                                .size(12.0),
                        );
                    }
                    ui.label(
                        RichText::new(format!("{percent:.0}%"))
                            .color(palette.text_primary)
                            .size(12.0),
                    );
                }
                None => {
                    ui.label(RichText::new("--").color(palette.text_secondary).size(12.0));
                }
            },
        );
    });

    let (percent, _) = window.unwrap_or((0.0, 0));
    progress_line(
        ui,
        palette,
        (percent / 100.0) as f32,
        usage_color(palette, percent as f32),
    );
    ui.add_space(10.0);
}
