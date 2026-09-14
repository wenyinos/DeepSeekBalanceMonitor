//! Linux tray: StatusNotifierItem over D-Bus.
//!
//! No GTK and no libappindicator, so the binary stays self-contained.

use std::sync::Arc;

use dsmon_core::icon::{self, IconSpec};
use ksni::{
    blocking::{Handle, TrayMethods},
    menu::StandardItem,
    Icon, MenuItem, Tray,
};

use crate::theme::Palette;

/// Keeps the tray registered for as long as it is held.
pub struct TrayHandle {
    _handle: Handle<MonitorTray>,
}

struct MonitorTray {
    title: String,
    label: String,
    palette: Palette,
    on_quit: Arc<dyn Fn() + Send + Sync>,
}

impl Tray for MonitorTray {
    fn id(&self) -> String {
        "com.github.wenyinos.deepseek-balance-monitor".to_owned()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        let rendered = icon::render(&IconSpec {
            label: &self.label,
            background: rgb(self.palette.bg_panel),
            foreground: rgb(self.palette.accent),
            size: 64,
        });
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
        vec![
            StandardItem {
                label: "退出".to_owned(),
                activate: Box::new(move |_| on_quit()),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Registers the tray icon. `on_quit` runs on the D-Bus thread when the user
/// picks the quit entry.
pub fn spawn(
    label: &str,
    palette: Palette,
    on_quit: impl Fn() + Send + Sync + 'static,
) -> TrayHandle {
    let tray = MonitorTray {
        title: dsmon_core::APP_NAME.to_owned(),
        label: label.to_owned(),
        palette,
        on_quit: Arc::new(on_quit),
    };
    let handle = tray
        .spawn()
        .expect("the session bus accepts a StatusNotifierItem");
    TrayHandle { _handle: handle }
}

fn rgb(color: egui::Color32) -> [u8; 3] {
    [color.r(), color.g(), color.b()]
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
}
