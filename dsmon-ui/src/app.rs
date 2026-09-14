//! Application shell: window, sidebar navigation and the page dispatch.

use std::path::PathBuf;

use dsmon_core::config::AppConfig;
use dsmon_core::monitor::Monitor;
use dsmon_core::storage;
use egui::{Align2, Color32, CornerRadius, FontId, Frame, Margin, Sense};

use crate::i18n::tr;
use crate::theme::{self, Palette};
use crate::views::{self, View};

/// Pages reachable from the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Page {
    /// One balance provider, identified by its catalog key.
    Balance(String),
    Subscriptions,
    Settings,
}

impl Page {
    /// Label for the fixed pages; balance pages build their own from the
    /// platform's display name.
    fn label(&self, lang: &str) -> &'static str {
        match self {
            Page::Balance(_) => "",
            Page::Subscriptions => tr(lang, "subscription_tab"),
            Page::Settings => tr(lang, "settings_tab"),
        }
    }
}

/// Starts the interface. Both platform binaries call this.
pub fn run() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(dsmon_core::APP_NAME)
        .with_inner_size([960.0, 620.0])
        .with_min_inner_size([760.0, 520.0]);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        dsmon_core::APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

struct App {
    page: Page,
    config: AppConfig,
    monitor: Monitor,
    history: views::history::State,
    subscriptions: views::subscriptions::State,
    settings: views::settings::State,
    /// Platforms that hold a key, independent of what the last poll returned.
    configured: std::collections::BTreeSet<String>,
    /// The tray icon and the commands picked in its menu.
    tray: crate::tray::Tray,
    /// Set when the tray asked to quit, so the close request is honoured
    /// instead of the window hiding itself.
    quitting: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::fonts::install(&cc.egui_ctx);

        let config = AppConfig::load();
        apply_theme(&cc.egui_ctx, &config);

        let monitor = Monitor::start(config.clone());
        let configured = configured_platforms();
        let settings = views::settings::State::new(config.clone(), configured.clone());

        let tray =
            crate::tray::Tray::spawn(&cc.egui_ctx, &config.ui_language, &icon_theme(&config));

        let billing_day = config.billing_day_command_code;
        let mut app = Self {
            page: Page::Balance(dsmon_core::storage::KEY_DEEPSEEK.to_owned()),
            config,
            monitor,
            history: views::history::State::default(),
            subscriptions: views::subscriptions::State::new(billing_day),
            settings,
            configured,
            tray,
            quitting: false,
        };
        app.reload_history();
        app.subscriptions.reload();
        app
    }

    /// Reloads the records for the current filters and the balance page on
    /// screen, since each provider keeps its own history.
    fn reload_history(&mut self) {
        self.history.platform = match &self.page {
            Page::Balance(key) => key.clone(),
            _ => dsmon_core::storage::KEY_DEEPSEEK.to_owned(),
        };

        let days = self.history.days;
        let currency = self.history.currency.clone();
        self.history.currencies =
            storage::history_currencies(&self.history.platform, days).unwrap_or_default();
        self.history.records =
            storage::history_records(&self.history.platform, days, currency.as_deref(), 5000)
                .unwrap_or_default();
    }

    /// Stores whatever the key fields hold. Blank fields are left alone.
    /// Returns false when a write fails, with the reason in the notice.
    fn store_keys(&mut self) -> bool {
        let mut failure = None;
        for (platform, value) in &self.settings.keys {
            let result = match storage::classify_key_input(value) {
                storage::KeyInput::Keep => continue,
                storage::KeyInput::Clear => storage::delete_secret(platform),
                storage::KeyInput::Set(secret) => storage::store_secret(platform, secret),
            };
            if let Err(error) = result {
                failure = Some(error);
                break;
            }
        }

        match failure {
            Some(error) => {
                self.settings.notice = Some(error);
                false
            }
            None => true,
        }
    }

    /// Saves the keys entered in the credentials card, in place.
    fn save_keys(&mut self) {
        if !self.store_keys() {
            return;
        }

        for value in self.settings.keys.values_mut() {
            value.clear();
        }
        self.settings.pending_clear = false;
        self.settings.configured = configured_platforms();
        self.configured = self.settings.configured.clone();

        let lang = self.config.ui_language.clone();
        self.settings.notice = Some(tr(&lang, "og_credentials_saved").to_owned());
        self.monitor.refresh();
    }

    /// Persists the settings draft, including any newly entered keys.
    fn save_settings(&mut self, ctx: &egui::Context) {
        let draft = self.settings.draft.clone();
        if let Err(error) = draft.save() {
            self.settings.notice = Some(error);
            return;
        }

        if !self.store_keys() {
            return;
        }

        let language_changed = self.config.ui_language != draft.ui_language;
        self.config = draft;
        apply_theme(ctx, &self.config);
        let lang = self.config.ui_language.clone();
        if language_changed {
            self.tray.set_language(&lang);
        }
        self.tray.set_widget_visible(self.config.widget_enabled);
        self.settings.reset(self.config.clone());
        self.settings.notice = Some(tr(&lang, "og_credentials_saved").to_owned());
        self.monitor.refresh();
    }

    /// Acts on whatever the user picked in the tray menu.
    fn drain_tray_commands(&mut self, ctx: &egui::Context) {
        use crate::tray::Command;

        for command in self.tray.take_commands() {
            match command {
                Command::ToggleWidget => {
                    self.config.widget_enabled = !self.config.widget_enabled;
                    let _ = self.config.save();
                    self.tray.set_widget_visible(self.config.widget_enabled);
                }
                Command::OpenWindow => show_window(ctx),
                Command::Refresh => self.monitor.refresh(),
                Command::ToggleScheme => self.toggle_scheme(ctx),
                Command::OpenSettings => {
                    self.page = Page::Settings;
                    show_window(ctx);
                }
                Command::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    /// Flips the interface between the light and the dark scheme and remembers
    /// the choice.
    fn toggle_scheme(&mut self, ctx: &egui::Context) {
        let mode = theme::toggled(ctx.theme());
        theme::set_mode(ctx, mode);
        self.config.ui_theme = theme::mode_to_config(mode).to_owned();
        let _ = self.config.save();
    }

    /// Closing the window leaves the application running in the tray.
    fn hide_on_close(&self, ctx: &egui::Context) {
        if self.quitting || !ctx.input(|input| input.viewport().close_requested()) {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    /// Copies the earlier build's data in, without touching its database.
    fn import_from_legacy(&mut self) {
        let lang = self.config.ui_language.clone();
        self.settings.notice = Some(match storage::import_from_legacy() {
            Ok(summary) => format!(
                "{} {} {} · {} {}",
                tr(&lang, "import_done"),
                summary.secrets,
                tr(&lang, "import_keys"),
                summary.history_records,
                tr(&lang, "import_rows"),
            ),
            Err(error) => error,
        });

        // Show what arrived straight away.
        self.reload_history();
        self.subscriptions.reload();
        self.monitor.refresh();
    }

    /// Writes the visible records to a CSV file.
    fn export_history(&mut self) {
        let path = export_target(&self.config);
        let csv = views::history::export(&self.history.records);
        let lang = self.config.ui_language.clone();

        self.history.notice = Some(match std::fs::write(&path, csv) {
            Ok(()) => format!("{} {}", tr(&lang, "export_success"), path.display()),
            Err(error) => format!("{} {error}", tr(&lang, "export_failed")),
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let style = theme::Style::from_config(&self.config.theme);
        let palette = theme::current(ui.ctx(), style, &self.config.icon_colors);
        let lang = self.config.ui_language.clone();
        let view = View {
            palette: &palette,
            lang: &lang,
        };
        let snapshot = self.monitor.snapshot();
        let mut switch = false;

        // Frames are drawn on demand, so without a heartbeat a reading that
        // lands between two interactions would sit unseen — on screen and in
        // the tray.
        ui.ctx().request_repaint_after(if snapshot.checking {
            std::time::Duration::from_millis(500)
        } else {
            std::time::Duration::from_secs(1)
        });

        // The tray carries the latest reading and answers what was picked in
        // it; the window hides rather than exits when it is closed.
        self.drain_tray_commands(ui.ctx());
        self.tray.publish(
            &crate::tray::status(&snapshot, &self.config, &lang),
            &icon_theme(&self.config),
        );
        self.hide_on_close(ui.ctx());

        egui::Panel::left("navigation")
            .exact_size(190.0)
            .resizable(false)
            .frame(
                Frame::NONE
                    .fill(palette.bg_sidebar)
                    .inner_margin(Margin::same(12)),
            )
            .show(ui, |ui| {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(dsmon_core::APP_NAME)
                        .color(palette.text_primary)
                        .strong(),
                );
                ui.add_space(14.0);

                // One entry per *configured* balance platform — decided by the
                // stored keys, not by the last poll, so the pages are there from
                // the first frame.
                for meta in dsmon_core::catalog::implemented()
                    .filter(|meta| meta.mode == dsmon_core::catalog::Mode::Payg)
                {
                    if !self.configured.contains(meta.key) {
                        continue;
                    }
                    let selected = matches!(&self.page, Page::Balance(key) if key == meta.key);
                    let label = format!("{} {}", meta.display_name, tr(&lang, "balance_word"));
                    if nav_item(ui, &palette, &label, selected).clicked() && !selected {
                        self.page = Page::Balance(meta.key.to_owned());
                        self.reload_history();
                    }
                }

                for page in [Page::Subscriptions, Page::Settings] {
                    let selected = self.page == page;
                    if nav_item(ui, &palette, page.label(&lang), selected).clicked() && !selected {
                        // Decide before the value moves into `self.page`.
                        let is_subscriptions = page == Page::Subscriptions;
                        self.page = page;
                        if is_subscriptions {
                            self.subscriptions.reload();
                        }
                    }
                }

                // Scheme switch, pinned to the bottom of the sidebar.
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(4.0);
                    let label = if palette.dark {
                        tr(&lang, "day_mode")
                    } else {
                        tr(&lang, "night_mode")
                    };
                    if ui.button(label).clicked() {
                        switch = true;
                    }
                });
            });

        if switch {
            self.toggle_scheme(ui.ctx());
        }

        egui::CentralPanel::default()
            .frame(
                Frame::NONE
                    .fill(palette.bg_app)
                    .inner_margin(Margin::same(16)),
            )
            .show(ui, |ui| {
                // The status page fills the window so it never scrolls; the
                // other pages scroll when their content grows.
                match &self.page {
                    Page::Balance(platform) => {
                        if let Some(action) =
                            views::status::show(ui, &view, &snapshot, &mut self.history, platform)
                        {
                            match action {
                                views::status::Action::Refresh => self.monitor.refresh(),
                                views::status::Action::ReloadHistory => self.reload_history(),
                                views::status::Action::ExportHistory => self.export_history(),
                            }
                        }
                    }
                    Page::Subscriptions => {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            if let Some(action) = views::subscriptions::show(
                                ui,
                                &view,
                                &snapshot,
                                &mut self.subscriptions,
                                &self.configured,
                            ) {
                                match action {
                                    views::subscriptions::Action::BillingDay(day) => {
                                        self.subscriptions.billing_day = day;
                                        self.config.billing_day_command_code = day;
                                        let _ = self.config.save();
                                    }
                                    views::subscriptions::Action::Refresh => {
                                        self.monitor.refresh_subscriptions()
                                    }
                                }
                            }
                        });
                    }
                    Page::Settings => {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            if let Some(action) =
                                views::settings::show(ui, &view, &mut self.settings)
                            {
                                match action {
                                    views::settings::Action::Save => self.save_settings(ui.ctx()),
                                    views::settings::Action::SaveKeys => {
                                        // Clearing a key waits for a second click.
                                        if self.settings.has_clear_request() {
                                            self.settings.pending_clear = true;
                                        } else {
                                            self.save_keys();
                                        }
                                    }
                                    views::settings::Action::ImportLegacy => {
                                        self.import_from_legacy()
                                    }
                                    views::settings::Action::ConfirmClear => {
                                        self.settings.pending_clear = false;
                                        self.save_keys();
                                    }
                                    views::settings::Action::Cancel => {
                                        self.settings.reset(self.config.clone())
                                    }
                                    views::settings::Action::OpenReleases => {
                                        open_url(views::settings::RELEASES_URL)
                                    }
                                    views::settings::Action::Preview => {
                                        apply_theme(ui.ctx(), &self.settings.draft)
                                    }
                                }
                            }
                        });
                    }
                }
            });
    }
}

/// Sidebar entry: a capsule that fills with the accent colour when selected.
fn nav_item(ui: &mut egui::Ui, palette: &Palette, label: &str, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 36.0), Sense::click());

    let fill = if selected {
        palette.accent
    } else if response.hovered() {
        palette.bg_hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(18), fill);

    let text_color = if selected {
        palette.on_accent
    } else {
        palette.text_primary
    };
    ui.painter().text(
        rect.left_center() + egui::vec2(16.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        text_color,
    );

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Platforms that already hold a key, for the settings page to group by.
fn configured_platforms() -> std::collections::BTreeSet<String> {
    dsmon_core::catalog::PLATFORMS
        .iter()
        .filter(|meta| matches!(storage::read_secret(meta.key), Ok(Some(_))))
        .map(|meta| meta.key.to_owned())
        .collect()
}

/// The window icon, scaled down from the bundled artwork.
fn window_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../../assets/AppIcon.png");
    let image = image::load_from_memory(bytes).ok()?;
    let image = image
        .resize(256, 256, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width: 256,
        height: 256,
    })
}

/// The tray icon's colour scheme, taken from the configuration.
fn icon_theme(config: &AppConfig) -> dsmon_core::icon::IconTheme {
    dsmon_core::icon::IconTheme {
        style: config.theme.clone(),
        custom: config.icon_colors.clone(),
    }
}

/// Applies a configuration's theme, without writing anything to disk.
fn apply_theme(ctx: &egui::Context, config: &AppConfig) {
    theme::install(
        ctx,
        theme::Style::from_config(&config.theme),
        &config.icon_colors,
    );
    theme::set_mode(ctx, theme::mode_from_config(&config.ui_theme));
}

/// Where an export lands: the configured directory, or the home directory.
fn export_target(config: &AppConfig) -> PathBuf {
    let name = format!(
        "deepseek-balance-history-{}.csv",
        chrono::Local::now().format("%Y%m%d")
    );
    let configured = config.export_path.trim();
    if configured.is_empty() {
        home_dir().join(name)
    } else {
        PathBuf::from(configured).join(name)
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Brings the main window back from the tray.
fn show_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
}

/// Hands a URL to the desktop.
fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();

    #[cfg(windows)]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
}
