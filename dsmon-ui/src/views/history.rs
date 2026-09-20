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
    /// Which platform's history is on screen.
    pub platform: String,
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
            platform: dsmon_core::storage::KEY_DEEPSEEK.to_owned(),
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
                // Latin captions: sitting beside "1d"/"30d", they share
                // the digits' glyph height so the row reads as one size. A Han
                // caption fills its em box and looks larger at the same point size.
                RichText::new("Days")
                    .text_style(egui::TextStyle::Button)
                    .color(palette.text_secondary),
            );

            ui.scope(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(10.0, 3.0);
                for days in RANGES {
                    let selected = state.days == days;
                    if ui
                        .selectable_label(
                            selected,
                            RichText::new(format!("{days}d")).text_style(egui::TextStyle::Button),
                        )
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
                    .selected_text(RichText::new(current).text_style(egui::TextStyle::Button))
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
                    RichText::new("Currency")
                        .text_style(egui::TextStyle::Button)
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
                .size(16.0)
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

        // The status band sits above the chart on the same scale as the chart's
        // readings, so a stretch of readings shows as a stretch of one colour.
        // Only DeepSeek has a status page; every other platform records
        // "unknown" for every reading, which a band would tell in grey as if it
        // meant something.
        let band = (state.platform == dsmon_core::storage::KEY_DEEPSEEK)
            .then(|| status_band(ui, view, &state.records))
            .flatten()
            .unwrap_or(0.0);

        let line = Line::new(view.text("history_total"), PlotPoints::from(points))
            .color(palette.accent)
            .width(2.0);

        let grid = palette.border;
        let text = palette.text_secondary;

        // Fill what is left after the card chrome and the band, so the page
        // never scrolls.
        let plot_height = (available - 52.0 - band).max(140.0);
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

/// The service status across the readings the chart draws: one segment per
/// stretch that shared a status, the share of readable readings that were
/// healthy beneath it. Returns the height it took, so the chart can give it up.
///
/// What it adds to the single dot on the balance page is time: whether the
/// vendor was down an hour ago, and how much of the window this program spent
/// unable to read the page at all — a stretch in grey, which is a fault here
/// rather than one of the vendor's.
fn status_band(ui: &mut egui::Ui, view: &View<'_>, records: &[HistoryRecord]) -> Option<f32> {
    let palette = view.palette;
    let spans = dsmon_core::history::service_status_spans(records);
    let total: usize = spans.iter().map(|span| span.readings).sum();
    if total == 0 {
        return None;
    }

    let height = 6.0;
    ui.label(
        RichText::new(view.text("service_status"))
            .color(palette.text_secondary)
            .size(12.0),
    );
    ui.add_space(3.0);

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );
    let mut left = rect.left();
    for span in &spans {
        let width = rect.width() * span.readings as f32 / total as f32;
        let segment =
            egui::Rect::from_min_size(egui::pos2(left, rect.top()), egui::vec2(width, height));
        ui.painter().rect_filled(
            segment,
            egui::CornerRadius::same(1),
            super::status_color(palette, &span.status),
        );
        left += width;
    }

    let mut caption = dsmon_core::history::availability_percent(&spans)
        .map(|percent| {
            format!(
                "{} {}%",
                view.text("availability_label"),
                dsmon_core::history::format_percent(percent)
            )
        })
        .unwrap_or_default();
    let unreadable: usize = spans
        .iter()
        .filter(|span| span.status == "unknown")
        .map(|span| span.readings)
        .sum();
    if unreadable > 0 {
        if !caption.is_empty() {
            caption.push_str(" · ");
        }
        caption.push_str(&format!(
            "{} {unreadable} {}",
            view.text("unreadable_label"),
            view.text("times")
        ));
    }

    ui.add_space(4.0);
    if !caption.is_empty() {
        ui.label(
            RichText::new(caption)
                .color(palette.text_secondary)
                .size(12.0),
        );
    }
    response.on_hover_text(hover_text(view, &spans));

    // What the band took: the label, the band and the caption, with the spacing
    // between them. The chart gives this much back, so the page still fits.
    Some(height + 43.0)
}

/// What the band's segments stand for, level by level, in the order the levels
/// first appear.
fn hover_text(view: &View<'_>, spans: &[dsmon_core::history::StatusSpan]) -> String {
    let mut levels: Vec<(&str, usize)> = Vec::new();
    for span in spans {
        match levels.iter_mut().find(|(status, _)| *status == span.status) {
            Some((_, count)) => *count += span.readings,
            None => levels.push((span.status.as_str(), span.readings)),
        }
    }

    levels
        .into_iter()
        .map(|(status, count)| format!("{} {count}", crate::i18n::status_text(view.lang, status)))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn summary_card(ui: &mut egui::Ui, view: &View<'_>, state: &State, action: &mut Option<Action>) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.set_min_height(super::SUMMARY_CARD_HEIGHT);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(view.text("history_trend"))
                    .size(16.0)
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
