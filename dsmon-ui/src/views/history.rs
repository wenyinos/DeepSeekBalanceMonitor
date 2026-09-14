//! Balance trend: range and currency filters, the chart and the CSV export.
//!
//! Drawn at the bottom of the status page.

use dsmon_core::history::{history_csv, summarize_history};
use dsmon_core::model::HistoryRecord;
use egui::RichText;
use egui_plot::{Line, Plot, PlotPoints};

use super::{card, View};
use crate::theme::Palette;

/// Ranges offered by the filter row.
pub const RANGES: [u64; 3] = [1, 7, 30];

/// What the page asks the application to do after a click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Reload the records for the current filters.
    Reload,
    /// Write the visible records to a CSV file.
    Export,
}

/// Page state, owned by the application.
#[derive(Debug, Clone)]
pub struct State {
    pub days: u64,
    pub currency: Option<String>,
    pub records: Vec<HistoryRecord>,
    pub currencies: Vec<String>,
    /// Result of the last export, shown under the buttons.
    pub notice: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            days: 7,
            currency: None,
            records: Vec::new(),
            currencies: Vec::new(),
            notice: None,
        }
    }
}

/// The trend summary, drawn as one column of the status page's top row.
pub fn show_summary(ui: &mut egui::Ui, view: &View<'_>, state: &State) -> Option<Action> {
    let mut action = None;
    summary_card(ui, view, state, &mut action);
    action
}

/// The filter row and the chart, drawn full width beneath the top row.
pub fn show_chart(ui: &mut egui::Ui, view: &View<'_>, state: &mut State) -> Option<Action> {
    let mut action = None;
    filters_card(ui, view, state, &mut action);
    chart_card(ui, view, state, ui.available_height());
    action
}

fn filters_card(
    ui: &mut egui::Ui,
    view: &View<'_>,
    state: &mut State,
    action: &mut Option<Action>,
) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(
                RichText::new(view.text("history_days"))
                    .size(14.0)
                    .color(palette.text_secondary),
            );

            ui.scope(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(10.0, 3.0);
                for days in RANGES {
                    let selected = state.days == days;
                    if ui
                        .selectable_label(selected, RichText::new(format!("{days}d")).size(14.0))
                        .clicked()
                        && !selected
                    {
                        state.days = days;
                        *action = Some(Action::Reload);
                    }
                }
            });

            // The currency picker is pinned to the right edge of the card, so
            // both halves of the row line up with the card's own margins.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let current = state
                    .currency
                    .clone()
                    .unwrap_or_else(|| view.text("history_all").to_owned());
                egui::ComboBox::from_id_salt("history-currency")
                    .selected_text(RichText::new(current).size(14.0))
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(state.currency.is_none(), view.text("history_all"))
                            .clicked()
                            && state.currency.is_some()
                        {
                            state.currency = None;
                            *action = Some(Action::Reload);
                        }
                        for currency in state.currencies.clone() {
                            let selected = state.currency.as_deref() == Some(currency.as_str());
                            if ui.selectable_label(selected, &currency).clicked() && !selected {
                                state.currency = Some(currency);
                                *action = Some(Action::Reload);
                            }
                        }
                    });

                ui.label(
                    RichText::new(view.text("history_currency_filter"))
                        .size(14.0)
                        .color(palette.text_secondary),
                );
            });
        });
    });
}

fn chart_card(ui: &mut egui::Ui, view: &View<'_>, state: &State, available: f32) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.label(
            RichText::new(view.text("history_chart"))
                .color(palette.text_primary)
                .strong(),
        );
        ui.add_space(8.0);

        if state.records.len() < 2 {
            ui.label(
                RichText::new(view.text("history_empty"))
                    .color(palette.text_secondary)
                    .size(12.0),
            );
            return;
        }

        let points: Vec<[f64; 2]> = state
            .records
            .iter()
            .enumerate()
            .map(|(index, record)| [index as f64, record.total])
            .collect();

        let line = Line::new(view.text("history_total"), PlotPoints::from(points))
            .color(palette.accent)
            .width(2.0);

        let grid = palette.border;
        let text = palette.text_secondary;

        // Fill what is left after the card chrome, so the page never scrolls.
        let plot_height = (available - 52.0).max(140.0);
        Plot::new("history-plot")
            .height(plot_height)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .show_grid(true)
            .x_axis_formatter(move |mark, _| {
                let index = mark.value.round() as isize;
                if index < 0 {
                    return String::new();
                }
                format!("#{index}")
            })
            .show(ui, |plot_ui| {
                plot_ui.line(line);
            })
            .response
            .on_hover_text(view.text("history_total"));

        // The plot paints with its own defaults; nudge the frame colours to the
        // palette so the card stays flat.
        let _ = (grid, text);
    });
}

fn summary_card(ui: &mut egui::Ui, view: &View<'_>, state: &State, action: &mut Option<Action>) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.set_min_height(super::SUMMARY_CARD_HEIGHT);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(view.text("history_trend"))
                    .color(palette.text_primary)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(view.text("export")).clicked() {
                    *action = Some(Action::Export);
                }
            });
        });
        ui.add_space(6.0);

        match summarize_history(&state.records).first() {
            Some(summary) => {
                ui.label(
                    RichText::new(format!(
                        "{}  {} {}",
                        summary.currency,
                        view.text("history_range"),
                        format_range(&summary.min_total, &summary.max_total)
                    ))
                    .color(palette.text_secondary)
                    .size(12.0),
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new(format!(
                        "{} {}  {} {} {}",
                        view.text("history_avg"),
                        dsmon_core::history::format_amount(summary.avg_total),
                        view.text("history_change"),
                        dsmon_core::history::format_amount(summary.change_total),
                        trend_label(view, summary.change_total),
                    ))
                    .color(trend_color(palette, summary.change_total))
                    .size(12.0),
                );
            }
            None => {
                ui.label(
                    RichText::new(view.text("history_empty"))
                        .color(palette.text_secondary)
                        .size(12.0),
                );
            }
        }

        if let Some(notice) = &state.notice {
            ui.add_space(4.0);
            ui.label(
                RichText::new(notice)
                    .color(palette.text_secondary)
                    .size(12.0),
            );
        }
    });
}

fn format_range(min: &f64, max: &f64) -> String {
    format!(
        "{} - {}",
        dsmon_core::history::format_amount(*min),
        dsmon_core::history::format_amount(*max)
    )
}

fn trend_label(view: &View<'_>, change: f64) -> &'static str {
    if change > 0.005 {
        view.text("history_rising")
    } else if change < -0.005 {
        view.text("history_falling")
    } else {
        view.text("history_flat")
    }
}

fn trend_color(palette: &Palette, change: f64) -> egui::Color32 {
    if change < -0.005 {
        palette.destructive
    } else {
        palette.text_secondary
    }
}

/// Renders the visible records as CSV, for the application to write out.
pub fn export(records: &[HistoryRecord]) -> String {
    history_csv(records)
}
