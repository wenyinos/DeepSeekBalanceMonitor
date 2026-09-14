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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Status,
    History,
    Settings,
}

impl Page {
    const ALL: [Page; 3] = [Page::Status, Page::History, Page::Settings];

    fn label(self, lang: &str) -> &'static str {
        match self {
            Page::Status => tr(lang, "balance_title"),
            Page::History => tr(lang, "history_tab"),
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
    settings: views::settings::State,
    /// Kept alive so the tray icon stays registered for the whole session.
    _tray: crate::tray::TrayHandle,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::fonts::install(&cc.egui_ctx);

        let config = AppConfig::load();
        apply_theme(&cc.egui_ctx, &config);

        let monitor = Monitor::start(config.clone());
        let settings = views::settings::State::new(config.clone());

        let quit_ctx = cc.egui_ctx.clone();
        let tray = crate::tray::spawn(
            "--",
            dsmon_core::icon::State::NoData,
            icon_theme(&config),
            move || {
                quit_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            },
        );

        let mut app = Self {
            page: Page::Status,
            config,
            monitor,
            history: views::history::State::default(),
            settings,
            _tray: tray,
        };
        app.reload_history();
        app
    }

    /// Reloads the records for the current filters.
    fn reload_history(&mut self) {
        let days = self.history.days;
        self.history.currencies = storage::history_currencies(days).unwrap_or_default();
        let currency = self.history.currency.clone();
        self.history.records =
            storage::history_records(days, currency.as_deref(), 5000).unwrap_or_default();
    }

    /// Persists the settings draft, including any newly entered keys.
    fn save_settings(&mut self, ctx: &egui::Context) {
        let draft = self.settings.draft.clone();
        if let Err(error) = draft.save() {
            self.settings.notice = Some(error);
            return;
        }

        for (key, value) in [
            (storage::KEY_DEEPSEEK, &self.settings.deepseek_key),
            (storage::KEY_OPENCODE_GO, &self.settings.opencode_key),
            (storage::KEY_COMMAND_CODE, &self.settings.command_code_key),
        ] {
            if value.trim().is_empty() {
                continue;
            }
            if let Err(error) = storage::store_secret(key, value) {
                self.settings.notice = Some(error);
                return;
            }
        }

        self.config = draft;
        apply_theme(ctx, &self.config);
        let lang = self.config.ui_language.clone();
        self.settings.reset(self.config.clone());
        self.settings.notice = Some(tr(&lang, "og_credentials_saved").to_owned());
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
        let mut switch = None;

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

                for page in Page::ALL {
                    let selected = self.page == page;
                    if nav_item(ui, &palette, page.label(&lang), selected).clicked() && !selected {
                        self.page = page;
                        if page == Page::History {
                            self.reload_history();
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
                        switch = Some(theme::toggled(ui.ctx().theme()));
                    }
                });
            });

        if let Some(mode) = switch {
            theme::set_mode(ui.ctx(), mode);
            self.config.ui_theme = theme::mode_to_config(mode).to_owned();
            let _ = self.config.save();
        }

        egui::CentralPanel::default()
            .frame(
                Frame::NONE
                    .fill(palette.bg_app)
                    .inner_margin(Margin::same(16)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| match self.page {
                    Page::Status => {
                        if views::status::show(ui, &view, &snapshot) {
                            self.monitor.refresh();
                        }
                    }
                    Page::History => {
                        if let Some(action) = views::history::show(ui, &view, &mut self.history) {
                            match action {
                                views::history::Action::Reload => self.reload_history(),
                                views::history::Action::Export => self.export_history(),
                            }
                        }
                    }
                    Page::Settings => {
                        if let Some(action) = views::settings::show(ui, &view, &mut self.settings) {
                            match action {
                                views::settings::Action::Save => self.save_settings(ui.ctx()),
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
                    }
                });
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

/// Hands a URL to the desktop.
fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();

    #[cfg(windows)]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
}
