//! The widget's cards: the burn rate, one per subscription, and the two states
//! the panel can be in when there is nothing to show.

use std::collections::BTreeMap;

use dsmon_core::platforms::format_reset_seconds;
use dsmon_core::widget_api::{Day, Platform};
use egui::{Color32, CornerRadius, FontId, RichText};

use crate::fonts::DIGITS_FAMILY;
use crate::i18n::tr;
use crate::theme::Palette;
use crate::views::{progress_line, usage_color};

use super::charts;

/// What a card is asking the widget to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Ask the application for a poll.
    Refresh,
    /// Start the application, which is not running.
    StartApplication,
    /// Raise the application so its settings page can be reached.
    OpenSettings,
    /// The range button was pressed.
    NextRange,
    /// An activity tab was picked: show that subscription's grid, or every
    /// one of them when it is `None`.
    ShowActivity(Option<String>),
}

/// Everything a card needs besides its own contents.
pub struct Look<'a> {
    pub palette: &'a Palette,
    pub lang: &'a str,
    /// How solid the panel is. Cards sit on it, so they fade with it.
    pub opacity: f32,
    /// Whether the balance curves are drawn at all (`widget_show_trend`).
    pub show_trend: bool,
}

/// Surface of a card, on top of the panel.
pub fn card(ui: &mut egui::Ui, look: &Look<'_>, contents: impl FnOnce(&mut egui::Ui)) {
    let tint = if look.palette.dark {
        Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 20)
    } else {
        Color32::from_rgba_unmultiplied(0x00, 0x00, 0x00, 12)
    };
    let mut frame = egui::Frame::NONE
        .fill(tint)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(12));
    if look.opacity >= 0.98 {
        // Nothing shows through an opaque panel, so the cards have to carry
        // their own surface instead of tinting the desktop.
        frame = frame.fill(look.palette.bg_hover);
    }
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        contents(ui);
    });
    ui.add_space(8.0);
}

/// One balance provider's card: what is left, how fast it is going, and the
/// curve both come from.
///
/// There is one per configured provider, the same rule the subscription cards
/// follow: a panel that showed only the first one would quietly hide every
/// other account the application is polling.
pub fn balance_card(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    platform: &Platform,
    range: u64,
    with_range: bool,
) -> Option<Action> {
    let mut action = None;

    card(ui, look, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(&platform.display)
                    .size(13.0)
                    .strong()
                    .color(look.palette.text_primary),
            );
            // The range is one setting for every curve on the panel at once, so
            // its button lives on the first card rather than on each of them.
            if with_range {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if pill(ui, look, &format!("{range}d")).clicked() {
                        action = Some(Action::NextRange);
                    }
                });
            }
        });

        // The balance itself, above the rate that is derived from it: it is the
        // figure the panel is opened for.
        if let Some(balance) = platform.balances.first() {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(tr(look.lang, "balance_word"))
                        .size(10.5)
                        .color(look.palette.text_secondary),
                );
                ui.label(
                    RichText::new(format_amount(balance.total_balance))
                        .font(FontId::new(
                            20.0,
                            egui::FontFamily::Name(DIGITS_FAMILY.into()),
                        ))
                        .color(look.palette.text_primary),
                );
                ui.label(
                    RichText::new(&balance.currency)
                        .size(10.5)
                        .color(look.palette.text_secondary),
                );
            });
            ui.add_space(1.0);
        }

        let rate = platform.rate.as_ref();
        let currency = rate
            .map(|rate| rate.currency.as_str())
            .or_else(|| {
                platform
                    .balances
                    .first()
                    .map(|balance| balance.currency.as_str())
            })
            .unwrap_or("");

        ui.horizontal(|ui| match rate {
            Some(rate) => {
                ui.label(
                    RichText::new(format_amount(rate.hourly_rate))
                        .font(FontId::new(
                            16.0,
                            egui::FontFamily::Name(DIGITS_FAMILY.into()),
                        ))
                        .color(look.palette.text_primary),
                );
                ui.label(
                    RichText::new(format!("/h {currency}"))
                        .size(10.0)
                        .color(look.palette.text_secondary),
                );
                ui.label(
                    RichText::new(format!(
                        "   {} {}",
                        tr(look.lang, "estimated_remaining"),
                        format_busy_hours(rate.busy_hours_left)
                    ))
                    .size(10.0)
                    .color(look.palette.text_secondary),
                );
            }
            None => {
                ui.label(
                    RichText::new(tr(look.lang, "not_enough_data"))
                        .size(12.0)
                        .color(look.palette.text_secondary),
                );
            }
        });

        // `widget_show_trend` is what this setting is for: a panel that wants
        // only the figures can drop the curve without giving up the card.
        if look.show_trend {
            charts::balance_curve(ui, look.palette, &platform.series);
        }
    });

    action
}

/// One subscription's card: a row per quota window.
pub fn subscription(ui: &mut egui::Ui, look: &Look<'_>, platform: &Platform) {
    card(ui, look, |ui| {
        ui.label(
            RichText::new(&platform.display)
                .size(13.0)
                .strong()
                .color(look.palette.text_primary),
        );
        ui.add_space(6.0);

        if platform.windows.is_empty() && platform.daily.is_empty() {
            // A plan that is configured but reported nothing: say so rather than
            // showing an empty card with a title.
            ui.label(
                RichText::new(tr(look.lang, "package_refresh_failed"))
                    .size(10.0)
                    .color(look.palette.text_secondary),
            );
            return;
        }

        // Two lines per window, not three: the reset is a footnote to the
        // percentage, so it sits beside it on the same row rather than taking
        // one of its own. Several windows then fit where one used to.
        for window in &platform.windows {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(tr(look.lang, &window.name_key))
                        .size(10.5)
                        .color(look.palette.text_secondary),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(dsmon_core::history::format_percent(window.usage_percent))
                            .font(FontId::new(
                                13.0,
                                egui::FontFamily::Name(DIGITS_FAMILY.into()),
                            ))
                            .color(look.palette.text_primary),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} {}",
                            tr(look.lang, "widget_reset_label"),
                            format_reset_seconds(window.reset_in_sec)
                        ))
                        .size(9.5)
                        .color(look.palette.text_secondary),
                    );
                });
            });
            progress_line(
                ui,
                look.palette,
                (window.usage_percent / 100.0) as f32,
                usage_color(look.palette, window.usage_percent as f32),
            );
            ui.add_space(7.0);
        }

        // Windows the provider does not report at all. The dashed line is not
        // decoration: it keeps "not offered" from looking like "zero used".
        if platform.kind == "package" && platform.windows.is_empty() {
            ui.label(
                RichText::new(tr(look.lang, "widget_provider_note"))
                    .size(9.5)
                    .color(look.palette.text_secondary),
            );
        }
    });
}

/// The activity card: one grid, for every subscription or for one of them.
///
/// It sits at the bottom, under every subscription. The tabs beside the title
/// pick which grid is on screen: "all" is what it opens with, because the sum
/// is the figure somebody paying for several plans is after, and a single plan
/// is one click away when a busy week has to be traced back to whoever caused
/// it.
pub fn activity_card(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    platforms: &[Platform],
    chosen: Option<&str>,
) -> Option<Action> {
    let mut action = None;
    let plans: Vec<&Platform> = platforms
        .iter()
        .filter(|platform| platform.kind == "package")
        .collect();
    // A subscription that is no longer configured falls back to all of them,
    // rather than leaving the card looking empty and blaming nobody.
    let chosen = chosen.filter(|key| plans.iter().any(|plan| plan.key == *key));

    card(ui, look, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            ui.label(
                RichText::new(tr(look.lang, "widget_activity"))
                    .size(10.5)
                    .strong()
                    .color(look.palette.text_primary),
            );
            ui.add_space(4.0);

            if tab(ui, look, tr(look.lang, "widget_tab_all"), chosen.is_none()).clicked() {
                action = Some(Action::ShowActivity(None));
            }
            for plan in &plans {
                let selected = chosen == Some(plan.key.as_str());
                let response = tab(ui, look, first_word(&plan.display), selected);
                if response.clicked() {
                    action = Some(Action::ShowActivity(Some(plan.key.clone())));
                }
                // The full name is one hover away, since the tab cannot hold it
                // and several plans have to share one line.
                response.on_hover_text(&plan.display);
            }
        });
        ui.add_space(6.0);

        let days = match chosen {
            Some(key) => plans
                .iter()
                .find(|plan| plan.key == key)
                .map(|plan| plan.daily.clone())
                .unwrap_or_default(),
            None => merged_days(platforms),
        };
        charts::activity_grid(ui, look.palette, &days);
    });

    action
}

/// The first word of a platform's name: enough for a tab, where "Command Code"
/// and "GLM Coding Plan" would push each other off the line. The full name is
/// in the tooltip.
fn first_word(display: &str) -> &str {
    display.split_whitespace().next().unwrap_or(display)
}

/// One of the activity card's tabs. The chosen one is filled, so which grid is
/// on screen is readable without reading the labels twice.
fn tab(ui: &mut egui::Ui, look: &Look<'_>, text: &str, selected: bool) -> egui::Response {
    let font = crate::fonts::ui_font(10.0);
    let width = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), look.palette.text_primary)
        .size()
        .x
        + 20.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 20.0), egui::Sense::click());

    let fill = if selected {
        look.palette.accent
    } else if response.hovered() {
        ui.visuals().widgets.hovered.bg_fill.gamma_multiply(0.5)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(10), fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        font,
        if selected {
            crate::theme::readable_on(look.palette.accent)
        } else {
            look.palette.text_secondary
        },
    );
    response
}

/// Every subscription's days added up, one entry per date.
///
/// The payload carries one series per platform, so the total is worked out
/// here: adding them needs nothing from the application, and a client that
/// wants the same figure can do the same sum.
fn merged_days(platforms: &[Platform]) -> Vec<Day> {
    let mut totals: BTreeMap<&str, f64> = BTreeMap::new();
    for platform in platforms.iter().filter(|p| p.kind == "package") {
        for day in &platform.daily {
            *totals.entry(day.date.as_str()).or_default() += day.used;
        }
    }

    totals
        .into_iter()
        .map(|(date, used)| Day {
            date: date.to_owned(),
            used,
            // The grid works the weekday out from the date itself.
            weekday: 0,
        })
        .collect()
}

/// The strip that says the readings on screen are old.
pub fn offline_banner(ui: &mut egui::Ui, look: &Look<'_>) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(9),
        Color32::from_rgba_unmultiplied(
            look.palette.warning.r(),
            look.palette.warning.g(),
            look.palette.warning.b(),
            0xE6,
        ),
    );
    ui.painter().circle_filled(
        rect.left_center() + egui::vec2(14.0, 0.0),
        3.5,
        look.palette.warning,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(24.0, 0.0),
        egui::Align2::LEFT_CENTER,
        tr(look.lang, "widget_offline_stale"),
        crate::fonts::ui_font(10.5),
        look.palette.text_primary,
    );
    ui.add_space(10.0);
}

/// The strip that says the application speaks a payload this widget does not
/// know.
///
/// The contract asks for exactly this: a client that cannot read the format
/// says so, rather than showing whatever happened to parse and letting the
/// missing half look like an empty account.
pub fn version_banner(ui: &mut egui::Ui, look: &Look<'_>) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(9),
        Color32::from_rgba_unmultiplied(
            look.palette.destructive.r(),
            look.palette.destructive.g(),
            look.palette.destructive.b(),
            0xE6,
        ),
    );
    ui.painter().circle_filled(
        rect.left_center() + egui::vec2(14.0, 0.0),
        3.5,
        look.palette.destructive,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(24.0, 0.0),
        egui::Align2::LEFT_CENTER,
        tr(look.lang, "widget_version_mismatch"),
        crate::fonts::ui_font(10.5),
        look.palette.text_primary,
    );
    ui.add_space(10.0);
}

/// The card that stands in for everything while the application is away.
pub fn not_connected(ui: &mut egui::Ui, look: &Look<'_>) -> Option<Action> {
    let mut action = None;
    card(ui, look, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(4.0);
            ui.label(
                RichText::new(tr(look.lang, "widget_offline_title"))
                    .size(13.0)
                    .strong()
                    .color(look.palette.text_primary),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(tr(look.lang, "widget_offline_body"))
                    .size(10.0)
                    .color(look.palette.text_secondary),
            );
            ui.label(
                RichText::new(tr(look.lang, "widget_offline_hint"))
                    .size(9.5)
                    .color(look.palette.text_secondary),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 10.0) / 2.0;
                if filled_button(ui, look, width, tr(look.lang, "widget_start_app")).clicked() {
                    action = Some(Action::StartApplication);
                }
                if plain_button(ui, look, width, tr(look.lang, "widget_retry")).clicked() {
                    action = Some(Action::Refresh);
                }
            });
        });
    });
    action
}

/// Shown when the application is there but holds no key at all.
pub fn nothing_configured(ui: &mut egui::Ui, look: &Look<'_>) -> Option<Action> {
    let mut action = None;
    card(ui, look, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(6.0);
            ui.label(
                RichText::new(tr(look.lang, "widget_no_platforms"))
                    .size(11.0)
                    .color(look.palette.text_secondary),
            );
            ui.add_space(10.0);
            if filled_button(
                ui,
                look,
                ui.available_width() - 40.0,
                tr(look.lang, "widget_open_settings"),
            )
            .clicked()
            {
                action = Some(Action::OpenSettings);
            }
        });
    });
    action
}

/// A rounded label, as the platform and range pickers are drawn.
fn pill(ui: &mut egui::Ui, look: &Look<'_>, text: &str) -> egui::Response {
    let font = crate::fonts::ui_font(10.0);
    let width = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), look.palette.text_primary)
        .size()
        .x
        + 24.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 20.0), egui::Sense::click());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(10),
        ui.visuals().widgets.hovered.bg_fill.gamma_multiply(0.5),
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        font,
        look.palette.text_primary,
    );
    response
}

fn filled_button(ui: &mut egui::Ui, look: &Look<'_>, width: f32, text: &str) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width.max(60.0), 26.0), egui::Sense::click());
    let colour = if response.hovered() {
        look.palette.accent.gamma_multiply(0.85)
    } else {
        look.palette.accent
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(8), colour);
    // Black or white on the accent, decided by the fill's own brightness — the
    // rule the icon renderer has always used.
    let ink = crate::theme::readable_on(colour);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        crate::fonts::ui_font(10.0),
        ink,
    );
    response
}

fn plain_button(ui: &mut egui::Ui, look: &Look<'_>, width: f32, text: &str) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width.max(60.0), 26.0), egui::Sense::click());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(8),
        ui.visuals().widgets.inactive.bg_fill.gamma_multiply(0.6),
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        crate::fonts::ui_font(10.0),
        look.palette.text_primary,
    );
    response
}

/// Amounts as the rest of the interface writes them.
fn format_amount(value: f64) -> String {
    dsmon_core::history::format_amount(value)
}

/// "12d 3h", the way the status page writes the estimate.
fn format_busy_hours(hours: f64) -> String {
    crate::views::status::format_busy_hours(hours)
}
