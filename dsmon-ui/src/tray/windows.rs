//! Windows tray: the Win32 notification area via `tray-icon`.

use dsmon_core::icon::{self, IconSpec};
use tray_icon::{
    menu::{Menu, MenuItem},
    TrayIcon, TrayIconBuilder,
};

use crate::theme::Palette;

/// Keeps the tray icon registered for as long as it is held.
pub struct TrayHandle {
    _icon: TrayIcon,
}

/// Registers the tray icon together with its menu.
///
/// The menu entries are wired to the UI in stage 3, when the shared command
/// channel exists; stage 0 only needs the icon to appear and stay alive.
pub fn spawn(
    label: &str,
    palette: Palette,
    on_quit: impl Fn() + Send + Sync + 'static,
) -> TrayHandle {
    let rendered = icon::render(&IconSpec {
        label,
        background: [
            palette.bg_panel.r(),
            palette.bg_panel.g(),
            palette.bg_panel.b(),
        ],
        foreground: [palette.accent.r(), palette.accent.g(), palette.accent.b()],
        size: 64,
    });

    let image = tray_icon::Icon::from_rgba(rendered.rgba, rendered.width, rendered.height)
        .expect("generated icon is a valid RGBA bitmap");

    let menu = Menu::new();
    let quit = MenuItem::new("退出", true, None);
    let _ = menu.append(&quit);

    let icon = TrayIconBuilder::new()
        .with_tooltip(dsmon_core::APP_NAME)
        .with_icon(image)
        .with_menu(Box::new(menu))
        .build()
        .expect("tray icon registers with the shell");

    let on_quit = std::sync::Arc::new(on_quit);
    quit.set_activate_handler(Box::new(move |_| on_quit()));

    TrayHandle { _icon: icon }
}
