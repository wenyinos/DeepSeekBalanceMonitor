//! Windows tray: the Win32 notification area via `tray-icon`.
//!
//! The menu belongs to muda, and both menu picks and clicks arrive through its
//! global channels. Handlers are installed here rather than polled, so a click
//! reaches the interface even while it is idle.

use std::sync::{Arc, Mutex};

use dsmon_core::icon::{self, IconSpec, IconTheme};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

use super::{Command, Status};

/// Menu ids, as they come back through `MenuEvent`.
const ID_OPEN: &str = "open-window";
const ID_REFRESH: &str = "refresh";
const ID_SETTINGS: &str = "settings";
const ID_QUIT: &str = "quit";

/// Keeps the tray icon registered for as long as it is held.
pub struct TrayHandle {
    icon: TrayIcon,
    /// Entries whose text follows the interface language, so they have to be
    /// reachable after the menu is built.
    items: Items,
}

struct Items {
    open: MenuItem,
    refresh: MenuItem,
    settings: MenuItem,
    quit: MenuItem,
}

impl Items {
    fn entries(&self) -> [(&MenuItem, &str); 4] {
        [
            (&self.open, "open_window"),
            (&self.refresh, "check_now"),
            (&self.settings, "settings"),
            (&self.quit, "quit"),
        ]
    }
}

impl TrayHandle {
    /// Redraws the icon and its hover text.
    pub fn draw(&self, status: &Status, theme: &IconTheme) {
        let fill = theme.state_color(status.state);
        let rendered = icon::render(&IconSpec {
            label: &status.label,
            background: fill,
            foreground: icon::readable_on(fill),
            size: 64,
        });

        match tray_icon::Icon::from_rgba(rendered.rgba, rendered.width, rendered.height) {
            Ok(image) => {
                let _ = self.icon.set_icon(Some(image));
            }
            Err(error) => {
                let _ = dsmon_core::storage::log_line(&format!("Icon update failed: {error}"));
            }
        }

        let tooltip = format!("{}\n{}", dsmon_core::APP_NAME, status.tooltip);
        let _ = self.icon.set_tooltip(Some(tooltip));
    }

    pub fn set_language(&self, lang: &str) {
        for (item, key) in self.items.entries() {
            item.set_text(crate::i18n::tr(lang, key));
        }
    }
}

/// Registers the tray icon together with its menu.
pub fn spawn(
    lang: &str,
    theme: &IconTheme,
    commands: Arc<Mutex<Vec<Command>>>,
    ctx: egui::Context,
) -> TrayHandle {
    let text = |key: &str| crate::i18n::tr(lang, key).to_owned();

    let menu = Menu::new();
    let open = MenuItem::with_id(ID_OPEN, text("open_window"), true, None);
    let refresh = MenuItem::with_id(ID_REFRESH, text("check_now"), true, None);
    let settings = MenuItem::with_id(ID_SETTINGS, text("settings"), true, None);
    let quit = MenuItem::with_id(ID_QUIT, text("quit"), true, None);

    let separator = tray_icon::menu::PredefinedMenuItem::separator();
    for entry in [
        &open as &dyn tray_icon::menu::IsMenuItem,
        &refresh,
        &separator,
        &settings,
        &quit,
    ] {
        let _ = menu.append(entry);
    }

    let rendered = icon::render(&IconSpec {
        label: "...",
        background: theme.state_color(icon::State::NoData),
        foreground: icon::readable_on(theme.state_color(icon::State::NoData)),
        size: 64,
    });
    let image = tray_icon::Icon::from_rgba(rendered.rgba, rendered.width, rendered.height)
        .expect("generated icon is a valid RGBA bitmap");

    // A left click opens the window, so the menu waits for the right button.
    let icon = TrayIconBuilder::new()
        .with_tooltip(dsmon_core::APP_NAME)
        .with_icon(image)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .expect("tray icon registers with the shell");

    install_handlers(Arc::clone(&commands), ctx);

    TrayHandle {
        icon,
        items: Items {
            open,
            refresh,
            settings,
            quit,
        },
    }
}

/// Routes menu picks and clicks into the command queue and wakes the interface.
fn install_handlers(commands: Arc<Mutex<Vec<Command>>>, ctx: egui::Context) {
    let menu_commands = Arc::clone(&commands);
    let menu_ctx = ctx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let command = match event.id.0.as_str() {
            ID_OPEN => Some(Command::OpenWindow),
            ID_REFRESH => Some(Command::Refresh),
            ID_SETTINGS => Some(Command::OpenSettings),
            ID_QUIT => Some(Command::Quit),
            _ => None,
        };
        if let Some(command) = command {
            push(&menu_commands, command);
            menu_ctx.request_repaint();
        }
    }));

    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            push(&commands, Command::OpenWindow);
            ctx.request_repaint();
        }
    }));
}

fn push(commands: &Arc<Mutex<Vec<Command>>>, command: Command) {
    if let Ok(mut queue) = commands.lock() {
        queue.push(command);
    }
}
