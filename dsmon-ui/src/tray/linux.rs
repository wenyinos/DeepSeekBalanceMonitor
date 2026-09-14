//! Linux tray: StatusNotifierItem over D-Bus.
//!
//! No GTK and no libappindicator, so the binary stays self-contained.

use std::sync::Arc;

use dsmon_core::icon::{self, IconSpec, IconTheme, State};
use ksni::{
    blocking::{Handle, TrayMethods},
    menu::StandardItem,
    Icon, MenuItem, Tray,
};

/// Keeps the tray registered for as long as it is held.
pub struct TrayHandle {
    _handle: Handle<MonitorTray>,
}

struct MonitorTray {
    title: String,
    label: String,
    state: State,
    theme: IconTheme,
    on_quit: Arc<dyn Fn() + Send + Sync>,
}

impl Tray for MonitorTray {
    fn id(&self) -> String {
        "com.github.wenyinos.deepseek-balance-monitor".to_owned()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        let rendered = render_icon(&self.label, self.state, &self.theme);
        vec![Icon {
            width: rendered.width as i32,
            height: rendered.height as i32,
            data: argb32(&rendered.rgba),
        }]
    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let on_quit = Arc::clone(&self.on_quit);
        vec![StandardItem {
            label: "退出".to_owned(),
            activate: Box::new(move |_| on_quit()),
            ..Default::default()
        }
        .into()]
    }
}

/// Registers the tray icon. `on_quit` runs on the D-Bus thread when the user
/// picks the quit entry.
pub fn spawn(
    label: &str,
    state: State,
    theme: IconTheme,
    on_quit: impl Fn() + Send + Sync + 'static,
) -> TrayHandle {
    let tray = MonitorTray {
        title: dsmon_core::APP_NAME.to_owned(),
        label: label.to_owned(),
        state,
        theme,
        on_quit: Arc::new(on_quit),
    };
    let handle = tray
        .spawn()
        .expect("the session bus accepts a StatusNotifierItem");
    TrayHandle { _handle: handle }
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

/// StatusNotifierItem expects ARGB32 in network byte order.
fn argb32(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for pixel in rgba.chunks_exact(4) {
        out.extend_from_slice(&[pixel[3], pixel[0], pixel[1], pixel[2]]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argb32_reorders_channels() {
        let rgba = [1u8, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(argb32(&rgba), vec![4, 1, 2, 3, 8, 5, 6, 7]);
    }

    #[test]
    fn the_icon_uses_the_state_colour() {
        let theme = IconTheme {
            style: "default".to_owned(),
            custom: Default::default(),
        };
        let rendered = render_icon("42", State::Ok, &theme);
        assert_eq!(rendered.width, 64);
        // Centre pixel carries the fill colour from the preset table.
        let centre = ((32 * 64 + 32) * 4) as usize;
        assert_eq!(rendered.rgba[centre], 0x3c);
        assert_eq!(rendered.rgba[centre + 1], 0x69);
        assert_eq!(rendered.rgba[centre + 2], 0x66);
        assert_eq!(rendered.rgba[centre + 3], 255);
    }
}
