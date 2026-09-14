//! Subscriptions page: quota windows for the paid plans, two per row.
//!
//! Cards are laid out in pairs so another provider only has to add an entry
//! here; the grid takes care of the rest.

use dsmon_core::history::daily_usage;
use dsmon_core::model::SubscriptionPoint;
use dsmon_core::monitor::{Snapshot, Subscription};
use dsmon_core::platforms::format_reset_seconds;
use dsmon_core::storage;
use egui::RichText;
use egui_plot::{Line, Plot, PlotPoints};

use super::{card, progress_line, usage_color, View};

/// Fixed height of a chart card's heading row.
const HEADING_ROW_HEIGHT: f32 = 30.0;
use crate::theme::Palette;

/// The providers the page can show, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Provider {
    OpenCodeGo,
    CommandCode,
}

impl Provider {
    const ALL: [Provider; 2] = [Provider::OpenCodeGo, Provider::CommandCode];

    /// The key this provider's secret is stored under.
    fn key(self) -> &'static str {
        match self {
            Provider::OpenCodeGo => dsmon_core::storage::KEY_OPENCODE_GO,
            Provider::CommandCode => dsmon_core::storage::KEY_COMMAND_CODE,
        }
    }

    /// Whether the provider has a key stored, and so deserves a card. Decided by
    /// the stored key rather than the last poll, so the cards appear at once.
    fn is_configured(self, configured: &std::collections::BTreeSet<String>) -> bool {
        configured.contains(self.key())
    }
}

/// What the page asks the application to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// The Command Code billing day was edited; persist it.
    BillingDay(u8),
    /// Re-read the subscription quotas, leaving the balance alone.
    Refresh,
}

/// Page state, owned by the application.
#[derive(Debug, Clone, Default)]
pub struct State {
    pub opencode_go: Vec<SubscriptionPoint>,
    pub command_code: Vec<SubscriptionPoint>,
    /// Day of the month Command Code renews on, 1-28.
    pub billing_day: u8,
}

impl State {
    pub fn new(billing_day: u8) -> Self {
        Self {
            billing_day,
            ..Default::default()
        }
    }

    /// Reloads both providers' logged readings.
    pub fn reload(&mut self) {
        self.opencode_go = storage::subscription_usage_history(storage::PROVIDER_OPENCODE_GO, 30)
            .unwrap_or_default();
        self.command_code = storage::subscription_usage_history(storage::PROVIDER_COMMAND_CODE, 60)
            .unwrap_or_default();
    }
}

/// Draws the page.
pub fn show(
    ui: &mut egui::Ui,
    view: &View<'_>,
    snapshot: &Snapshot,
    state: &mut State,
    configured: &std::collections::BTreeSet<String>,
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

    // Only configured subscriptions take up space. Leaving a placeholder for each
    // unconfigured provider would turn into clutter once more are added.
    let configured: Vec<Provider> = Provider::ALL
        .into_iter()
        .filter(|provider| provider.is_configured(configured))
        .collect();

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
                for (slot, provider) in chunk.iter().enumerate() {
                    match provider {
                        Provider::OpenCodeGo => {
                            opencode_go_card(&mut columns[slot], view, snapshot, state)
                        }
                        Provider::CommandCode => command_code_card(
                            &mut columns[slot],
                            view,
                            snapshot,
                            state,
                            &mut billing_day,
                        ),
                    }
                }
            });
        }
    }

    if refresh {
        return Some(Action::Refresh);
    }
    billing_day.map(Action::BillingDay)
}

fn opencode_go_card(ui: &mut egui::Ui, view: &View<'_>, snapshot: &Snapshot, state: &State) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.set_min_height(super::SUBSCRIPTION_CARD_HEIGHT);
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
                let message = format!("{} {error}", view.text("og_refresh_failed"));
                ui.add(
                    egui::Label::new(RichText::new(message).color(palette.destructive).size(12.0))
                        .truncate(),
                )
                .on_hover_text(error);
            }
        }
    });

    // No subscription, no chart.
    if !matches!(snapshot.opencode_go, Subscription::NotConfigured) {
        usage_chart(ui, view, view.text("og_title"), &state.opencode_go, None);
    }
}

fn command_code_card(
    ui: &mut egui::Ui,
    view: &View<'_>,
    snapshot: &Snapshot,
    state: &State,
    billing_day: &mut Option<u8>,
) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.set_min_height(super::SUBSCRIPTION_CARD_HEIGHT);
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
                let message = format!("{} {error}", view.text("cc_refresh_failed"));
                ui.add(
                    egui::Label::new(RichText::new(message).color(palette.destructive).size(12.0))
                        .truncate(),
                )
                .on_hover_text(error);
            }
        }
    });

    // No subscription, no chart.
    if !matches!(snapshot.command_code, Subscription::NotConfigured) {
        if let Some(edited) = usage_chart(
            ui,
            view,
            view.text("group_cc"),
            &state.command_code,
            Some(state.billing_day),
        ) {
            *billing_day = Some(edited);
        }
    }
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

/// Per-day consumption for one provider, with an optional billing-day field.
///
/// Returns the edited day when the field changes.
fn usage_chart(
    ui: &mut egui::Ui,
    view: &View<'_>,
    title: &str,
    points: &[SubscriptionPoint],
    billing_day: Option<u8>,
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

                if let Some(day) = billing_day {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // The field, its label and the heading all share one size:
                        // egui aligns rows by the middle of each item, so mixing
                        // sizes leaves them sitting on different baselines.
                        ui.scope(|ui| {
                            ui.style_mut()
                                .text_styles
                                .insert(egui::TextStyle::Button, egui::FontId::proportional(16.0));

                            let mut edited = day;
                            if ui
                                .add(
                                    egui::DragValue::new(&mut edited)
                                        .range(1..=dsmon_core::config::MAX_BILLING_DAY)
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
