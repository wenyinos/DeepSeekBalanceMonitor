//! Settings page: credentials, general options, alerts and data handling.

use dsmon_core::config::{
    AppConfig, ALERT_MODES, LANGUAGES, MAX_INTERVAL_MINUTES, MAX_RETENTION_DAYS,
    MAX_THRESHOLD_YUAN, MIN_INTERVAL_MINUTES, MIN_RETENTION_DAYS, UI_THEMES,
};
use egui::RichText;

use super::{card, View};
use crate::theme::{Palette, Style};

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
    /// Apply the draft's theme without saving, so the change is visible at once.
    Preview,
}

/// Page state, owned by the application.
#[derive(Debug, Clone)]
pub struct State {
    /// Edited copy of the configuration.
    pub draft: AppConfig,
    /// New key material; empty means "keep what is stored".
    pub deepseek_key: String,
    pub opencode_key: String,
    pub command_code_key: String,
    /// Whether the key fields show their content.
    pub reveal_keys: bool,
    /// Feedback shown under the cards.
    pub notice: Option<String>,
}

impl State {
    pub fn new(config: AppConfig) -> Self {
        Self {
            draft: config,
            deepseek_key: String::new(),
            opencode_key: String::new(),
            command_code_key: String::new(),
            reveal_keys: false,
            notice: None,
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

    credentials_card(ui, view, state);
    general_card(ui, view, state, &mut preview);
    alerts_card(ui, view, state);
    data_card(ui, view, state);
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

fn credentials_card(ui: &mut egui::Ui, view: &View<'_>, state: &mut State) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(view.text("group_credentials"))
                    .color(palette.text_primary)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(&mut state.reveal_keys, view.text("show_key"));
            });
        });
        ui.add_space(8.0);

        key_field(
            ui,
            palette,
            view.text("api_key_label"),
            &mut state.deepseek_key,
            state.reveal_keys,
        );
        ui.label(
            RichText::new(view.text("api_key_missing_body"))
                .color(palette.text_secondary)
                .size(12.0),
        );
        ui.add_space(10.0);

        key_field(
            ui,
            palette,
            view.text("og_api_key_label"),
            &mut state.opencode_key,
            state.reveal_keys,
        );
        ui.label(
            RichText::new(view.text("og_hint"))
                .color(palette.text_secondary)
                .size(12.0),
        );
        ui.add_space(10.0);

        key_field(
            ui,
            palette,
            view.text("cc_api_key_label"),
            &mut state.command_code_key,
            state.reveal_keys,
        );
        ui.label(
            RichText::new(view.text("cc_hint"))
                .color(palette.text_secondary)
                .size(12.0),
        );
    });
}

fn key_field(ui: &mut egui::Ui, palette: &Palette, label: &str, value: &mut String, reveal: bool) {
    row(ui, palette, label, |ui| {
        ui.add(
            egui::TextEdit::singleline(value)
                .password(!reveal)
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

fn data_card(ui: &mut egui::Ui, view: &View<'_>, state: &mut State) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.label(
            RichText::new(view.text("retention_label"))
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
    });
}

fn about_card(ui: &mut egui::Ui, view: &View<'_>, action: &mut Option<Action>) {
    let palette = view.palette;

    card(ui, palette, |ui| {
        ui.label(
            RichText::new(dsmon_core::APP_NAME)
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
