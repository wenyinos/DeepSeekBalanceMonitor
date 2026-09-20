//! Subscriptions page: quota windows for the paid plans, two per row.
//!
//! Which plans appear, what they are called and which windows they have all
//! come from the catalog, so a plan brings only its client with it — the page
//! takes care of the rest.

use std::collections::{BTreeMap, BTreeSet};

use dsmon_core::catalog::{self, window_label_key, Mode, PlatformMeta};
use dsmon_core::config::MAX_BILLING_DAY;
use dsmon_core::history::daily_usage;
use dsmon_core::model::{PackageQuota, SubscriptionPoint};
use dsmon_core::monitor::{Snapshot, Subscription};
use dsmon_core::platforms::format_reset_seconds;
use dsmon_core::storage;
use egui::RichText;
use egui_plot::{Line, Plot, PlotPoints};

use super::{card, progress_line, usage_color, View};
use crate::theme::Palette;

/// How far back each plan's chart looks.
const HISTORY_DAYS: u64 = 30;

/// Fixed height of a chart card's heading row.
const HEADING_ROW_HEIGHT: f32 = 30.0;

/// What the page asks the application to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// The Command Code billing day was edited; persist it.
    BillingDay(u8),
    /// Re-read the quotas, leaving the balance alone.
    Refresh,
}

/// Page state, owned by the application.
#[derive(Debug, Clone, Default)]
pub struct State {
    /// Logged readings per platform, for the charts.
    pub history: BTreeMap<String, Vec<SubscriptionPoint>>,
    /// Day of the month Command Code renews on.
    pub billing_day: u8,
}

impl State {
    pub fn new(billing_day: u8) -> Self {
        Self {
            billing_day,
            ..Default::default()
        }
    }

    /// Reloads every plan's logged readings.
    pub fn reload(&mut self) {
        self.history = plans()
            .map(|meta| {
                let points = storage::subscription_usage_history(meta.key, "monthly", HISTORY_DAYS)
                    .unwrap_or_default();
                (meta.key.to_owned(), points)
            })
            .collect();
    }
}

/// Every plan the catalog lists, in display order.
fn plans() -> impl Iterator<Item = PlatformMeta> {
    catalog::implemented()
        .filter(|meta| meta.mode == Mode::Package)
        .copied()
}

/// The plans that hold a key, and so deserve a card. Decided by the stored key
/// rather than the last poll, so the cards appear at once.
fn configured_plans(configured: &BTreeSet<String>) -> Vec<PlatformMeta> {
    plans()
        .filter(|meta| configured.contains(meta.key))
        .collect()
}

/// Draws the page.
pub fn show(
    ui: &mut egui::Ui,
    view: &View<'_>,
    snapshot: &Snapshot,
    state: &mut State,
    configured: &BTreeSet<String>,
) -> Option<Action> {
    let mut refresh = false;

    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if snapshot.checking {
                view.text("checking")
            } else {
                view.text("refresh")
            };
            if ui.button(label).clicked() {
                refresh = true;
            }
        });
    });
    ui.add_space(4.0);

    // Only configured plans take up space. Leaving a placeholder for each
    // unconfigured one would turn into clutter as the list grows.
    let configured = configured_plans(configured);
    let mut billing_day = None;

    if configured.is_empty() {
        ui.label(
            RichText::new(view.text("no_subscriptions"))
                .color(view.palette.text_secondary)
                .size(12.0),
        );
    } else {
        // Two per row, so a growing list keeps the same rhythm.
        for chunk in configured.chunks(2) {
            ui.columns(2, |columns| {
                for (slot, meta) in chunk.iter().enumerate() {
                    plan_card(
                        &mut columns[slot],
                        view,
                        snapshot,
                        state,
                        meta,
                        &mut billing_day,
                    );
                }
            });
        }
    }

    if refresh {
        return Some(Action::Refresh);
    }
    billing_day.map(Action::BillingDay)
}

/// One plan: its windows in a card, its chart underneath.
fn plan_card(
    ui: &mut egui::Ui,
    view: &View<'_>,
    snapshot: &Snapshot,
    state: &State,
    meta: &PlatformMeta,
    billing_day: &mut Option<u8>,
) {
    let palette = view.palette;
    let reading = snapshot.packages.get(meta.key);

    card(ui, palette, |ui| {
        ui.set_min_height(super::SUBSCRIPTION_CARD_HEIGHT);
        ui.label(
            RichText::new(meta.display_name)
                .size(16.0)
                .color(palette.text_primary)
                .strong(),
        );
        ui.add_space(10.0);

        match reading {
            Some(Subscription::Loaded(quota)) => {
                for name in meta.windows {
                    window_row(
                        ui,
                        palette,
                        view.text(window_label_key(name)),
                        usage_of(quota, name),
                        pace_line(view, palette, snapshot, meta.key, name),
                    );
                }
            }
            Some(Subscription::Failed(error)) => {
                let message = format!("{} {error}", view.text("package_refresh_failed"));
                ui.add(
                    egui::Label::new(RichText::new(message).color(palette.destructive).size(12.0))
                        .truncate(),
                )
                .on_hover_text(error);
            }
            // A card for a plan without a key is not shown, so this is only
            // reached while the first reading is on its way.
            _ => {
                ui.label(
                    RichText::new(view.text("package_not_configured"))
                        .color(palette.text_secondary)
                        .size(12.0),
                );
            }
        }
    });

    // No reading, no chart.
    let points = state
        .history
        .get(meta.key)
        .map(Vec::as_slice)
        .unwrap_or_default();
    if matches!(reading, Some(Subscription::Loaded(_))) {
        if let Some(edited) = usage_chart(
            ui,
            view,
            meta.display_name,
            points,
            meta.key == storage::KEY_COMMAND_CODE,
            state.billing_day,
        ) {
            *billing_day = Some(edited);
        }
    }
}

/// The used share of one window, absent when the plan does not report it.
fn usage_of(quota: &PackageQuota, name: &str) -> Option<(f64, i64)> {
    quota
        .get(name)
        .map(|window| (window.usage_percent, window.reset_in_sec))
}

/// One quota window: label and figures on a line, the bar right beneath.
fn window_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    window: Option<(f64, i64)>,
    pace: Option<(String, egui::Color32)>,
) {
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
                        RichText::new(dsmon_core::history::format_percent(percent))
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
    ui.add_space(6.0);

    // How fast the window is going, and whether that pace outruns its clock.
    if let Some((text, color)) = pace {
        ui.label(RichText::new(text).color(color).size(11.0));
        ui.add_space(6.0);
    }
    ui.add_space(4.0);
}

/// The pace of one window of one plan, for the line under its bar.
///
/// A window whose pace spends it before it resets is worth saying out loud, so
/// it is coloured as a warning and named; one that is simply being used gets
/// the figure and nothing else.
fn pace_line(
    view: &View<'_>,
    palette: &Palette,
    snapshot: &Snapshot,
    platform: &str,
    window: &str,
) -> Option<(String, egui::Color32)> {
    let rate = snapshot.window_rates.get(platform)?.get(window)?;
    let pace = format!(
        "{} {}%",
        view.text("pace_per_day"),
        dsmon_core::history::format_percent(rate.percent_per_day())
    );

    Some(if rate.runs_out_first() {
        (
            format!("{pace} · {}", view.text("quota_runs_out")),
            palette.destructive,
        )
    } else {
        (pace, palette.text_secondary)
    })
}

/// Per-day consumption for one plan, with an optional billing-day field.
///
/// Returns the edited day when the field changes.
fn usage_chart(
    ui: &mut egui::Ui,
    view: &View<'_>,
    title: &str,
    points: &[SubscriptionPoint],
    billing_day_field: bool,
    billing_day: u8,
) -> Option<u8> {
    let palette = view.palette;
    let mut action = None;

    card(ui, palette, |ui| {
        ui.set_min_height(super::SUBSCRIPTION_CHART_HEIGHT);

        // The heading row has a fixed height: the numeric field is taller than
        // the heading, which would otherwise push this card's chart a few pixels
        // lower than the other card's.
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), HEADING_ROW_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(
                    RichText::new(title)
                        .size(16.0)
                        .color(palette.text_primary)
                        .strong(),
                );

                if billing_day_field {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // The field, its label and the heading all share one size:
                        // egui aligns rows by the middle of each item, so mixing
                        // sizes leaves them sitting on different baselines.
                        ui.scope(|ui| {
                            ui.style_mut()
                                .text_styles
                                .insert(egui::TextStyle::Button, egui::FontId::proportional(16.0));

                            let mut edited = billing_day;
                            if ui
                                .add(
                                    egui::DragValue::new(&mut edited)
                                        .range(1..=MAX_BILLING_DAY)
                                        .speed(0.2),
                                )
                                .changed()
                            {
                                action = Some(edited);
                            }
                        });

                        ui.label(
                            // A short Latin caption, like "Days" and "Currency" on
                            // the status page: it keeps the digits' glyph height.
                            RichText::new("Day")
                                .size(16.0)
                                .color(palette.text_secondary),
                        );
                    });
                }
            },
        );

        ui.add_space(8.0);

        let usage = daily_usage(points);
        if usage.len() < 2 {
            ui.label(
                RichText::new(view.text("trend_needs_data"))
                    .color(palette.text_secondary)
                    .size(12.0),
            );
            return;
        }

        let series: Vec<[f64; 2]> = usage
            .iter()
            .enumerate()
            .map(|(index, day)| [index as f64, day.used])
            .collect();
        // Axis labels show month and day; the year is the same throughout.
        let labels: Vec<String> = usage
            .iter()
            .map(|day| day.date.get(5..).unwrap_or(&day.date).to_owned())
            .collect();

        let line = Line::new(view.text("daily_usage"), PlotPoints::from(series))
            .color(palette.accent)
            .width(2.0);

        let label_for = labels.clone();
        Plot::new(format!("subscription-trend-{title}"))
            .height(150.0)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .x_axis_formatter(move |mark, _| {
                let index = mark.value.round() as isize;
                if index < 0 {
                    return String::new();
                }
                label_for.get(index as usize).cloned().unwrap_or_default()
            })
            .show(ui, |plot_ui| {
                plot_ui.line(line);
            });
    });

    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_plan_in_the_catalog_gets_a_card_when_configured() {
        let configured: BTreeSet<String> = plans().map(|meta| meta.key.to_owned()).collect();
        let shown = configured_plans(&configured);
        assert_eq!(
            shown.len(),
            plans().count(),
            "a key for every plan shows every plan"
        );
        assert!(shown.iter().all(|meta| meta.mode == Mode::Package));
    }

    #[test]
    fn a_plan_without_a_key_is_not_shown() {
        let configured: BTreeSet<String> =
            [storage::KEY_COMMAND_CODE.to_owned()].into_iter().collect();
        let shown = configured_plans(&configured);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].key, storage::KEY_COMMAND_CODE);
    }

    #[test]
    fn a_missing_window_reads_as_nothing() {
        let mut quota = PackageQuota::new();
        quota.insert(
            "5h".to_owned(),
            dsmon_core::model::QuotaWindow::from_percent(25.0, 60),
        );

        assert_eq!(usage_of(&quota, "5h"), Some((25.0, 60)));
        assert_eq!(usage_of(&quota, "weekly"), None, "a window the plan omits");
    }
}
