//! Settings page: credentials, general options, alerts and data handling.

use dsmon_core::config::{
    AppConfig, ALERT_MODES, LANGUAGES, MAX_INTERVAL_MINUTES, MAX_RETENTION_DAYS,
    MAX_THRESHOLD_YUAN, MIN_INTERVAL_MINUTES, MIN_RETENTION_DAYS, UI_THEMES,
};
use egui::RichText;

use super::{card, View};
use crate::theme::{Palette, Style};
use dsmon_core::catalog::Mode;

/// Where the release page lives, opened from the About card.
pub const RELEASES_URL: &str = "https://github.com/wenyinos/DeepSeekBalanceMonitor/releases";

/// What the page asks the application to do after a click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Write the draft to disk and store any new keys.
    Save,
    /// Discard the draft and reload from disk.
    Cancel,
    /// Open the releases page in the browser.
    OpenReleases,
    /// Store the keys entered in the credentials card right away.
    SaveKeys,
    /// The user confirmed clearing the keys marked with `0`.
    ConfirmClear,
    /// Copy the earlier build's data into this one.
    ImportLegacy,
    /// Apply the draft's theme without saving, so the change is visible at once.
    Preview,
}

/// Page state, owned by the application.
#[derive(Debug, Clone)]
pub struct State {
    /// Edited copy of the configuration.
    pub draft: AppConfig,
    /// New key material, keyed by platform; empty means "keep what is stored".
    pub keys: std::collections::BTreeMap<String, String>,
    /// Feedback shown under the cards.
    pub notice: Option<String>,
    /// Set while an erase is waiting for a second click.
    pub pending_clear: bool,
}

impl State {
    pub fn new(config: AppConfig) -> Self {
        Self {
            draft: config,
            keys: std::collections::BTreeMap::new(),
            notice: None,
            pending_clear: false,
        }
    }

    /// Replaces the draft, for example after a cancel.
    pub fn reset(&mut self, config: AppConfig) {
        *self = Self::new(config);
    }
}

/// Draws the page.
pub fn show(ui: &mut egui::Ui, view: &View<'_>, state: &mut State) -> Option<Action> {
    let mut action = None;
    let mut preview = false;

    credentials_card(ui, view, state, &mut action);
    general_card(ui, view, state, &mut preview);
    alerts_card(ui, view, state);
    data_card(ui, view, state, &mut action);
    about_card(ui, view, &mut action);

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button(view.text("save")).clicked() {
            action = Some(Action::Save);
        }
        if ui.button(view.text("cancel")).clicked() {
            action = Some(Action::Cancel);
        }
        if let Some(notice) = &state.notice {
            ui.label(
                RichText::new(notice)
                    .color(view.palette.text_secondary)
                    .size(12.0),
            );
        }
    });

    // A theme change previews at once; an explicit Save or Cancel wins.
    if action.is_none() && preview {
        action = Some(Action::Preview);
    }
    action
}

impl State {
    /// Whether any field asks for its stored key to be erased.
    pub fn has_clear_request(&self) -> bool {
        self.keys.values().any(|value| {
            matches!(
                dsmon_core::storage::classify_key_input(value),
                dsmon_core::storage::KeyInput::Clear
            )
        })
    }
}

/// A settings row: label on the left, control on the right.
fn row(ui: &mut egui::Ui, palette: &Palette, label: &str, control: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(palette.text_primary));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            control(ui);
        });
    });
    ui.add_space(6.0);
}

/// Label for one of the [`UI_THEMES`] values.
fn mode_label(view: &View<'_>, value: &str) -> &'static str {
    match value {
        "light" => view.text("day_mode"),
        "dark" => view.text("night_mode"),
        _ => view.text("theme_system"),
    }
}

fn credentials_card(
    ui: &mut egui::Ui,
    view: &View<'_>,
    state: &mut State,
    action: &mut Option<Action>,
) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(view.text("group_credentials"))
                    .size(16.0)
                    .color(palette.text_primary)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(view.text("save_keys")).clicked() {
                    *action = Some(Action::SaveKeys);
                }
            });
        });

        ui.add_space(4.0);
        ui.label(
            RichText::new(view.text("keys_hint"))
                .color(palette.text_secondary)
                .size(12.0),
        );

        // Erasing a key is not reversible, so it waits for a second click.
        if state.pending_clear {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(view.text("clear_confirm"))
                        .color(palette.destructive)
                        .size(12.0),
                );
                if ui.button(view.text("confirm")).clicked() {
                    *action = Some(Action::ConfirmClear);
                }
                if ui.button(view.text("cancel")).clicked() {
                    state.pending_clear = false;
                }
            });
        }

        ui.add_space(10.0);

        // Balance providers first, then subscriptions: the two report different
        // things and are read in different places.
        for (heading, mode) in [
            ("payg_accounts", Mode::Payg),
            ("package_accounts", Mode::Package),
        ] {
            ui.label(
                RichText::new(view.text(heading))
                    .color(palette.text_secondary)
                    .size(12.0),
            );
            ui.add_space(6.0);

            for meta in dsmon_core::catalog::implemented().filter(|meta| meta.mode == mode) {
                let value = state.keys.entry(meta.key.to_owned()).or_default();
                key_field(ui, palette, meta.display_name, value);
            }

            // The two groups share a heading, and egui derives a widget's id
            // from its label — without a salt the second one would collide.
            egui::CollapsingHeader::new(view.text("pending_platforms"))
                .id_salt(heading)
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    for meta in dsmon_core::catalog::pending().filter(|meta| meta.mode == mode) {
                        let value = state.keys.entry(meta.key.to_owned()).or_default();
                        key_field(ui, palette, meta.display_name, value);
                    }
                });

            ui.add_space(10.0);
        }
    });
}

fn key_field(ui: &mut egui::Ui, palette: &Palette, label: &str, value: &mut String) {
    row(ui, palette, label, |ui| {
        ui.add(
            egui::TextEdit::singleline(value)
                .password(true)
                .hint_text("••••••••")
                .desired_width(260.0),
        );
    });
}

fn general_card(ui: &mut egui::Ui, view: &View<'_>, state: &mut State, preview: &mut bool) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.label(
            RichText::new(view.text("group_general"))
                .size(16.0)
                .color(palette.text_primary)
                .strong(),
        );
        ui.add_space(8.0);

        row(ui, palette, view.text("interval_label"), |ui| {
            ui.add(
                egui::DragValue::new(&mut state.draft.interval_minutes)
                    .range(MIN_INTERVAL_MINUTES..=MAX_INTERVAL_MINUTES)
                    .speed(0.5),
            );
        });

        row(ui, palette, view.text("language_label"), |ui| {
            egui::ComboBox::from_id_salt("ui-language")
                .selected_text(&state.draft.ui_language)
                .show_ui(ui, |ui| {
                    for language in LANGUAGES {
                        ui.selectable_value(
                            &mut state.draft.ui_language,
                            language.to_owned(),
                            language,
                        );
                    }
                });
        });

        row(ui, palette, view.text("theme_label"), |ui| {
            let mut changed = false;
            egui::ComboBox::from_id_salt("theme-style")
                .selected_text(view.text(Style::from_config(&state.draft.theme).label_key()))
                .show_ui(ui, |ui| {
                    for style in Style::ALL {
                        if ui
                            .selectable_value(
                                &mut state.draft.theme,
                                style.as_config().to_owned(),
                                view.text(style.label_key()),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
            *preview |= changed;
        });

        row(ui, palette, view.text("appearance_label"), |ui| {
            let mut changed = false;
            egui::ComboBox::from_id_salt("ui-theme")
                .selected_text(mode_label(view, &state.draft.ui_theme))
                .show_ui(ui, |ui| {
                    for value in UI_THEMES {
                        if ui
                            .selectable_value(
                                &mut state.draft.ui_theme,
                                value.to_owned(),
                                mode_label(view, value),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
            *preview |= changed;
        });

        ui.add_space(4.0);
        ui.checkbox(&mut state.draft.auto_start, view.text("auto_start"));
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.checkbox(&mut state.draft.proxy_enabled, view.text("proxy_enable"));
            ui.add_enabled(
                state.draft.proxy_enabled,
                egui::TextEdit::singleline(&mut state.draft.http_proxy)
                    .hint_text(view.text("proxy_placeholder"))
                    .desired_width(220.0),
            );
        });
    });
}

fn alerts_card(ui: &mut egui::Ui, view: &View<'_>, state: &mut State) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.label(
            RichText::new(view.text("group_query"))
                .size(16.0)
                .color(palette.text_primary)
                .strong(),
        );
        ui.add_space(8.0);

        row(ui, palette, view.text("threshold_label"), |ui| {
            ui.add(
                egui::DragValue::new(&mut state.draft.threshold_yuan)
                    .range(0.0..=MAX_THRESHOLD_YUAN)
                    .speed(0.1),
            );
        });

        row(ui, palette, view.text("alert_mode_label"), |ui| {
            egui::ComboBox::from_id_salt("alert-mode")
                .selected_text(&state.draft.alert_mode)
                .show_ui(ui, |ui| {
                    for mode in ALERT_MODES {
                        ui.selectable_value(&mut state.draft.alert_mode, mode.to_owned(), mode);
                    }
                });
        });

        ui.checkbox(
            &mut state.draft.api_alert_enabled,
            view.text("api_alert_label"),
        );
    });
}

fn data_card(ui: &mut egui::Ui, view: &View<'_>, state: &mut State, action: &mut Option<Action>) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.label(
            RichText::new(view.text("retention_label"))
                .size(16.0)
                .color(palette.text_primary)
                .strong(),
        );
        ui.add_space(8.0);

        row(ui, palette, view.text("retention_label"), |ui| {
            ui.add(
                egui::DragValue::new(&mut state.draft.retention_days)
                    .range(MIN_RETENTION_DAYS..=MAX_RETENTION_DAYS)
                    .speed(1.0),
            );
        });

        row(ui, palette, view.text("export_path_label"), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.draft.export_path)
                    .hint_text("~")
                    .desired_width(260.0),
            );
        });

        ui.add_space(10.0);
        ui.label(
            RichText::new(view.text("import_hint"))
                .color(palette.text_secondary)
                .size(12.0),
        );
        ui.add_space(4.0);
        if ui.button(view.text("import_legacy")).clicked() {
            *action = Some(Action::ImportLegacy);
        }
    });
}

fn about_card(ui: &mut egui::Ui, view: &View<'_>, action: &mut Option<Action>) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.label(
            RichText::new(dsmon_core::APP_NAME)
                .size(16.0)
                .color(palette.text_primary)
                .strong(),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("v{}", dsmon_core::VERSION))
                    .color(palette.text_secondary)
                    .size(12.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(RELEASES_URL).clicked() {
                    *action = Some(Action::OpenReleases);
                }
            });
        });
    });
}
