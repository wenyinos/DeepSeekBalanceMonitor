//! Settings page: credentials, general options, alerts and data handling.

use dsmon_core::config::{
    AppConfig, ALERT_MODES, LANGUAGES, MAX_INTERVAL_MINUTES, MAX_LINE_PERCENT, MAX_RETENTION_DAYS,
    MAX_THRESHOLD_YUAN, MIN_INTERVAL_MINUTES, MIN_RETENTION_DAYS,
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
    /// Drop the records that sit outside the retention window and compact the
    /// database, so its size comes back down.
    ClearOldData,
    /// Apply the draft's theme without saving, so the change is visible at once.
    Preview,
    /// The start-up setting was switched. It is told to the system at once
    /// rather than on save: it is not a draft value but a request to the
    /// desktop, and a ticked box behind an unsaved page is exactly what
    /// "starting with the session does not work" looked like.
    AutoStart(bool),
}

/// Page state, owned by the application.
#[derive(Debug, Clone)]
pub struct State {
    /// Edited copy of the configuration.
    pub draft: AppConfig,
    /// New key material, keyed by account id; empty means "keep what is
    /// stored", and a single `0` asks for the stored secret to be removed.
    pub keys: std::collections::BTreeMap<String, String>,
    /// Platform picked in the "add account" row, one per group heading.
    pub new_platforms: std::collections::BTreeMap<String, String>,
    /// Key typed for the account about to be added.
    pub new_key: String,
    /// Feedback shown under the cards.
    pub notice: Option<String>,
    /// Set while an erase is waiting for a second click.
    pub pending_clear: bool,
    /// Platforms that already have a key stored.
    pub configured: std::collections::BTreeSet<String>,
}

impl State {
    pub fn new(config: AppConfig, configured: std::collections::BTreeSet<String>) -> Self {
        Self {
            draft: config,
            keys: std::collections::BTreeMap::new(),
            new_platforms: std::collections::BTreeMap::new(),
            new_key: String::new(),
            notice: None,
            pending_clear: false,
            configured,
        }
    }

    /// Replaces the draft, for example after a cancel. The set of configured
    /// platforms stays as it is.
    pub fn reset(&mut self, config: AppConfig) {
        let configured = std::mem::take(&mut self.configured);
        *self = Self::new(config, configured);
    }
}

/// Draws the page.
pub fn show(
    ui: &mut egui::Ui,
    view: &View<'_>,
    state: &mut State,
    newer_version: Option<&str>,
) -> Option<Action> {
    let mut action = None;
    let mut preview = false;

    credentials_card(ui, view, state, &mut action);
    general_card(ui, view, state, &mut preview, &mut action);
    alerts_card(ui, view, state);
    data_card(ui, view, state, &mut action);
    about_card(ui, view, newer_version, &mut action);

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
        // things and are read in different places. Every account is a row; the
        // platform is picked when a row is added, not when the key is typed.
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

            let indices: Vec<usize> = state
                .draft
                .accounts
                .iter()
                .enumerate()
                .filter(|(_, account)| {
                    dsmon_core::catalog::find(&account.platform)
                        .is_some_and(|meta| meta.mode == mode)
                })
                .map(|(index, _)| index)
                .collect();

            let mut remove = None;
            for index in indices {
                let id = state.draft.accounts[index].id.clone();
                ui.push_id(id, |ui| {
                    account_row(ui, view, state, index, &mut remove);
                });
            }
            if let Some(index) = remove {
                state.draft.accounts.remove(index);
            }

            add_account_row(ui, view, state, heading, mode);
            ui.add_space(10.0);
        }
    });
}

/// One account: its name, its platform, its key field and whether it polls.
fn account_row(
    ui: &mut egui::Ui,
    view: &View<'_>,
    state: &mut State,
    index: usize,
    remove: &mut Option<usize>,
) {
    let palette = view.palette;
    let account = &mut state.draft.accounts[index];
    let platform = dsmon_core::catalog::find(&account.platform)
        .map(|meta| meta.display_name)
        .unwrap_or(account.platform.as_str())
        .to_owned();
    let key = state.keys.entry(account.id.clone()).or_default();

    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut account.name)
                .hint_text(view.text("account_name"))
                .desired_width(130.0),
        );
        ui.label(
            RichText::new(platform)
                .color(palette.text_secondary)
                .size(12.0),
        );
        ui.add(
            egui::TextEdit::singleline(key)
                .password(true)
                .hint_text("••••••••")
                .desired_width(210.0),
        );
        ui.checkbox(&mut account.enabled, "");
        if ui.small_button("✕").clicked() {
            *remove = Some(index);
        }
    });
}

/// The row that adds another account of one mode.
fn add_account_row(
    ui: &mut egui::Ui,
    view: &View<'_>,
    state: &mut State,
    heading: &str,
    mode: Mode,
) {
    let palette = view.palette;
    let platforms: Vec<&dsmon_core::catalog::PlatformMeta> = dsmon_core::catalog::PLATFORMS
        .iter()
        .filter(|meta| meta.implemented && meta.mode == mode)
        .collect();
    if platforms.is_empty() {
        return;
    }

    let mut chosen = state
        .new_platforms
        .get(heading)
        .cloned()
        .filter(|key| platforms.iter().any(|meta| meta.key == key))
        .unwrap_or_else(|| platforms[0].key.to_owned());
    let display = platforms
        .iter()
        .find(|meta| meta.key == chosen)
        .map(|meta| meta.display_name)
        .unwrap_or(chosen.as_str())
        .to_owned();

    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(format!("add-account-{heading}"))
            .selected_text(RichText::new(display).color(palette.text_secondary))
            .show_ui(ui, |ui| {
                for meta in &platforms {
                    ui.selectable_value(&mut chosen, meta.key.to_owned(), meta.display_name);
                }
            });
        ui.add(
            egui::TextEdit::singleline(&mut state.new_key)
                .password(true)
                .hint_text("••••••••")
                .desired_width(210.0),
        );
        if ui.button(view.text("add_account")).clicked() {
            let id = dsmon_core::accounts::new_id(&state.draft);
            let name = dsmon_core::accounts::next_name(&state.draft, &chosen);
            state
                .draft
                .accounts
                .push(dsmon_core::config::AccountConfig {
                    id: id.clone(),
                    platform: chosen.clone(),
                    name,
                    enabled: true,
                    billing_day: None,
                });
            if !state.new_key.trim().is_empty() {
                state.keys.insert(id, state.new_key.trim().to_owned());
            }
            state.new_key.clear();
        }
    });
    state.new_platforms.insert(heading.to_owned(), chosen);
}

fn general_card(
    ui: &mut egui::Ui,
    view: &View<'_>,
    state: &mut State,
    preview: &mut bool,
    action: &mut Option<Action>,
) {
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

        ui.add_space(4.0);
        if ui
            .checkbox(&mut state.draft.auto_start, view.text("auto_start"))
            .changed()
        {
            *action = Some(Action::AutoStart(state.draft.auto_start));
        }
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

        row(ui, palette, view.text("threshold_package_label"), |ui| {
            ui.add(
                egui::DragValue::new(&mut state.draft.threshold_package_percent)
                    .range(0.0..=MAX_LINE_PERCENT)
                    .speed(1.0),
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

        ui.checkbox(
            &mut state.draft.peak_alert_enabled,
            view.text("peak_alert_label"),
        );

        ui.checkbox(
            &mut state.draft.quota_alert_enabled,
            view.text("quota_alert_label"),
        );

        ui.checkbox(
            &mut state.draft.update_check_enabled,
            view.text("update_check_label"),
        );

        // The line for a day that is costing a lot. Zero is off, which is how
        // it ships: only the user knows what "a lot" is for their account.
        row(ui, palette, view.text("brisk_threshold_label"), |ui| {
            ui.add(
                egui::DragValue::new(&mut state.draft.brisk_threshold_yuan)
                    .range(0.0..=MAX_THRESHOLD_YUAN)
                    .speed(0.1),
            );
        });

        // The package counterpart, in percentage points of a plan's window.
        row(ui, palette, view.text("brisk_package_label"), |ui| {
            ui.add(
                egui::DragValue::new(&mut state.draft.brisk_package_percent)
                    .range(0.0..=MAX_LINE_PERCENT)
                    .speed(1.0),
            );
        });
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

        // Read afresh every frame: it is a couple of stat calls, and it means
        // the figure is right after a poll, a cleanup or a long run.
        row(ui, palette, view.text("db_size_label"), |ui| {
            ui.label(
                RichText::new(dsmon_core::storage::format_size(
                    dsmon_core::storage::database_size(),
                ))
                .color(palette.text_primary),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(crate::i18n::clear_button(
                        view.lang,
                        state.draft.retention_days,
                    ))
                    .clicked()
                {
                    *action = Some(Action::ClearOldData);
                }
            });
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

fn about_card(
    ui: &mut egui::Ui,
    view: &View<'_>,
    newer_version: Option<&str>,
    action: &mut Option<Action>,
) {
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

        // Only when the release page named one: a build that has moved on is
        // said here as well as in a notification, since the settings page is
        // where a user goes to look for the version.
        if let Some(latest) = newer_version {
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("{} v{latest}", view.text("update_available_label")))
                    .color(palette.warning)
                    .size(12.0),
            );
        }
    });
}
