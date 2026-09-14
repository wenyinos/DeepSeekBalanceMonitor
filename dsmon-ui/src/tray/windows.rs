//! Windows tray: the Win32 notification area via `tray-icon`.

use dsmon_core::icon::{self, IconSpec, IconTheme, State};
use tray_icon::{
    menu::{Menu, MenuItem},
    TrayIcon, TrayIconBuilder,
};

/// Keeps the tray icon registered for as long as it is held.
pub struct TrayHandle {
    _icon: TrayIcon,
}

/// Registers the tray icon together with its menu.
///
/// The menu entries are wired to the UI in stage 3, when the shared command
/// channel exists; for now only the icon and the quit entry are live.
pub fn spawn(
    label: &str,
    state: State,
    theme: IconTheme,
    on_quit: impl Fn() + Send + Sync + 'static,
) -> TrayHandle {
    let rendered = render_icon(label, state, &theme);

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

/// The icon: a rounded square in the state's colour with the figure on top, the
/// same shape the previous build drew.
fn render_icon(label: &str, state: State, theme: &IconTheme) -> icon::TrayIcon {
    let fill = theme.state_color(state);
    let ink = icon::readable_on(fill);
    icon::render(&IconSpec {
        label,
        background: fill,
        foreground: ink,
        size: 64,
    })
}
