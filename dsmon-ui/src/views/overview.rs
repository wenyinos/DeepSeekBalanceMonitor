//! Every account at a glance: totals per currency and one compact card per
//! account, the ones closest to their line first.

use std::collections::BTreeMap;

use dsmon_core::catalog::{self, Mode};
use dsmon_core::config::{AccountConfig, AppConfig};
use dsmon_core::history::format_amount;
use dsmon_core::model::preferred_balance;
use dsmon_core::monitor::{Snapshot, Subscription};
use egui::{FontId, RichText, Sense};

use super::{card, status_dot, usage_color, View};
use crate::fonts::DIGITS_FAMILY;

/// What the page asks the application to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Open the account's platform page, with this account selected.
    Open { platform: String, account: String },
}

/// Draws the page.
pub fn show(
    ui: &mut egui::Ui,
    view: &View<'_>,
    snapshot: &Snapshot,
    config: &AppConfig,
) -> Option<Action> {
    let mut action = None;
    let accounts: Vec<&AccountConfig> = dsmon_core::accounts::enabled(config).collect();

    if accounts.is_empty() {
        ui.label(
            RichText::new(view.text("overview_no_accounts"))
                .color(view.palette.text_secondary)
                .size(12.0),
        );
        return None;
    }

    totals(ui, view, snapshot, &accounts);
    ui.add_space(10.0);

    let mut ordered = accounts.clone();
    ordered.sort_by(|left, right| {
        risk(left, snapshot, config)
            .partial_cmp(&risk(right, snapshot, config))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for chunk in ordered.chunks(2) {
        ui.columns(2, |columns| {
            for (slot, account) in chunk.iter().enumerate() {
                if let Some(chosen) =
                    account_card(&mut columns[slot], view, snapshot, config, account)
                {
                    action = Some(chosen);
                }
            }
        });
        ui.add_space(8.0);
    }

    action
}

/// A figure no reading can be compared against sorts to the end.
fn risk(account: &AccountConfig, snapshot: &Snapshot, config: &AppConfig) -> f64 {
    if snapshot.balance_errors.contains_key(&account.id) {
        return -1.0;
    }

    let Some(meta) = catalog::find(&account.platform) else {
        return 2.0;
    };
    match meta.mode {
        Mode::Payg => {
            let Some(balances) = snapshot.balances.get(&account.id) else {
                return 2.0;
            };
            let Some((_, balance)) = preferred_balance(balances) else {
                return 2.0;
            };
            // The one line that exists is in yuan and belongs to DeepSeek; for
            // another provider the balance has no comparable line, so it sorts
            // by its own figure relative to a nominal one.
            let line = if account.alert_threshold > 0.0 {
                account.alert_threshold
            } else if account.platform == dsmon_core::storage::KEY_DEEPSEEK {
                config.threshold_yuan
            } else {
                0.0
            };
            if line > 0.0 {
                balance.total_balance / line
            } else {
                1.0
            }
        }
        Mode::Package => match snapshot.packages.get(&account.id) {
            Some(Subscription::Loaded(quota)) => quota
                .values()
                .map(|window| window.percent_remaining)
                .fold(f64::INFINITY, f64::min),
            Some(Subscription::Failed(_)) => -1.0,
            _ => 2.0,
        },
    }
}

/// The balances added up per currency. Currencies are never mixed.
fn totals(ui: &mut egui::Ui, view: &View<'_>, snapshot: &Snapshot, accounts: &[&AccountConfig]) {
    let mut sums: BTreeMap<String, f64> = BTreeMap::new();
    for account in accounts {
        if !catalog::find(&account.platform).is_some_and(|meta| meta.mode == Mode::Payg) {
            continue;
        }
        let Some(balances) = snapshot.balances.get(&account.id) else {
            continue;
        };
        let Some((currency, balance)) = preferred_balance(balances) else {
            continue;
        };
        *sums.entry(currency.clone()).or_default() += balance.total_balance;
    }

    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(view.text("overview_total"))
                .color(view.palette.text_secondary)
                .size(12.0),
        );
        if sums.is_empty() {
            ui.label(
                RichText::new("--")
                    .color(view.palette.text_secondary)
                    .size(12.0),
            );
            return;
        }
        for (currency, total) in sums {
            ui.label(
                RichText::new(format!("{} {currency}", format_amount(total)))
                    .color(view.palette.text_primary)
                    .strong(),
            );
        }
    });
}

fn account_card(
    ui: &mut egui::Ui,
    view: &View<'_>,
    snapshot: &Snapshot,
    config: &AppConfig,
    account: &AccountConfig,
) -> Option<Action> {
    let palette = view.palette;
    let meta = catalog::find(&account.platform)?;
    let error = snapshot.balance_errors.get(&account.id);
    let failed = error.is_some()
        || matches!(
            snapshot.packages.get(&account.id),
            Some(Subscription::Failed(_))
        );
    let dot = if failed {
        palette.destructive
    } else if low(account, snapshot, config) {
        palette.warning
    } else {
        palette.positive
    };

    let panel = card(ui, palette, |ui| {
        ui.set_min_height(96.0);
        ui.horizontal(|ui| {
            status_dot(ui, dot);
            ui.label(
                RichText::new(account.label())
                    .size(14.0)
                    .strong()
                    .color(palette.text_primary),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(meta.display_name)
                        .size(11.0)
                        .color(palette.text_secondary),
                );
            });
        });
        ui.add_space(4.0);

        match meta.mode {
            Mode::Payg => payg_body(ui, view, snapshot, account),
            Mode::Package => package_body(ui, view, snapshot, account),
        }

        if let Some(error) = error {
            ui.add(
                egui::Label::new(RichText::new(error).color(palette.destructive).size(11.0))
                    .truncate(),
            )
            .on_hover_text(error);
        }
    });

    let response = ui.interact(
        panel,
        ui.make_persistent_id(("overview-card", &account.id)),
        Sense::click(),
    );
    if response.clicked() {
        return Some(Action::Open {
            platform: account.platform.clone(),
            account: account.id.clone(),
        });
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    None
}

fn payg_body(ui: &mut egui::Ui, view: &View<'_>, snapshot: &Snapshot, account: &AccountConfig) {
    let palette = view.palette;
    let preferred = snapshot
        .balances
        .get(&account.id)
        .and_then(preferred_balance);
    let (amount, currency) = preferred
        .map(|(currency, balance)| (format_amount(balance.total_balance), currency.as_str()))
        .unwrap_or_else(|| ("--.--".to_owned(), "---"));

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(amount)
                .font(FontId::new(
                    22.0,
                    egui::FontFamily::Name(DIGITS_FAMILY.into()),
                ))
                .color(if preferred.is_some() {
                    palette.text_primary
                } else {
                    palette.text_secondary
                }),
        );
        ui.label(RichText::new(currency).color(palette.text_secondary));
    });
    if let Some((currency, spent)) = snapshot.today_spend.get(&account.id) {
        ui.label(
            RichText::new(format!(
                "{} {} {currency}",
                view.text("widget_today_spend"),
                format_amount(*spent)
            ))
            .size(11.0)
            .color(palette.text_secondary),
        );
    }
}

fn package_body(ui: &mut egui::Ui, view: &View<'_>, snapshot: &Snapshot, account: &AccountConfig) {
    let palette = view.palette;
    let Some(Subscription::Loaded(quota)) = snapshot.packages.get(&account.id) else {
        ui.label(
            RichText::new(view.text("not_checked"))
                .size(11.0)
                .color(palette.text_secondary),
        );
        return;
    };
    let Some((window, entry)) = quota.iter().min_by(|left, right| {
        left.1
            .percent_remaining
            .partial_cmp(&right.1.percent_remaining)
            .unwrap_or(std::cmp::Ordering::Equal)
    }) else {
        return;
    };

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{:.0}%", entry.percent_remaining))
                .font(FontId::new(
                    22.0,
                    egui::FontFamily::Name(DIGITS_FAMILY.into()),
                ))
                .color(palette.text_primary),
        );
        ui.label(
            RichText::new(view.text(catalog::window_label_key(window)))
                .size(11.0)
                .color(palette.text_secondary),
        );
    });
    super::progress_line(
        ui,
        palette,
        entry.percent_remaining as f32 / 100.0,
        usage_color(palette, entry.percent_remaining as f32),
    );
}

/// Whether the account sits below its line, which is what turns the dot amber.
fn low(account: &AccountConfig, snapshot: &Snapshot, config: &AppConfig) -> bool {
    let Some(meta) = catalog::find(&account.platform) else {
        return false;
    };
    match meta.mode {
        Mode::Payg => {
            let Some((_, balance)) = snapshot
                .balances
                .get(&account.id)
                .and_then(preferred_balance)
            else {
                return false;
            };
            let line = if account.alert_threshold > 0.0 {
                account.alert_threshold
            } else if account.platform == dsmon_core::storage::KEY_DEEPSEEK {
                config.threshold_yuan
            } else {
                0.0
            };
            account.alert_enabled && line > 0.0 && balance.total_balance < line
        }
        Mode::Package => snapshot
            .packages
            .get(&account.id)
            .and_then(|reading| match reading {
                Subscription::Loaded(quota) => quota
                    .values()
                    .map(|window| window.percent_remaining)
                    .fold(None, |lowest: Option<f64>, remaining| {
                        Some(lowest.map_or(remaining, |value| value.min(remaining)))
                    }),
                _ => None,
            })
            .is_some_and(|remaining| {
                account.alert_enabled
                    && remaining
                        < if account.alert_threshold > 0.0 {
                            account.alert_threshold
                        } else {
                            config.threshold_package_percent
                        }
            }),
    }
}
