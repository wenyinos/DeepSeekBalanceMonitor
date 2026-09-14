//! Status page: balance, service health and the subscription quotas.

use dsmon_core::model::{preferred_balance, Balances};
use dsmon_core::monitor::Snapshot;
use dsmon_core::platforms::format_reset_seconds;
use dsmon_core::platforms::status as service_status;
use egui::{FontId, RichText};

use super::{card, progress_line, status_color, status_dot, usage_color, View};
use crate::fonts::DIGITS_FAMILY;
use crate::theme::Palette;

/// Draws the page. Returns true when the user asked for a fresh reading.
pub fn show(ui: &mut egui::Ui, view: &View<'_>, snapshot: &Snapshot) -> bool {
    let mut refresh = false;

    balance_card(ui, view, snapshot, &mut refresh);
    health_card(ui, view, snapshot);
    subscriptions_card(ui, view, snapshot);

    refresh
}

fn balance_card(ui: &mut egui::Ui, view: &View<'_>, snapshot: &Snapshot, refresh: &mut bool) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(view.text("balance_title"))
                    .color(palette.text_secondary)
                    .small(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if snapshot.checking {
                    view.text("checking")
                } else {
                    view.text("check_now")
                };
                if ui.button(label).clicked() {
                    *refresh = true;
                }
            });
        });

        ui.add_space(6.0);

        match preferred_balance(&snapshot.balances) {
            Some((currency, balance)) => {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(dsmon_core::history::format_amount(balance.total_balance))
                            .font(FontId::new(
                                34.0,
                                egui::FontFamily::Name(DIGITS_FAMILY.into()),
                            ))
                            .color(palette.text_primary),
                    );
                    ui.label(RichText::new(currency).color(palette.text_secondary));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} {}",
                            view.text("topped_up"),
                            dsmon_core::history::format_amount(balance.topped_up_balance)
                        ))
                        .color(palette.text_secondary)
                        .small(),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} {}",
                            view.text("granted"),
                            dsmon_core::history::format_amount(balance.granted_balance)
                        ))
                        .color(palette.text_secondary)
                        .small(),
                    );
                });
            }
            None => {
                ui.label(
                    RichText::new("--.--")
                        .font(FontId::new(
                            34.0,
                            egui::FontFamily::Name(DIGITS_FAMILY.into()),
                        ))
                        .color(palette.text_secondary),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(view.text("balance_empty"))
                        .color(palette.text_secondary)
                        .small(),
                );
            }
        }

        if let Some(error) = &snapshot.last_error {
            ui.add_space(6.0);
            ui.label(RichText::new(error).color(palette.destructive).small());
        }

        ui.add_space(6.0);
        ui.label(
            RichText::new(match &snapshot.last_check {
                Some(checked) => format!(
                    "{} {}",
                    view.text("last_check"),
                    dsmon_core::time::format_local(*checked)
                ),
                None => view.text("not_checked").to_owned(),
            })
            .color(palette.text_secondary)
            .small(),
        );
    });
}

fn health_card(ui: &mut egui::Ui, view: &View<'_>, snapshot: &Snapshot) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.label(RichText::new(view.text("service_status")).strong());
        ui.add_space(8.0);

        let status = if snapshot.service_status.is_empty() {
            "unknown"
        } else {
            snapshot.service_status.as_str()
        };

        ui.horizontal(|ui| {
            status_dot(ui, status_color(palette, status));
            ui.label(RichText::new(status_text(view, status)).color(palette.text_primary));
        });

        ui.add_space(8.0);

        match &snapshot.consumption_rate {
            Some(rate) => {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} {}/h",
                            view.text("daily_rate"),
                            dsmon_core::history::format_amount(rate.hourly_rate)
                        ))
                        .color(palette.text_secondary)
                        .small(),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} {}",
                            view.text("estimated_remaining"),
                            format_busy_hours(rate.busy_hours_left)
                        ))
                        .color(palette.text_secondary)
                        .small(),
                    );
                });
            }
            None => {
                ui.label(
                    RichText::new(view.text("not_enough_data"))
                        .color(palette.text_secondary)
                        .small(),
                );
            }
        }
    });
}

fn subscriptions_card(ui: &mut egui::Ui, view: &View<'_>, snapshot: &Snapshot) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.label(RichText::new(view.text("og_title")).strong());
        ui.add_space(8.0);

        match &snapshot.opencode_go {
            Some(quota) => {
                window_row(
                    ui,
                    palette,
                    view.text("og_window_5h"),
                    quota
                        .rolling
                        .as_ref()
                        .map(|w| (w.usage_percent, w.reset_in_sec)),
                );
                window_row(
                    ui,
                    palette,
                    view.text("og_window_weekly"),
                    quota
                        .weekly
                        .as_ref()
                        .map(|w| (w.usage_percent, w.reset_in_sec)),
                );
                window_row(
                    ui,
                    palette,
                    view.text("og_window_monthly"),
                    quota
                        .monthly
                        .as_ref()
                        .map(|w| (w.usage_percent, w.reset_in_sec)),
                );
            }
            None => {
                ui.label(
                    RichText::new(view.text("og_not_configured"))
                        .color(palette.text_secondary)
                        .small(),
                );
            }
        }

        ui.add_space(12.0);
        ui.label(RichText::new(view.text("group_cc")).strong());
        ui.add_space(8.0);

        match &snapshot.command_code {
            Some(quota) => {
                window_row(
                    ui,
                    palette,
                    view.text("cc_window_5h"),
                    quota.five_hour.as_ref().map(window_from_cc),
                );
                window_row(
                    ui,
                    palette,
                    view.text("cc_window_weekly"),
                    quota.weekly.as_ref().map(window_from_cc),
                );
                window_row(
                    ui,
                    palette,
                    view.text("cc_window_monthly"),
                    quota.monthly.as_ref().map(window_from_cc),
                );
            }
            None => {
                ui.label(
                    RichText::new(view.text("cc_not_configured"))
                        .color(palette.text_secondary)
                        .small(),
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

fn window_row(ui: &mut egui::Ui, palette: &Palette, label: &str, window: Option<(f64, i64)>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(palette.text_secondary).small());
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| match window {
                Some((percent, reset_in_sec)) => {
                    if reset_in_sec > 0 {
                        ui.label(
                            RichText::new(format_reset_seconds(reset_in_sec))
                                .color(palette.text_secondary)
                                .small(),
                        );
                    }
                    ui.label(
                        RichText::new(format!("{percent:.0}%"))
                            .color(palette.text_primary)
                            .small(),
                    );
                }
                None => {
                    ui.label(RichText::new("--").color(palette.text_secondary).small());
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
    ui.add_space(6.0);
}

fn status_text(view: &View<'_>, status: &str) -> &'static str {
    view.text(match status {
        "none" => "status_none",
        "minor" => "status_minor",
        "major" => "status_major",
        "critical" => "status_critical",
        "maintenance" => "status_maintenance",
        _ => "status_unknown",
    })
}

fn format_busy_hours(hours: f64) -> String {
    if !hours.is_finite() || hours <= 0.0 {
        return "--".to_owned();
    }
    let days = (hours / 24.0).floor();
    let remainder = (hours % 24.0).floor();
    if days > 0.0 {
        format!("{days:.0}d {remainder:.0}h")
    } else {
        format!("{remainder:.0}h")
    }
}

/// Whether the balances fall below the configured threshold.
pub fn below_threshold(balances: &Balances, threshold: f64) -> bool {
    dsmon_core::model::is_low_balance(balances, threshold)
}

/// Re-exported for the tray tooltip.
pub fn health_label(status: &str) -> &'static str {
    service_status::normalize(status)
}
