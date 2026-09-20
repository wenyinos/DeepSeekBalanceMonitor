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
    prefer_x11();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(dsmon_core::APP_NAME)
        // A desktop looks the icon up under this name, which is the one the
        // packages will install a `.desktop` file for.
        .with_app_id("deepseek-balance-monitor")
        .with_inner_size([960.0, 620.0])
        .with_min_inner_size([760.0, 520.0]);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }

    // What the tray asks for, and what a second launch asks for, arrive
    // through two queues: one is the menu, the other is another process
    // saying it could not start.
    let commands = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let ctx_holder = std::sync::Arc::new(std::sync::OnceLock::new());

    let instance = match crate::instance::claim(
        crate::instance::Names::APPLICATION,
        starts_minimized(),
        std::sync::Arc::clone(&requests),
        std::sync::Arc::clone(&ctx_holder),
    ) {
        crate::instance::Role::First(guard) => guard,
        // The copy that is already running has the window; this one is done.
        crate::instance::Role::Latecomer => return Ok(()),
    };

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        dsmon_core::APP_NAME,
        options,
        Box::new(move |cc| {
            let _ = ctx_holder.set(cc.egui_ctx.clone());
            Ok(Box::new(App::new(cc, commands, requests, instance)))
        }),
    )
}

struct App {
    page: Page,
    config: AppConfig,
    monitor: Monitor,
    /// The local interface the desktop widget reads. Dropped with the window.
    _widget_api: Option<dsmon_core::widget_api::Server>,
    history: views::history::State,
    subscriptions: views::subscriptions::State,
    settings: views::settings::State,
    /// Platforms that hold a key, independent of what the last poll returned.
    configured: std::collections::BTreeSet<String>,
    /// The tray icon and the commands picked in its menu.
    tray: crate::tray::Tray,
    /// The commands picked in the tray menu.
    commands: std::sync::Arc<std::sync::Mutex<Vec<crate::tray::Command>>>,
    /// The requests a second launch makes, which it cannot carry out itself.
    requests: std::sync::Arc<std::sync::Mutex<Vec<crate::instance::Request>>>,
    /// The claim on being the running copy, kept for the process's lifetime.
    _instance: crate::instance::Guard,
    /// What the last reading left behind, for the notifications it calls for.
    alerts: crate::notify::Watch,
    /// Set by a start that was asked to stay out of the way, holding the moment
    /// by which it stops waiting for the tray icon to be there.
    hide_at_start: Option<std::time::Instant>,
    /// Set when the tray asked to quit, so the close request is honoured
    /// instead of the window hiding itself.
    quitting: bool,
    /// When the stored widget setting was last read back, so the tray entry
    /// follows a change the widget made to it.
    widget_checked: std::time::Instant,
}

impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        commands: std::sync::Arc<std::sync::Mutex<Vec<crate::tray::Command>>>,
        requests: std::sync::Arc<std::sync::Mutex<Vec<crate::instance::Request>>>,
        instance: crate::instance::Guard,
    ) -> Self {
        crate::fonts::install(&cc.egui_ctx);

        // This build used to keep its files where the earlier build reads its
        // own, so what is still there is moved to this build's own directory
        // before anything is read — the configuration included.
        if let Err(error) = dsmon_core::adopt::earlier_files() {
            let _ = storage::log_line(&format!(
                "The files left in the shared directory could not be moved: {error}"
            ));
        }

        let config = AppConfig::load();
        apply_theme(&cc.egui_ctx, &config);

        // Starting with the session is a setting the system is told about, and
        // that telling lives outside this application's own storage: it is
        // reconciled on every run, so a setting carried over from another build
        // — or an executable that has moved since — still works. Whether the
        // system holds the entry afterwards is logged, since that is the one
        // question a log can answer about starting with the session; a refusal
        // is also shown on the settings page, where the setting lives.
        let start_up = dsmon_core::autostart::set_enabled(
            dsmon_core::autostart::Program::Application,
            config.auto_start,
        );
        match &start_up {
            Ok(()) => {
                let _ = storage::log_line(&format!(
                    "Start-up entry: asked for {}, the system holds {}.",
                    config.auto_start,
                    dsmon_core::autostart::is_enabled(dsmon_core::autostart::Program::Application)
                ));
            }
            Err(error) => {
                let _ =
                    storage::log_line(&format!("The start-up entry could not be written: {error}"));
            }
        }

        let monitor = Monitor::start(config.clone());

        // The desktop widget reads everything it draws from here. A port that
        // cannot be taken is written to the log and the application carries on
        // without the interface: the window and the tray do not depend on it.
        let widget_api = match dsmon_core::widget_api::start(monitor.handle()) {
            Ok(server) => {
                let _ = storage::log_line(&format!(
                    "The local interface is listening on {}.",
                    server.url()
                ));
                Some(server)
            }
            Err(error) => {
                let _ = storage::log_line(&error);
                None
            }
        };

        let configured = configured_platforms();
        let mut settings = views::settings::State::new(config.clone(), configured.clone());
        if let Err(error) = &start_up {
            settings.notice = Some(format!(
                "{} {error}",
                tr(&config.ui_language, "auto_start_failed")
            ));
        }

        // eframe only hands the window icon to Windows and macOS itself; on
        // Linux the window comes up without one unless it is passed along.
        if let Some(icon) = window_icon() {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Icon(Some(std::sync::Arc::new(icon))));
        }

        let tray = crate::tray::Tray::spawn(
            &cc.egui_ctx,
            &config.ui_language,
            &icon_theme(&config),
            config.widget_enabled,
            std::sync::Arc::clone(&commands),
        );

        // The widget is a program of its own, and whether it should be up is a
        // setting this one owns. Starting it here is what makes the tray entry
        // mean "from now on" rather than "until the next start".
        if config.widget_enabled {
            if let Err(error) = crate::widget::start_process() {
                let _ =
                    storage::log_line(&format!("The desktop widget could not be started: {error}"));
            }
        }

        let billing_day = config.billing_day_command_code;
        let mut app = Self {
            page: Page::Balance(dsmon_core::storage::KEY_DEEPSEEK.to_owned()),
            config,
            monitor,
            _widget_api: widget_api,
            history: views::history::State::default(),
            subscriptions: views::subscriptions::State::new(billing_day),
            settings,
            configured,
            tray,
            commands,
            requests,
            _instance: instance,
            alerts: crate::notify::Watch::default(),
            widget_checked: std::time::Instant::now(),
            hide_at_start: starts_minimized()
                .then(|| std::time::Instant::now() + std::time::Duration::from_secs(15)),
            quitting: false,
        };
        app.reload_history();
        app.subscriptions.reload();
        app.announce_start_up();
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
        let auto_start_changed = self.config.auto_start != draft.auto_start;
        self.config = draft;
        if auto_start_changed {
            self.apply_auto_start();
        }
        apply_theme(ctx, &self.config);
        let lang = self.config.ui_language.clone();
        if language_changed {
            self.tray.set_language(&lang);
        }
        self.settings.reset(self.config.clone());
        self.settings.notice = Some(tr(&lang, "og_credentials_saved").to_owned());
        self.monitor.refresh();
    }

    /// Everything that has been asked of the application since the last frame.
    fn take_commands(&self) -> Vec<crate::tray::Command> {
        match self.commands.lock() {
            Ok(mut queue) => std::mem::take(&mut *queue),
            Err(_) => Vec::new(),
        }
    }

    /// Acts on whatever the user picked in the tray menu.
    fn drain_tray_commands(&mut self, ctx: &egui::Context) {
        use crate::tray::Command;

        for command in self.take_commands() {
            match command {
                Command::ShowBalance => {
                    let lang = self.config.ui_language.clone();
                    crate::notify::send(crate::notify::balance_message(
                        &self.monitor.snapshot(),
                        &lang,
                    ));
                }
                Command::OpenWindow => show_window(ctx),
                Command::Refresh => self.monitor.refresh(),
                Command::OpenSettings => {
                    self.page = Page::Settings;
                    show_window(ctx);
                }
                Command::ToggleWidget => self.toggle_widget(),
                Command::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    /// Brings the desktop widget up or down for good.
    ///
    /// Only the setting and the start are done here: hiding is the widget's own
    /// business, since it is the process that has to end — it reads the same
    /// file and closes itself.
    fn toggle_widget(&mut self) {
        self.config.widget_enabled = !self.config.widget_enabled;
        if let Err(error) = self.config.save() {
            let _ = storage::log_line(&format!("The configuration could not be written: {error}"));
        }

        if self.config.widget_enabled {
            if let Err(error) = crate::widget::start_process() {
                let _ =
                    storage::log_line(&format!("The desktop widget could not be started: {error}"));
            }
        }
        self.tray.set_widget(self.config.widget_enabled);
    }

    /// A second launch cannot raise a window that belongs to this process, so
    /// it asks; showing the window is the whole of what it can ask for.
    fn drain_instance_requests(&self, ctx: &egui::Context) {
        let asked = match self.requests.lock() {
            Ok(mut queue) => std::mem::take(&mut *queue),
            Err(_) => Vec::new(),
        };
        if asked.contains(&crate::instance::Request::Show) {
            show_window(ctx);
        }
    }

    /// The widget's own close button writes the setting the tray entry shows,
    /// so that entry follows the file rather than only what this process
    /// remembers. Read on a timer: the file is small, but it is not free.
    fn follow_widget_setting(&mut self) {
        const EVERY: std::time::Duration = std::time::Duration::from_secs(2);
        if self.widget_checked.elapsed() < EVERY {
            return;
        }
        self.widget_checked = std::time::Instant::now();

        let stored = AppConfig::load().widget_enabled;
        if stored != self.config.widget_enabled {
            self.config.widget_enabled = stored;
            self.tray.set_widget(stored);
        }
    }

    /// Flips the interface between the light and the dark scheme and remembers
    /// the choice. The sidebar button is the only way in.
    fn toggle_scheme(&mut self, ctx: &egui::Context) {
        let mode = theme::toggled(ctx.theme());
        theme::set_mode(ctx, mode);
        self.config.ui_theme = theme::mode_to_config(mode).to_owned();
        let _ = self.config.save();
    }

    /// Closing the window puts it out of the way and leaves the application
    /// running; only the tray's quit entry ends it.
    fn hide_on_close(&self, ctx: &egui::Context) {
        if self.quitting || !ctx.input(|input| input.viewport().close_requested()) {
            return;
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);

        // Out of sight only while the tray can bring the window back: with no
        // icon registered, the task bar is the way in, so the window goes there
        // instead of disappearing.
        if window_can_hide() && self.tray.is_registered() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }

    /// Writes or removes the entry that starts this build with the session.
    fn apply_auto_start(&mut self) {
        let lang = self.config.ui_language.clone();
        if let Err(error) = dsmon_core::autostart::set_enabled(
            dsmon_core::autostart::Program::Application,
            self.config.auto_start,
        ) {
            self.settings.notice = Some(format!("{} {error}", tr(&lang, "auto_start_failed")));
        }
    }

    /// The two notices the previous build raised once, at start-up: nothing is
    /// configured yet, or the database had to be built again.
    fn announce_start_up(&mut self) {
        let lang = self.config.ui_language.clone();

        if self.configured.is_empty() {
            self.page = Page::Settings;
            crate::notify::send(crate::notify::missing_key_message(&lang));
        }
        if storage::take_recreated_notice() {
            crate::notify::send(crate::notify::recreated_database_message(&lang));
        }
    }

    /// Drops the records outside the retention window and compacts the file,
    /// which is what actually brings its size down.
    fn clear_old_data(&mut self) {
        let lang = self.config.ui_language.clone();
        self.settings.notice = Some(
            match storage::clear_older_than(self.config.retention_days) {
                Ok(cleared) => crate::i18n::cleared_notice(
                    &lang,
                    cleared.balance_rows + cleared.subscription_rows,
                    &storage::format_size(cleared.reclaimed),
                ),
                Err(error) => error,
            },
        );

        self.reload_history();
        self.subscriptions.reload();
    }

    /// Copies the earlier build's data in, without touching its database.
    fn import_from_legacy(&mut self) {
        let lang = self.config.ui_language.clone();
        self.settings.notice = Some(match storage::import_from_legacy() {
            Ok(summary) => {
                let mut notice = format!(
                    "{} {} {} · {} {}",
                    tr(&lang, "import_done"),
                    summary.secrets,
                    tr(&lang, "import_keys"),
                    summary.history_records,
                    tr(&lang, "import_rows"),
                );
                if summary.unreadable_secrets > 0 {
                    // Said plainly: those keys belong to the earlier Windows
                    // build, which protected them with DPAPI.
                    notice.push_str(&format!(
                        " · {} {}",
                        summary.unreadable_secrets,
                        tr(&lang, "import_unreadable"),
                    ));
                }
                notice
            }
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
    /// Everything that has to happen whether or not a window is on screen.
    ///
    /// eframe calls this in both cases and `ui` only when there is something to
    /// draw, so the tray — the one surface a hidden or minimized window leaves —
    /// is driven from here. It is also where a close request has to be answered:
    /// cancelling it from `ui` would miss whenever the window happens to be
    /// minimized or behind another one, and the application would quit.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let snapshot = self.monitor.snapshot();
        let lang = self.config.ui_language.clone();

        // Frames are drawn on demand, so without a heartbeat a reading that
        // lands between two interactions would sit unseen — on screen and in
        // the tray.
        ctx.request_repaint_after(if snapshot.checking {
            std::time::Duration::from_millis(500)
        } else {
            std::time::Duration::from_secs(1)
        });

        self.drain_tray_commands(ctx);
        self.drain_instance_requests(ctx);
        self.follow_widget_setting();
        self.tray.publish(
            &crate::tray::status(&snapshot, &self.config, &lang),
            &icon_theme(&self.config),
        );

        for message in self.alerts.judge(&snapshot, &self.config, &lang) {
            crate::notify::send(message);
        }

        // A start asked for by the session stays out of the way — but only once
        // the tray icon is really there, because the window is the only other
        // way into the application. A desktop that has not taken the icon yet is
        // waited for; one that never takes it leaves the window on screen,
        // rather than hiding the last way in.
        //
        // Asking here rather than at startup is also what makes it stick: eframe
        // shows every window once it has painted one frame.
        if let Some(deadline) = self.hide_at_start {
            if self.tray.is_registered() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.hide_at_start = None;
            } else if std::time::Instant::now() >= deadline {
                let _ = storage::log_line(
                    "The tray icon is not there; the window stays on screen this run.",
                );
                self.hide_at_start = None;
            }
        }

        self.hide_on_close(ctx);
    }

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
                            if let Some(action) = views::settings::show(
                                ui,
                                &view,
                                &mut self.settings,
                                snapshot.newer_version.as_deref(),
                            ) {
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
                                    views::settings::Action::AutoStart(enabled) => {
                                        // Remembered here and now, not left to
                                        // "save": every start reconciles the
                                        // entry against the configuration, so a
                                        // switch that was never saved would be
                                        // written back out at the next one.
                                        self.config.auto_start = enabled;
                                        if let Err(error) = self.config.save() {
                                            let _ = storage::log_line(&format!(
                                                "The configuration could not be written: {error}"
                                            ));
                                        }
                                        self.apply_auto_start();
                                    }
                                    views::settings::Action::ClearOldData => self.clear_old_data(),
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

/// The window icon: the application's own artwork, so the task bar and the
/// window list carry the same mark on both platforms.
///
/// 128 px rather than the file's largest frame on purpose. An X11 property
/// change carries at most 65535 words, and a 256×256 icon needs two more than
/// that: the larger frame is refused without a word and the window comes up
/// without an icon, which is how this was found.
fn window_icon() -> Option<egui::IconData> {
    let icon = dsmon_core::icon::app_icon(128)?;
    Some(egui::IconData {
        rgba: icon.rgba,
        width: icon.width,
        height: icon.height,
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

    if window_can_hide() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    } else {
        // Wayland refuses to unminimize, so the closest thing to calling the
        // window back is asking the compositor to activate it — the protocol
        // behind that request is meant for exactly this. Whether it also
        // unminimizes is the compositor's decision, so the task bar entry is
        // still the route that always works.
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
            egui::UserAttentionType::Critical,
        ));
    }
}

/// Whether this platform lets a window disappear and be brought back.
///
/// Wayland leaves that to the compositor: winit's backend ignores a request to
/// hide a surface, refuses to unminimize one, and its focus request does
/// nothing, so a close there has to minimize instead and the window comes back
/// from the task bar.
fn window_can_hide() -> bool {
    #[cfg(unix)]
    {
        std::env::var_os("WAYLAND_DISPLAY").is_none()
    }

    #[cfg(not(unix))]
    {
        true
    }
}

/// Whether the session asked for a start without the window: the entry written
/// for starting at login passes `--minimized`.
fn starts_minimized() -> bool {
    std::env::args().any(|argument| argument == "--minimized")
}

/// Starts through XWayland rather than on Wayland itself.
///
/// Closing the window to the tray is the point of this application, and Wayland
/// will not let a window be hidden — a hidden window cannot be brought back
/// either, since it is the compositor that owns both. Under X11 both work, so
/// the session is asked for X11 whenever it offers one: winit picks Wayland the
/// moment a session advertises it, which leaves clearing the variable as the
/// only way to say otherwise.
///
/// A session with no X display at all keeps its native Wayland window, and
/// `DSMON_NATIVE_WAYLAND=1` asks for that on purpose.
#[cfg(target_os = "linux")]
pub(crate) fn prefer_x11() {
    if std::env::var_os("DSMON_NATIVE_WAYLAND").is_some() {
        return;
    }
    if std::env::var_os("DISPLAY").is_none() {
        return;
    }

    std::env::remove_var("WAYLAND_DISPLAY");
    let _ = dsmon_core::storage::log_line(
        "Opening the window through XWayland, which can hide it to the tray.",
    );
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn prefer_x11() {}

/// Hands a URL to the desktop.
fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();

    #[cfg(windows)]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
}
