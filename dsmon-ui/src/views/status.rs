//! Status page: balance, service health and the balance trend.
//!
//! The two summary cards sit side by side; the chart takes the full width
//! underneath.

use dsmon_core::model::preferred_balance;
use dsmon_core::monitor::Snapshot;
use egui::{FontId, RichText};

use super::{card, history, status_color, status_dot, View};
use crate::fonts::DIGITS_FAMILY;

/// What the page asks the application to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Poll the APIs again.
    Refresh,
    /// Reload the chart data for the current filters.
    ReloadHistory,
    /// Write the visible records to a CSV file.
    ExportHistory,
}

/// Draws the page.
pub fn show(
    ui: &mut egui::Ui,
    view: &View<'_>,
    snapshot: &Snapshot,
    history: &mut history::State,
    platform: &str,
) -> Option<Action> {
    let mut refresh = false;
    let mut history_action = None;

    // Balance, trend summary and health share the top row, for every provider.
    // The third card differs: DeepSeek has a public status page, the others
    // report whether their last read went through.
    ui.columns(3, |columns| {
        balance_card(&mut columns[0], view, snapshot, platform, &mut refresh);
        history_action = history::show_summary(&mut columns[1], view, history);

        if platform == dsmon_core::storage::KEY_DEEPSEEK {
            health_card(&mut columns[2], view, snapshot);
        } else {
            connection_card(&mut columns[2], view, snapshot, platform);
        }
    });

    // The filter row and the chart keep the full width below.
    if let Some(action) = history::show_chart(ui, view, history) {
        history_action = Some(action);
    }

    if refresh {
        return Some(Action::Refresh);
    }
    history_action.map(|action| match action {
        history::Action::Reload => Action::ReloadHistory,
        history::Action::Export => Action::ExportHistory,
    })
}

fn balance_card(
    ui: &mut egui::Ui,
    view: &View<'_>,
    snapshot: &Snapshot,
    platform: &str,
    refresh: &mut bool,
) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.set_min_height(super::SUMMARY_CARD_HEIGHT);

        ui.horizontal(|ui| {
            let title = dsmon_core::catalog::find(platform)
                .map(|meta| format!("{} {}", meta.display_name, view.text("balance_word")))
                .unwrap_or_default();
            ui.label(
                RichText::new(title)
                    .size(16.0)
                    .color(palette.text_primary)
                    .strong(),
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

        ui.add_space(2.0);

        // Every line is drawn whether or not a reading has arrived yet, with
        // placeholders standing in for the figures. The card therefore keeps the
        // same height and nothing shifts when data lands.
        let balances = snapshot.balances.get(platform);
        let preferred = balances.and_then(preferred_balance);
        let amount = preferred
            .map(|(_, balance)| dsmon_core::history::format_amount(balance.total_balance))
            .unwrap_or_else(|| "--.--".to_owned());
        let currency = preferred
            .map(|(currency, _)| currency.clone())
            .unwrap_or_else(|| "---".to_owned());

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(amount)
                    .font(FontId::new(
                        24.0,
                        egui::FontFamily::Name(DIGITS_FAMILY.into()),
                    ))
                    .color(if preferred.is_some() {
                        palette.text_primary
                    } else {
                        palette.text_secondary
                    }),
            );
            ui.add_space(6.0);
            ui.label(RichText::new(currency).color(palette.text_secondary));
        });

        ui.add_space(2.0);

        let (topped, granted) = preferred
            .map(|(_, balance)| {
                (
                    dsmon_core::history::format_amount(balance.topped_up_balance),
                    dsmon_core::history::format_amount(balance.granted_balance),
                )
            })
            .unwrap_or_else(|| ("--".to_owned(), "--".to_owned()));
        ui.label(
            RichText::new(format!(
                "{} {} · {} {}",
                view.text("topped_up"),
                topped,
                view.text("granted"),
                granted,
            ))
            .color(palette.text_secondary)
            .size(12.0),
        );

        ui.add_space(2.0);
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
            .size(12.0),
        );

        // Show why there is no figure: this platform's own failure, or the whole
        // poll's, which is the case when the key could not even be read.
        let failure = snapshot
            .balance_errors
            .get(platform)
            .or(if snapshot.balances.is_empty() {
                snapshot.last_error.as_ref()
            } else {
                None
            });
        if let Some(error) = failure {
            ui.add_space(2.0);
            ui.label(RichText::new(error).color(palette.destructive).size(12.0));
        }
    });
}

/// Whether the provider could be reached on the last poll.
///
/// Derived rather than fetched: every platform reports success or failure anyway,
/// and only DeepSeek publishes a status page of its own.
fn connection_card(ui: &mut egui::Ui, view: &View<'_>, snapshot: &Snapshot, platform: &str) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.label(
            RichText::new(view.text("connection_status"))
                .size(16.0)
                .color(palette.text_primary)
                .strong(),
        );
        ui.add_space(8.0);

        let error = snapshot.balance_errors.get(platform);

        ui.horizontal(|ui| {
            status_dot(
                ui,
                if error.is_some() {
                    palette.destructive
                } else {
                    palette.positive
                },
            );
            ui.label(
                RichText::new(view.text(if error.is_some() {
                    "connection_failed"
                } else {
                    "connection_ok"
                }))
                .color(palette.text_primary),
            );
        });

        ui.add_space(6.0);
        match error {
            Some(error) => {
                ui.label(RichText::new(error).color(palette.destructive).size(12.0));
            }
            None => {
                if let Some(checked) = snapshot.last_check {
                    ui.label(
                        RichText::new(format!(
                            "{} {}",
                            view.text("last_check"),
                            dsmon_core::time::format_local(checked)
                        ))
                        .color(palette.text_secondary)
                        .size(12.0),
                    );
                }
            }
        }
    });
}

fn health_card(ui: &mut egui::Ui, view: &View<'_>, snapshot: &Snapshot) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.set_min_height(super::SUMMARY_CARD_HEIGHT);
        ui.label(
            RichText::new(view.text("service_status"))
                .size(16.0)
                .color(palette.text_primary)
                .strong(),
        );
        ui.add_space(8.0);

        let status = if snapshot.service_status.is_empty() {
            "unknown"
        } else {
            snapshot.service_status.as_str()
        };

        ui.horizontal(|ui| {
            status_dot(ui, status_color(palette, status));
            ui.label(
                RichText::new(crate::i18n::status_text(view.lang, status))
                    .color(palette.text_primary),
            );
        });

        ui.add_space(6.0);

        match snapshot
            .consumption_rates
            .get(dsmon_core::storage::KEY_DEEPSEEK)
        {
            Some(rate) => {
                ui.label(
                    RichText::new(format!(
                        "{} {}/h",
                        view.text("daily_rate"),
                        dsmon_core::history::format_amount(rate.hourly_rate)
                    ))
                    .color(palette.text_secondary)
                    .size(12.0),
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new(format!(
                        "{} {}",
                        view.text("estimated_remaining"),
                        format_busy_hours(rate.busy_hours_left)
                    ))
                    .color(palette.text_secondary)
                    .size(12.0),
                );
            }
            None => {
                ui.label(
                    RichText::new(view.text("not_enough_data"))
                        .color(palette.text_secondary)
                        .size(12.0),
                );
            }
        }
    });
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
