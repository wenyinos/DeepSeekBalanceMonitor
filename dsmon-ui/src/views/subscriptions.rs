//! Subscriptions page: quota windows for the paid plans, two per row.
//!
//! Which plans appear, what they are called and which windows they have all
//! come from the catalog, so a plan brings only its client with it — the page
//! takes care of the rest.

use std::collections::BTreeMap;

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
    /// The Command Code billing day was edited for one account.
    BillingDay(String, u8),
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

    /// Reloads every account's logged readings.
    pub fn reload(&mut self, accounts: &[dsmon_core::config::AccountConfig]) {
        self.history = accounts
            .iter()
            .filter(|account| {
                catalog::find(&account.platform).is_some_and(|meta| meta.mode == Mode::Package)
            })
            .map(|account| {
                let points = storage::subscription_usage_history(
                    &account.id,
                    &account.platform,
                    "monthly",
                    HISTORY_DAYS,
                )
                .unwrap_or_default();
                (account.id.clone(), points)
            })
            .collect();
    }
}

/// Draws the page.
pub fn show(
    ui: &mut egui::Ui,
    view: &View<'_>,
    snapshot: &Snapshot,
    state: &mut State,
    accounts: &[dsmon_core::config::AccountConfig],
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

    // Only accounts that hold a key take up space. One plan can appear more
    // than once, once per account.
    let configured: Vec<(&PlatformMeta, &dsmon_core::config::AccountConfig)> = accounts
        .iter()
        .filter(|account| account.enabled)
        .filter(|account| matches!(storage::read_secret(&account.id), Ok(Some(_))))
        .filter_map(|account| {
            catalog::find(&account.platform)
                .filter(|meta| meta.mode == Mode::Package)
                .map(|meta| (meta, account))
        })
        .collect();
    let mut billing_day = None;

    if configured.is_empty() {
        ui.label(
            RichText::new(view.text("no_subscriptions"))
                .color(view.palette.text_secondary)
                .size(12.0),
        );
    } else {
        // One height for every card on the page, sized by the plan that carries
        // the most windows: a card that followed its own contents left the page
        // ragged, and the row is easier to read when the cards line up.
        let height = super::subscription_card_height(
            configured
                .iter()
                .map(|(meta, _)| meta.windows.len())
                .max()
                .unwrap_or(2),
        );

        // Two per row, so a growing list keeps the same rhythm.
        for chunk in configured.chunks(2) {
            ui.columns(2, |columns| {
                for (slot, (meta, account)) in chunk.iter().enumerate() {
                    plan_card(
                        &mut columns[slot],
                        view,
                        snapshot,
                        state,
                        meta,
                        account,
                        height,
                        &mut billing_day,
                    );
                }
            });
        }
    }

    if refresh {
        return Some(Action::Refresh);
    }
    billing_day.map(|(account, day)| Action::BillingDay(account, day))
}

/// One plan: its windows in a card, its chart underneath.
///
/// `height` is the page's one card height, set as both the floor and the
/// ceiling so a plan with two windows keeps the same shape as one with three.
fn plan_card(
    ui: &mut egui::Ui,
    view: &View<'_>,
    snapshot: &Snapshot,
    state: &State,
    meta: &PlatformMeta,
    account: &dsmon_core::config::AccountConfig,
    height: f32,
    billing_day: &mut Option<(String, u8)>,
) -> egui::Rect {
    let palette = view.palette;
    let reading = snapshot.packages.get(&account.id);
    let title = if account.name.trim().is_empty() {
        meta.display_name.to_owned()
    } else {
        account.name.clone()
    };

    let panel = card(ui, palette, |ui| {
        ui.set_min_height(height);
        ui.label(
            RichText::new(title.clone())
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
                        pace_line(view, palette, snapshot, &account.id, name),
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
        .get(&account.id)
        .map(Vec::as_slice)
        .unwrap_or_default();
    if matches!(reading, Some(Subscription::Loaded(_))) {
        let account_day = account.billing_day.unwrap_or(state.billing_day);
        if let Some(edited) = usage_chart(
            ui,
            view,
            &title,
            points,
            meta.key == storage::KEY_COMMAND_CODE,
            account_day,
        ) {
            *billing_day = Some((account.id.clone(), edited));
        }
    }

    panel
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

    /// One plan's card, rendered off-screen with `rows` windows' worth of floor
    /// under it, reporting the height the card came out.
    ///
    /// Off-screen because egui lays out on the CPU, so no window is needed, and
    /// with the application's own style — its item spacing is eight points where
    /// egui's default is three, which is worth a couple of points a row and made
    /// an earlier version of these figures measure short.
    ///
    /// A floor of zero asks for the card's natural height: what it draws, with
    /// nothing added.
    fn card_height(plan: &str, rows: usize) -> f32 {
        let context = egui::Context::default();
        crate::fonts::install(&context);
        let custom = BTreeMap::new();
        crate::theme::install(&context, Default::default(), &custom);
        crate::theme::set_mode(&context, crate::theme::ThemeMode::Dark);
        let palette = crate::theme::Palette::new(egui::Theme::Dark, Default::default(), &custom);
        let view = View {
            palette: &palette,
            lang: "zh",
        };
        let meta = catalog::find(plan).expect("the plan is in the catalog");

        // Every window the plan declares, each with a pace line: the tallest a
        // card can come out.
        let mut package = PackageQuota::new();
        let mut rates = BTreeMap::new();
        for name in meta.windows {
            package.insert(
                (*name).to_owned(),
                dsmon_core::model::QuotaWindow::from_percent(20.0, 3 * 3600),
            );
            rates.insert(
                (*name).to_owned(),
                dsmon_core::model::WindowRate {
                    percent_per_hour: 0.2,
                    hours_left: Some(30.0),
                    reset_in_sec: 3 * 3600,
                },
            );
        }

        let mut snapshot = Snapshot::default();
        snapshot
            .packages
            .insert(plan.to_owned(), Subscription::Loaded(package));
        snapshot.window_rates.insert(plan.to_owned(), rates);

        let state = State::new(1);
        let height = crate::views::subscription_card_height(rows);
        let account = dsmon_core::config::AccountConfig {
            id: plan.to_owned(),
            platform: plan.to_owned(),
            name: meta.display_name.to_owned(),
            enabled: true,
            billing_day: None,
        };
        let mut billing_day = None;

        let mut measured = 0.0;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 800.0),
            )),
            ..Default::default()
        };
        // Twice: egui settles its own layout on the first pass.
        for _ in 0..2 {
            let mut output = context.run_ui(input.clone(), |ui| {
                measured = plan_card(
                    ui,
                    &view,
                    &snapshot,
                    &state,
                    meta,
                    &account,
                    height,
                    &mut billing_day,
                )
                .height();
            });
            // Nothing renders these frames, so the texture deltas go on purpose.
            output.textures_delta.clear();
        }
        measured
    }

    /// The page's one card height has room for the fullest card, and every card
    /// is drawn at it.
    ///
    /// The first assertion is the one that matters: a height short of what the
    /// card draws leaves that card taller than the one beside it, which is
    /// exactly how the page came out ragged. The second is the padding that
    /// makes the row line up.
    #[test]
    fn a_card_takes_the_height_the_page_gives_it() {
        let page = crate::views::subscription_card_height(3);

        // The frame's own 12-point margins, inside and outside.
        let natural = card_height("opencode_go", 0);
        assert!(
            page + 24.0 >= natural,
            "three windows need {natural} but the page gives {}",
            page + 24.0
        );

        assert_eq!(
            card_height("opencode_go", 3),
            page + 24.0,
            "the fullest card is drawn at the page's height"
        );
        assert_eq!(
            card_height("minimax_token_cn", 3),
            page + 24.0,
            "and a plan with two windows is padded to it"
        );
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
