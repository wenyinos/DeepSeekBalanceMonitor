//! Application shell: window, sidebar navigation and theme switching.

use egui::{Align2, Color32, CornerRadius, FontId, Frame, Margin, Sense};

use crate::fonts::DIGITS_FAMILY;
use crate::theme::{self, Palette, ThemeMode};

/// Pages reachable from the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Status,
    History,
    Settings,
}

impl Page {
    const ALL: [Page; 3] = [Page::Status, Page::History, Page::Settings];

    fn label(self) -> &'static str {
        match self {
            Page::Status => "状态",
            Page::History => "历史",
            Page::Settings => "设置",
        }
    }
}

/// Starts the interface. Both platform binaries call this.
pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(dsmon_core::APP_NAME)
            .with_inner_size([960.0, 620.0])
            .with_min_inner_size([760.0, 520.0]),
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
    mode: ThemeMode,
    /// Kept alive so the tray icon stays registered for the whole session.
    _tray: crate::tray::TrayHandle,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::fonts::install(&cc.egui_ctx);
        theme::install(&cc.egui_ctx);

        let mode = ThemeMode::System;
        theme::set_mode(&cc.egui_ctx, mode);

        let quit_ctx = cc.egui_ctx.clone();
        let tray = crate::tray::spawn("--", theme::current(&cc.egui_ctx), move || {
            quit_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        });

        Self {
            page: Page::Status,
            mode,
            _tray: tray,
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let palette = theme::current(ui.ctx());

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
                ui.label(egui::RichText::new(dsmon_core::APP_NAME).strong());
                ui.add_space(14.0);
                for page in Page::ALL {
                    if nav_item(ui, &palette, page.label(), self.page == page).clicked() {
                        self.page = page;
                    }
                }
            });

        egui::CentralPanel::default()
            .frame(
                Frame::NONE
                    .fill(palette.bg_app)
                    .inner_margin(Margin::same(16)),
            )
            .show(ui, |ui| match self.page {
                Page::Status => status_page(ui, &palette),
                Page::History => history_page(ui, &palette),
                Page::Settings => settings_page(ui, &palette, &mut self.mode),
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

/// Panel with the standard card styling.
fn card(ui: &mut egui::Ui, palette: &Palette, contents: impl FnOnce(&mut egui::Ui)) {
    Frame::NONE
        .fill(palette.bg_panel)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            contents(ui);
        });
    ui.add_space(12.0);
}

/// Renders a figure with the embedded digits face.
fn figure(ui: &mut egui::Ui, palette: &Palette, text: &str, size: f32) {
    ui.label(
        egui::RichText::new(text)
            .font(FontId::new(size, egui::FontFamily::Name(DIGITS_FAMILY.into())))
            .color(palette.text_primary),
    );
}

fn status_page(ui: &mut egui::Ui, palette: &Palette) {
    card(ui, palette, |ui| {
        ui.label(
            egui::RichText::new("DeepSeek 余额")
                .color(palette.text_secondary)
                .small(),
        );
        ui.add_space(4.0);
        figure(ui, palette, "--.--", 34.0);
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("尚未配置 API Key，请在设置中填入。")
                .color(palette.text_secondary),
        );
    });

    card(ui, palette, |ui| {
        ui.label(egui::RichText::new("服务状态").strong());
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("等待首次检查")
                .color(palette.text_secondary)
                .small(),
        );
    });
}

fn history_page(ui: &mut egui::Ui, palette: &Palette) {
    card(ui, palette, |ui| {
        ui.label(egui::RichText::new("余额历史").strong());
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("暂无历史数据。配置 API Key 后开始记录。")
                .color(palette.text_secondary)
                .small(),
        );
    });
}

fn settings_page(ui: &mut egui::Ui, palette: &Palette, mode: &mut ThemeMode) {
    let mut selected = *mode;

    card(ui, palette, |ui| {
        ui.label(egui::RichText::new("外观").strong());
        ui.add_space(8.0);
        for (candidate, label) in [
            (ThemeMode::System, "跟随系统"),
            (ThemeMode::Light, "日间"),
            (ThemeMode::Dark, "夜间"),
        ] {
            ui.radio_value(&mut selected, candidate, label);
        }
    });

    card(ui, palette, |ui| {
        ui.label(egui::RichText::new("版本").strong());
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(format!("v{}", dsmon_core::VERSION))
                .color(palette.text_secondary)
                .small(),
        );
    });

    if selected != *mode {
        *mode = selected;
        theme::set_mode(ui.ctx(), selected);
    }
}
