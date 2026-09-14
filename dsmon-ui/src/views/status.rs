//! Status page: the DeepSeek balance and the service health, laid out tight.

use dsmon_core::model::preferred_balance;
use dsmon_core::monitor::Snapshot;
use egui::{FontId, RichText};

use super::{card, status_color, status_dot, View};
use crate::fonts::DIGITS_FAMILY;

/// Draws the page. Returns true when the user asked for a fresh reading.
pub fn show(ui: &mut egui::Ui, view: &View<'_>, snapshot: &Snapshot) -> bool {
    let mut refresh = false;

    balance_card(ui, view, snapshot, &mut refresh);
    health_card(ui, view, snapshot);

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

        ui.add_space(4.0);

        match preferred_balance(&snapshot.balances) {
            Some((currency, balance)) => {
                // Figure, currency and the top-up split share one line.
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(dsmon_core::history::format_amount(balance.total_balance))
                            .font(FontId::new(
                                32.0,
                                egui::FontFamily::Name(DIGITS_FAMILY.into()),
                            ))
                            .color(palette.text_primary),
                    );
                    ui.add_space(6.0);
                    ui.label(RichText::new(currency).color(palette.text_secondary));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} {}   {} {}",
                                view.text("topped_up"),
                                dsmon_core::history::format_amount(balance.topped_up_balance),
                                view.text("granted"),
                                dsmon_core::history::format_amount(balance.granted_balance),
                            ))
                            .color(palette.text_secondary)
                            .small(),
                        );
                    });
                });
            }
            None => {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("--.--")
                            .font(FontId::new(
                                32.0,
                                egui::FontFamily::Name(DIGITS_FAMILY.into()),
                            ))
                            .color(palette.text_secondary),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(view.text("balance_empty"))
                            .color(palette.text_secondary)
                            .small(),
                    );
                });
            }
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
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

            if let Some(error) = &snapshot.last_error {
                ui.add_space(12.0);
                ui.label(RichText::new(error).color(palette.destructive).small());
            }
        });
    });
}

fn health_card(ui: &mut egui::Ui, view: &View<'_>, snapshot: &Snapshot) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.label(
            RichText::new(view.text("service_status"))
                .color(palette.text_primary)
                .strong(),
        );
        ui.add_space(8.0);

        let status = if snapshot.service_status.is_empty() {
            "unknown"
        } else {
            snapshot.service_status.as_str()
        };

        // Health, burn rate and runway share one line.
        ui.horizontal(|ui| {
            status_dot(ui, status_color(palette, status));
            ui.label(RichText::new(status_text(view, status)).color(palette.text_primary));

            ui.add_space(16.0);
            match &snapshot.consumption_rate {
                Some(rate) => {
                    ui.label(
                        RichText::new(format!(
                            "{} {}/h",
                            view.text("daily_rate"),
                            dsmon_core::history::format_amount(rate.hourly_rate)
                        ))
                        .color(palette.text_secondary)
                        .small(),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(format!(
                            "{} {}",
                            view.text("estimated_remaining"),
                            format_busy_hours(rate.busy_hours_left)
                        ))
                        .color(palette.text_secondary)
                        .small(),
                    );
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
    });
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
