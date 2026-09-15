//! The desktop widget: a small always-on-top panel that draws what the
//! application's local interface reports.
//!
//! Everything it shows comes from there; it has no poller, no database and no
//! keys of its own (see `docs/WIDGET.md` and `docs/INTERFACES.md`). What it
//! does own is its own look: the panel's opacity, its window, and following the
//! application's theme and language so the two never disagree.

mod cards;
mod charts;
mod client;
mod tray;

use std::time::{Duration, Instant};

use dsmon_core::config::{widget_opacity_level, AppConfig, WIDGET_OPACITY_LEVELS};
use dsmon_core::widget_api::{Payload, DAYS};
use egui::{Color32, CornerRadius, Sense, Stroke, ViewportCommand};

use crate::i18n::tr;
use crate::theme::{self, Palette};

/// The two sizes the settings page offers. Dragging the window overrides them.
const STANDARD: [f32; 2] = [380.0, 600.0];
const COMPACT: [f32; 2] = [320.0, 330.0];
const MIN_SIZE: [f32; 2] = [280.0, 240.0];

const HEADER: f32 = 34.0;
const FOOTER: f32 = 26.0;
const PADDING: f32 = 14.0;

/// How wide the band along the left, right and bottom edges is, in points.
/// That band is what moves the window.
const EDGE: f32 = 8.0;

/// How big the two bottom corner handles are, in points. Those are what change
/// the window's height.
const CORNER: f32 = 18.0;

/// How long the window has to hold still before its place is written down.
const SETTLE: Duration = Duration::from_millis(700);

/// Starts the widget. The platform binary is a thin call to this.
pub fn run() -> eframe::Result<()> {
    crate::app::prefer_x11();

    let config = AppConfig::load();

    // The widget's own start-up entry, separate from the application's: it can
    // come up at login on its own — saying so when the application is not
    // running — or wait to be started by the application. Reconciled on every
    // run, so a moved executable or a setting carried over from another build
    // still starts; a refusal is logged and the widget runs anyway.
    if let Err(error) = dsmon_core::autostart::set_enabled(
        dsmon_core::autostart::Program::Widget,
        config.widget_auto_start,
    ) {
        let _ = dsmon_core::storage::log_line(&format!(
            "The widget's start-up entry could not be written: {error}"
        ));
    }

    let mut viewport = viewport(&config);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }

    // The widget is single-instance in its own right, under a name of its own.
    // Taking the application's would make a widget started while only the
    // application runs exit at once — and the application would refuse to come
    // up whenever a widget was on screen.
    let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let ctx_holder = std::sync::Arc::new(std::sync::OnceLock::new());
    let instance = match crate::instance::claim(
        crate::instance::Names::WIDGET,
        false,
        std::sync::Arc::clone(&requests),
        std::sync::Arc::clone(&ctx_holder),
    ) {
        crate::instance::Role::First(guard) => guard,
        // The copy that is already running has the window; this one is done.
        crate::instance::Role::Latecomer => return Ok(()),
    };

    eframe::run_native(
        "token 用量",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |cc| {
            let _ = ctx_holder.set(cc.egui_ctx.clone());
            crate::fonts::install(&cc.egui_ctx);
            apply_theme(&cc.egui_ctx, &config);
            // eframe only hands the icon to Windows and macOS itself; on Linux
            // the window comes up without one unless it is passed along.
            if let Some(icon) = window_icon() {
                cc.egui_ctx
                    .send_viewport_cmd(egui::ViewportCommand::Icon(Some(std::sync::Arc::new(
                        icon,
                    ))));
            }
            Ok(Box::new(Widget::new(
                &cc.egui_ctx,
                config,
                requests,
                instance,
            )))
        }),
    )
}

/// The widget's window icon: the application's artwork with the widget's badge
/// on it, so a task bar or a window list that holds both does not show the same
/// mark twice.
///
/// 128 px on purpose, the same ceiling the application's icon lives under: an
/// X11 property change carries at most 65535 words, and a 256×256 icon needs
/// two more than that, so a larger frame is refused without a word.
fn window_icon() -> Option<egui::IconData> {
    let icon = dsmon_core::icon::widget_icon(128)?;
    Some(egui::IconData {
        rgba: icon.rgba,
        width: icon.width,
        height: icon.height,
    })
}

/// Starts the widget beside the program that is running.
///
/// The application calls this when its setting says the widget should be up.
/// Calling it while a widget is already there is harmless: the second copy
/// hands its request over — raising the window that exists — and leaves.
pub fn start_process() -> Result<(), String> {
    client::spawn_sibling(if cfg!(windows) {
        "dsmon2-widget.exe"
    } else {
        "dsmon2-widget"
    })
}

fn viewport(config: &AppConfig) -> egui::ViewportBuilder {
    let size = config
        .widget_window_size
        .unwrap_or_else(|| preset(&config.widget_size));
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("token 用量")
        // A name of its own, which is what the package's `.desktop` file
        // matches the window against. Sharing the application's would leave
        // the desktop guessing which of the two a window belongs to.
        .with_app_id("deepseek-balance-monitor-widget")
        // No decorations and no chrome: the panel is the whole window, and the
        // title bar is drawn by the widget itself.
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(true)
        .with_inner_size(size)
        .with_min_inner_size(MIN_SIZE);
    if config.widget_always_on_top {
        viewport = viewport.with_always_on_top();
    }
    if let Some([x, y]) = config.widget_pos {
        viewport = viewport.with_position([x, y]);
    }
    #[cfg(windows)]
    {
        // A widget is not a task to switch to; the tray owns that.
        viewport = viewport.with_taskbar(false);
    }
    #[cfg(target_os = "linux")]
    {
        // The same idea, the X11 way: `Utility` is what the spec calls "a small
        // persistent utility window, such as a palette or toolbox", and a
        // window manager keeps those out of the task bar and the window list.
        // The window icon still carries the widget's badge, so a desktop that
        // does show it (or shows it in a panel of its own) cannot be confused
        // with the application's window.
        viewport = viewport.with_window_type(egui::X11WindowType::Utility);
    }
    viewport
}

fn preset(name: &str) -> [f32; 2] {
    match name {
        "compact" => COMPACT,
        _ => STANDARD,
    }
}

/// The theme the application is using, applied to this window too.
fn apply_theme(ctx: &egui::Context, config: &AppConfig) {
    theme::install(
        ctx,
        theme::Style::from_config(&config.theme),
        &config.icon_colors,
    );
    theme::set_mode(ctx, theme::mode_from_config(&config.ui_theme));
}

/// What the panel knows about the reading behind it, as opposed to what the
/// reading says: whether it arrived, whether more is on the way, and whether
/// the application speaks a format this widget understands.
#[derive(Debug, Clone, Copy)]
struct Live {
    /// The application answered the last request.
    connected: bool,
    /// A poll is in flight.
    checking: bool,
    /// The payload version is not the one this widget was built for.
    mismatched: bool,
}

struct Widget {
    config: AppConfig,
    link: client::Link,
    /// The window state that was last written to the configuration.
    stored: Stored,
    /// The window state that is waiting to stop changing.
    settling: Option<(Instant, [f32; 2], [f32; 2])>,
    /// Which subscription the activity card is narrowed to; `None` is all of
    /// them, which is what it shows unless asked otherwise.
    activity: Option<String>,
    /// Whether the panel is on screen. The tray puts it away and brings it
    /// back, so the widget has to remember which it is.
    visible: bool,
    /// The widget's own tray entry, and what its menu asks for.
    tray: tray::Tray,
    tray_commands: tray::Queue,
    /// What a second launch asks of this copy, which has the window.
    requests: std::sync::Arc<std::sync::Mutex<Vec<crate::instance::Request>>>,
    /// The claim on being the running copy, kept for the process's lifetime.
    _instance: crate::instance::Guard,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Stored {
    position: Option<[f32; 2]>,
    size: Option<[f32; 2]>,
}

impl Widget {
    fn new(
        ctx: &egui::Context,
        config: AppConfig,
        requests: std::sync::Arc<std::sync::Mutex<Vec<crate::instance::Request>>>,
        instance: crate::instance::Guard,
    ) -> Self {
        let stored = Stored {
            position: config.widget_pos,
            size: config.widget_window_size,
        };
        let tray_commands: tray::Queue = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let tray = tray::Tray::spawn(
            &config.ui_language,
            std::sync::Arc::clone(&tray_commands),
            ctx,
        );

        Self {
            link: client::Link::start(DAYS[1]),
            config,
            stored,
            settling: None,
            activity: None,
            visible: true,
            tray,
            tray_commands,
            requests,
            _instance: instance,
        }
    }

    /// The panel's opacity, in the four steps the button walks through.
    fn opacity(&self) -> f32 {
        widget_opacity_level(self.config.widget_opacity)
    }

    fn look<'a>(&self, palette: &'a Palette, lang: &'a str) -> cards::Look<'a> {
        cards::Look {
            palette,
            lang,
            opacity: self.opacity(),
            show_trend: self.config.widget_show_trend,
        }
    }

    /// The panel's surface, letting the desktop show through by the chosen
    /// step. Only the panel fades: the cards and the text keep their contrast,
    /// or 25% would leave nothing readable.
    fn panel_colour(&self, palette: &Palette) -> Color32 {
        let base = palette.bg_panel;
        let alpha = (self.opacity() * 255.0).round() as u8;
        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha)
    }

    /// Applies whatever changed in the configuration.
    fn follow_config(&mut self, ctx: &egui::Context) {
        let fresh = AppConfig::load();

        // The tray entry is the other end of this setting, and unticking it
        // there means the window goes away — which for a window with no close
        // button of its own means this process ends. Only a change counts: a
        // widget started by hand was already running with the setting off.
        if self.config.widget_enabled && !fresh.widget_enabled {
            // Kept in step with the file as well: a position written on this
            // last frame would otherwise carry the old setting back into it.
            self.config = fresh;
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        let theme_changed = fresh.ui_theme != self.config.ui_theme
            || fresh.theme != self.config.theme
            || fresh.icon_colors != self.config.icon_colors;
        if theme_changed {
            apply_theme(ctx, &fresh);
        }
        if fresh.ui_language != self.config.ui_language {
            self.tray.set_language(&fresh.ui_language);
        }
        if fresh.widget_always_on_top != self.config.widget_always_on_top {
            let level = if fresh.widget_always_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            };
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
        }
        self.config = fresh;
    }

    /// Stores where the window ended up, once it has stopped moving.
    fn remember_window(&mut self, ctx: &egui::Context) {
        // The rects in `ViewportInfo` are already in points, which is what the
        // configuration stores: dividing by the scale again here is what once
        // shrank the window a little on every start.
        let observed = ctx.input(|input| {
            let viewport = input.viewport();
            let position = viewport
                .outer_rect
                .or(viewport.inner_rect)
                .map(|rect| [rect.min.x, rect.min.y]);
            let size = viewport
                .inner_rect
                .map(|rect| [rect.width().round(), rect.height().round()]);
            (position, size)
        });

        let (Some(position), Some(size)) = observed else {
            return;
        };
        if self.stored.position == Some(position) && self.stored.size == Some(size) {
            self.settling = None;
            return;
        }

        match self.settling {
            Some((since, last_position, last_size))
                if last_position == position && last_size == size =>
            {
                if since.elapsed() < SETTLE {
                    // Writing on every frame of a drag would put the file to
                    // work for nothing.
                    ctx.request_repaint_after(SETTLE);
                    return;
                }
                self.settling = None;
                self.stored = Stored {
                    position: Some(position),
                    size: Some(size),
                };
                self.config.widget_pos = Some(position);
                self.config.widget_window_size = Some(size);
                let _ = self.config.save();
            }
            _ => self.settling = Some((Instant::now(), position, size)),
        }
    }

    /// Does whatever was picked in the tray menu.
    ///
    /// This runs in `logic` rather than in `ui`, which is what lets the menu
    /// bring the panel back: a window that is not on screen is not drawn, and
    /// anything handled while drawing would never run for it.
    fn drain_tray(&mut self, ctx: &egui::Context) {
        let picked = match self.tray_commands.lock() {
            Ok(mut queue) => std::mem::take(&mut *queue),
            Err(_) => Vec::new(),
        };

        for command in picked {
            match command {
                tray::Command::ToggleWindow => {
                    self.visible = !self.visible;
                    ctx.send_viewport_cmd(ViewportCommand::Visible(self.visible));
                    self.tray.set_visible(self.visible);
                }
                tray::Command::OpenApplication => self.open_application(),
                tray::Command::Quit => self.close(ctx),
            }
        }
    }

    /// Answers a second launch, which cannot do anything but ask.
    ///
    /// The window is already open — that is what being the running copy means
    /// — so the request is only to bring it forward, which is what a user who
    /// started the widget twice was after.
    fn drain_requests(&mut self, ctx: &egui::Context) {
        let asked = match self.requests.lock() {
            Ok(mut queue) => std::mem::take(&mut *queue),
            Err(_) => Vec::new(),
        };
        if asked.contains(&crate::instance::Request::Show) {
            // A second launch wants the panel in front of it, which includes
            // bringing it back if the tray put it away.
            self.visible = true;
            self.tray.set_visible(true);
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
    }

    /// Keeps the width where the preset put it.
    ///
    /// Only the height is the user's to change: the panel is one column of
    /// cards, so a wider window would only stretch them, while a taller one
    /// saves scrolling when several subscriptions are configured. A drag of the
    /// right edge is answered by putting the width back and leaving the height
    /// alone.
    fn lock_width(&self, ctx: &egui::Context) {
        let Some(inner) = ctx.input(|input| input.viewport().inner_rect) else {
            return;
        };
        let width = preset(&self.config.widget_size)[0];
        if (inner.width() - width).abs() > 0.5 {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
                width,
                inner.height(),
            )));
        }
    }

    /// Turns the next range in the cycle.
    fn next_range(&self) {
        let current = self.link.range();
        let index = DAYS.iter().position(|days| *days == current).unwrap_or(1);
        self.link.set_range(DAYS[(index + 1) % DAYS.len()]);
    }

    /// Closes the widget for good: the application is told not to start it
    /// again, and this process ends.
    fn close(&mut self, ctx: &egui::Context) {
        let mut config = AppConfig::load();
        config.widget_enabled = false;
        let _ = config.save();

        // And the session is told not to start it either: closing this is
        // "not wanted", and an entry left behind would bring it back at the
        // next login.
        if let Err(error) =
            dsmon_core::autostart::set_enabled(dsmon_core::autostart::Program::Widget, false)
        {
            let _ = dsmon_core::storage::log_line(&format!(
                "The widget's start-up entry could not be removed: {error}"
            ));
        }
        // Kept in step with the file: a position written before the window
        // really goes would otherwise carry the old setting back into it.
        self.config = config;
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    /// Raises the application, starting it when it is not running. One call
    /// covers both: it is single-instance, so a second copy hands the request
    /// over and leaves.
    fn open_application(&self) {
        let _ = client::start_application();
    }

    /// One press, one effect.
    fn pressed(&mut self, kind: &str, ctx: &egui::Context) {
        match kind {
            "close" => self.close(ctx),
            "settings" => self.open_application(),
            "refresh" => self.link.check_now(),
            "pin" => {
                let mut config = AppConfig::load();
                config.widget_always_on_top = !config.widget_always_on_top;
                let _ = config.save();
                let level = if config.widget_always_on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                };
                ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
            }
            "opacity" => {
                let step = WIDGET_OPACITY_LEVELS
                    .iter()
                    .position(|level| *level == self.opacity())
                    .unwrap_or(0);
                let mut config = AppConfig::load();
                config.widget_opacity =
                    WIDGET_OPACITY_LEVELS[(step + 1) % WIDGET_OPACITY_LEVELS.len()];
                let _ = config.save();
            }
            "theme" => {
                let mode = theme::toggled(ctx.theme());
                theme::set_mode(ctx, mode);
                let mut config = AppConfig::load();
                config.ui_theme = theme::mode_to_config(mode).to_owned();
                let _ = config.save();
            }
            _ => {}
        }
    }
}

impl eframe::App for Widget {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // The panel paints its own surface; the window behind it stays empty,
        // so what shows through the translucent panel is the desktop.
        [0.0, 0.0, 0.0, 0.0]
    }

    /// Everything that has to happen whether or not a frame is drawn.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Frames are drawn on demand, and the readings arrive from another
        // thread: without a heartbeat a new one would sit unseen.
        ctx.request_repaint_after(Duration::from_millis(500));
        // Whether the tray icon is really there is a question only Windows
        // asks, and it asks it late (see `Tray::report`).
        self.tray.report();
        self.drain_tray(ctx);
        self.drain_requests(ctx);
        self.follow_config(ctx);
        self.lock_width(ctx);
        self.remember_window(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // The edges move the window and the bottom corners resize it; the title
        // bar does neither, because a drag area lying under the buttons
        // swallows their clicks.
        self.window_handles(ui);

        let palette = theme::current(
            ui.ctx(),
            theme::Style::from_config(&self.config.theme),
            &self.config.icon_colors,
        );
        let lang = self.config.ui_language.clone();
        let look = self.look(&palette, &lang);
        let state = self.link.state();
        let payload = state.payload.clone();
        let range = self.link.range();
        let panel = self.panel_colour(&palette);
        let live = Live {
            connected: state.connected,
            checking: payload.as_ref().is_some_and(|payload| payload.checking),
            mismatched: state.mismatched,
        };
        let mut action = None;

        egui::Frame::NONE
            .fill(panel)
            .corner_radius(CornerRadius::same(16))
            .stroke(Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(
                    0xff,
                    0xff,
                    0xff,
                    if palette.dark { 33 } else { 20 },
                ),
            ))
            .inner_margin(egui::Margin::same(PADDING as i8))
            .show(ui, |ui| {
                // The panel *is* the window: it keeps the window's size rather
                // than growing and shrinking with the cards inside it.
                ui.set_min_size(ui.available_size());
                ui.vertical(|ui| {
                    if let Some(pressed) = self.header(ui, &look, live) {
                        self.pressed(pressed, ui.ctx());
                    }
                    // The title bar and the footer stay put; only the cards
                    // scroll, so the panel keeps its shape at any size.
                    let body_height = (ui.available_height() - FOOTER).max(0.0);
                    egui::ScrollArea::vertical()
                        .max_height(body_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            action = self.body(ui, &look, payload.as_ref(), range, live);
                        });
                    self.footer(ui, &look, payload.as_ref(), live.connected);
                });
            });

        match action {
            Some(cards::Action::Refresh) => self.link.check_now(),
            Some(cards::Action::StartApplication) | Some(cards::Action::OpenSettings) => {
                self.open_application()
            }
            Some(cards::Action::NextRange) => self.next_range(),
            // The tabs say which grid they want; the widget only remembers it.
            Some(cards::Action::ShowActivity(key)) => self.activity = key,
            None => {}
        }
    }
}

impl Widget {
    /// The title bar: the only way in and out of a window with no decorations.
    ///
    /// Returns the button that was pressed, if any.
    fn header(
        &self,
        ui: &mut egui::Ui,
        look: &cards::Look<'_>,
        live: Live,
    ) -> Option<&'static str> {
        // No `Sense::drag()` here on purpose. A drag region under the buttons
        // takes the press for itself — egui hands an overlapping press to the
        // drag — so the buttons never see a click. The window is moved from the
        // edges instead (see [`Widget::drag_edges`]), and this row senses
        // nothing at all.
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), HEADER), Sense::hover());

        let centre = rect.center().y;
        for (index, (ax, ay)) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
            .into_iter()
            .enumerate()
        {
            let corner = egui::pos2(rect.left() + 3.0 + ax * 7.0, centre - 6.0 + ay * 7.0);
            ui.painter().rect_filled(
                egui::Rect::from_center_size(corner, egui::vec2(5.0, 5.0)),
                CornerRadius::same(2),
                if index == 0 || index == 3 {
                    look.palette.accent
                } else {
                    look.palette.positive
                },
            );
        }
        ui.painter().text(
            egui::pos2(rect.left() + 23.0, centre),
            egui::Align2::LEFT_CENTER,
            tr(look.lang, "widget_title"),
            crate::fonts::ui_font(13.0),
            look.palette.text_primary,
        );

        let mut pressed = None;
        let mut cursor = rect.right();
        for kind in ["close", "settings", "pin", "refresh", "opacity", "theme"] {
            let button = egui::Rect::from_center_size(
                egui::pos2(cursor - 13.0, centre),
                egui::vec2(22.0, 24.0),
            );
            cursor -= 26.0;

            // Refresh is the one button that needs something from the
            // application, so it is dead while the application is away and
            // while a poll it asked for is still running — pressing it then
            // would only queue another.
            let enabled = kind != "refresh" || (live.connected && !live.checking);
            let response = ui.interact(
                button,
                ui.id().with(kind),
                if enabled {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );
            if response.hovered() {
                ui.painter()
                    .rect_filled(button, CornerRadius::same(6), wash(look.palette));
            }
            // The pin carries a state, and a glyph that looks the same either
            // way is a button nobody can tell is working.
            paint_glyph(
                ui,
                kind,
                button.center(),
                look,
                self.opacity(),
                kind == "pin" && self.config.widget_always_on_top,
                enabled,
            );
            // `on_hover_text` takes the response, so the click is read first.
            let clicked = response.clicked();
            if let Some(tip) = self.tooltip(kind) {
                response.on_hover_text(tip);
            }
            if clicked {
                pressed = Some(kind);
            }
        }
        pressed
    }

    /// What can be done to the window by hand: the three edges move it, and the
    /// two bottom corners change its height.
    ///
    /// The title bar used to carry the moving and no longer can — see
    /// [`Widget::header`]. A band along the bottom edge would do for the height
    /// too, except that the moving band is already there, so the corners take
    /// it instead: they are where a panel is resized from anyway, and they are
    /// big enough to hit without aiming.
    ///
    /// Nothing inside the panel reaches these places, and they are registered
    /// before it, so no press has to be argued over.
    fn window_handles(&self, ui: &mut egui::Ui) {
        // The whole window, whatever the layout inside it is doing.
        let window = ui.ctx().input(|i| i.viewport_rect());

        let bands = [
            egui::Rect::from_min_max(
                window.left_top(),
                egui::pos2(window.left() + EDGE, window.bottom()),
            ),
            egui::Rect::from_min_max(
                egui::pos2(window.right() - EDGE, window.top()),
                window.right_bottom(),
            ),
            egui::Rect::from_min_max(
                egui::pos2(window.left(), window.bottom() - EDGE),
                window.right_bottom(),
            ),
        ];
        for (index, band) in bands.into_iter().enumerate() {
            let response = ui.interact(band, ui.id().with(("drag_edge", index)), Sense::drag());
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            if response.drag_started() {
                // The window manager does the moving; the widget only says when.
                ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
            }
        }

        // Registered last, so the corners win wherever they overlap the bands.
        //
        // Either corner asks for `South` rather than its own diagonal: the
        // width is fixed (see [`Widget::lock_width`]), and a window manager
        // moving both edges at once is a fight the width lock wins — dragging
        // the height would see it snap back, because the command that restores
        // the width carries the previous frame's height with it. One edge, one
        // dimension, no argument.
        let corners = [
            (
                "resize_south_west",
                egui::Rect::from_min_max(
                    egui::pos2(window.left(), window.bottom() - CORNER),
                    egui::pos2(window.left() + CORNER, window.bottom()),
                ),
            ),
            (
                "resize_south_east",
                egui::Rect::from_min_max(
                    egui::pos2(window.right() - CORNER, window.bottom() - CORNER),
                    window.right_bottom(),
                ),
            ),
        ];
        for (id, corner) in corners {
            let response = ui.interact(corner, ui.id().with(id), Sense::drag());
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeSouth);
            }
            if response.drag_started() {
                ui.ctx()
                    .send_viewport_cmd(ViewportCommand::BeginResize(egui::ResizeDirection::South));
            }
        }
    }

    fn tooltip(&self, kind: &str) -> Option<&'static str> {
        let key = match kind {
            "close" => "widget_tip_close",
            "settings" => "widget_tip_settings",
            "pin" => "widget_tip_on_top",
            "refresh" => "check_now",
            "opacity" => "widget_tip_opacity",
            "theme" => "widget_tip_theme",
            _ => return None,
        };
        Some(tr(&self.config.ui_language, key))
    }

    /// The cards, and whatever they asked for.
    fn body(
        &mut self,
        ui: &mut egui::Ui,
        look: &cards::Look<'_>,
        payload: Option<&Payload>,
        range: u64,
        live: Live,
    ) -> Option<cards::Action> {
        let mut action = None;

        // Old readings stay on screen, greyed, with the strip that says so.
        if !live.connected {
            cards::offline_banner(ui, look);
        } else if live.mismatched {
            // The connection is fine; it is the format that is not. Saying so
            // is the contract's own rule — better than showing the half of the
            // payload that happened to parse.
            cards::version_banner(ui, look);
        }

        match payload {
            None => action = cards::not_connected(ui, look),
            Some(payload) => {
                if payload.platforms.is_empty() {
                    action = cards::nothing_configured(ui, look);
                } else {
                    // One card per configured balance provider. The curve range
                    // is shared, so only the first card carries its button.
                    let mut first = true;
                    for platform in payload.platforms.iter().filter(|p| p.kind == "payg") {
                        if let Some(chosen) = cards::balance_card(ui, look, platform, range, first)
                        {
                            action = Some(chosen);
                        }
                        first = false;
                    }
                }
                for platform in payload.platforms.iter().filter(|p| p.kind == "package") {
                    cards::subscription(ui, look, platform);
                }
                // The activity card covers every subscription, so it comes
                // after all of them — and only when there is one.
                if payload.platforms.iter().any(|p| p.kind == "package") {
                    if let Some(chosen) =
                        cards::activity_card(ui, look, &payload.platforms, self.activity.as_deref())
                    {
                        action = Some(chosen);
                    }
                }
                if !live.connected {
                    // The card that offers to start the application sits under
                    // the readings rather than replacing them.
                    if let Some(chosen) = cards::not_connected(ui, look) {
                        action = Some(chosen);
                    }
                }
            }
        }

        action
    }

    fn footer(
        &self,
        ui: &mut egui::Ui,
        look: &cards::Look<'_>,
        payload: Option<&Payload>,
        connected: bool,
    ) {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), FOOTER), Sense::hover());
        let centre = rect.center().y;
        let (dot, left) = if connected {
            (look.palette.positive, tr(look.lang, "widget_connected"))
        } else {
            (look.palette.warning, tr(look.lang, "widget_offline"))
        };
        ui.painter()
            .circle_filled(egui::pos2(rect.left() + 4.0, centre), 3.5, dot);
        ui.painter().text(
            egui::pos2(rect.left() + 14.0, centre),
            egui::Align2::LEFT_CENTER,
            left,
            crate::fonts::ui_font(10.0),
            look.palette.text_secondary,
        );

        let last = payload
            .and_then(|payload| payload.last_check_at.as_deref())
            .unwrap_or_default();
        let right = if connected {
            if last.is_empty() {
                String::new()
            } else {
                format!("{} {last}", tr(look.lang, "last_check"))
            }
        } else {
            tr(look.lang, "widget_retry_every").to_owned()
        };
        ui.painter().text(
            egui::pos2(rect.right(), centre),
            egui::Align2::RIGHT_CENTER,
            right,
            crate::fonts::ui_font(10.0),
            look.palette.text_secondary,
        );

        // The middle column says where the numbers come from while they are
        // live, and how old they are when they are not. A narrow panel drops it
        // rather than letting the three columns overlap.
        if rect.width() >= 340.0 {
            let middle = if connected {
                tr(look.lang, "widget_source").to_owned()
            } else if last.is_empty() {
                String::new()
            } else {
                format!("{} {last}", tr(look.lang, "widget_last_update"))
            };
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                middle,
                crate::fonts::ui_font(10.0),
                look.palette.text_secondary,
            );
        }
    }
}

/// The wash a hovered title-bar button sits on.
fn wash(palette: &Palette) -> Color32 {
    if palette.dark {
        Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 22)
    } else {
        Color32::from_rgba_unmultiplied(0x00, 0x00, 0x00, 14)
    }
}

/// Paints one of the six title-bar glyphs.
///
/// Drawn rather than loaded: the tray icon has always been painted in code, and
/// a handful of circles and lines needs no image files beside the executable.
fn paint_glyph(
    ui: &egui::Ui,
    kind: &str,
    centre: egui::Pos2,
    look: &cards::Look<'_>,
    opacity: f32,
    on: bool,
    enabled: bool,
) {
    // A glyph that stands for a switch (the pin) is drawn in the accent colour
    // while it is on, so its state is readable without a legend. A disabled one
    // is drawn faint, which is what "there is nothing to ask for" looks like.
    let colour = if !enabled {
        look.palette.text_secondary.gamma_multiply(0.45)
    } else if on {
        look.palette.accent
    } else {
        look.palette.text_primary.gamma_multiply(0.78)
    };
    let painter = ui.painter();
    let stroke = Stroke::new(if on { 1.9 } else { 1.5 }, colour);

    match kind {
        "close" => {
            let arm = 5.0;
            painter.line_segment(
                [
                    centre + egui::vec2(-arm, -arm),
                    centre + egui::vec2(arm, arm),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    centre + egui::vec2(-arm, arm),
                    centre + egui::vec2(arm, -arm),
                ],
                stroke,
            );
        }
        "settings" => {
            painter.circle_stroke(centre, 5.2, Stroke::new(1.7, colour));
            painter.circle_stroke(centre, 1.9, Stroke::new(1.4, colour));
            for step in 0..6 {
                let angle = step as f32 * std::f32::consts::TAU / 6.0;
                let (sin, cos) = angle.sin_cos();
                painter.line_segment(
                    [
                        centre + egui::vec2(cos * 5.0, sin * 5.0),
                        centre + egui::vec2(cos * 7.8, sin * 7.8),
                    ],
                    Stroke::new(1.9, colour),
                );
            }
        }
        "pin" => {
            painter.circle_stroke(centre + egui::vec2(0.0, -4.0), 3.8, stroke);
            painter.line_segment(
                [
                    centre + egui::vec2(-5.0, -0.5),
                    centre + egui::vec2(5.0, -0.5),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    centre + egui::vec2(0.0, -0.5),
                    centre + egui::vec2(0.0, 7.0),
                ],
                stroke,
            );
        }
        "refresh" => {
            let points: Vec<egui::Pos2> = (0..24)
                .map(|step| {
                    let angle = 0.5 + step as f32 * (std::f32::consts::TAU * 0.8) / 23.0;
                    let (sin, cos) = angle.sin_cos();
                    centre + egui::vec2(cos * 5.5, sin * 5.5)
                })
                .collect();
            painter.add(egui::epaint::PathShape::line(points, stroke));
            painter.add(egui::Shape::convex_polygon(
                vec![
                    centre + egui::vec2(3.0, -7.6),
                    centre + egui::vec2(7.0, -3.4),
                    centre + egui::vec2(1.0, -2.6),
                ],
                colour,
                Stroke::NONE,
            ));
        }
        "opacity" => {
            // Filled from the left in four steps, so the button says which step
            // it is on without a number beside it.
            let radius = 6.0;
            let step = WIDGET_OPACITY_LEVELS
                .iter()
                .position(|level| *level == opacity)
                .unwrap_or(0) as f32;
            let share = (step + 1.0) / WIDGET_OPACITY_LEVELS.len() as f32;
            let edge = radius * (2.0 * share - 1.0);

            if edge >= radius {
                painter.circle_filled(centre, radius, colour);
            } else {
                // The filled part is the circular segment left of the chord at
                // `edge`: its arc runs from `angle` round to `TAU - angle`.
                let angle = (edge / radius).clamp(-1.0, 1.0).acos();
                let points: Vec<egui::Pos2> = (0..=32)
                    .map(|step| {
                        let turn =
                            angle + (std::f32::consts::TAU - 2.0 * angle) * step as f32 / 32.0;
                        let (sin, cos) = turn.sin_cos();
                        centre + egui::vec2(cos * radius, sin * radius)
                    })
                    .collect();
                painter.add(egui::Shape::convex_polygon(points, colour, Stroke::NONE));
            }
            painter.circle_stroke(centre, radius, Stroke::new(1.5, colour));
        }
        "theme" => {
            if look.palette.dark {
                // A sun, for switching to the light scheme.
                painter.circle_filled(centre, 3.4, colour);
                for step in 0..8 {
                    let angle = step as f32 * std::f32::consts::TAU / 8.0;
                    let (sin, cos) = angle.sin_cos();
                    painter.line_segment(
                        [
                            centre + egui::vec2(cos * 5.4, sin * 5.4),
                            centre + egui::vec2(cos * 8.2, sin * 8.2),
                        ],
                        Stroke::new(1.3, colour),
                    );
                }
            } else {
                // A crescent, for switching to the dark one. The bite is taken
                // out with the panel's own colour, which is what sits behind it.
                painter.circle_filled(centre, 6.0, colour);
                painter.circle_filled(centre + egui::vec2(3.6, -2.6), 5.2, look.palette.bg_panel);
            }
        }
        _ => {}
    }
}
