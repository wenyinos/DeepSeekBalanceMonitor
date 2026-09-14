//! Linux tray: StatusNotifierItem over D-Bus.
//!
//! No GTK and no libappindicator, so the binary stays self-contained.
//!
//! Redrawing goes through `Handle::update`: ksni compares the icon it is
//! serving against the new one and emits `NewIcon` when it differs, which is
//! what makes the panel repaint. Nothing else needs to be told.

use std::sync::{Arc, Mutex};

use dsmon_core::icon::{self, IconSpec, IconTheme};
use ksni::{
    blocking::{Handle, TrayMethods},
    menu::{MenuItem, StandardItem},
    Icon, ToolTip, Tray,
};

use super::{Command, Status};

/// Keeps the tray registered for as long as it is held.
pub struct TrayHandle {
    handle: Handle<MonitorTray>,
}

/// The item the panel draws, and everything its menu needs.
pub struct MonitorTray {
    lang: String,
    status: Status,
    theme: IconTheme,
    commands: Arc<Mutex<Vec<Command>>>,
    ctx: egui::Context,
}

impl MonitorTray {
    /// Hands a command to the interface and wakes it, so the reaction does not
    /// wait for the next frame.
    fn send(&self, command: Command) {
        if let Ok(mut queue) = self.commands.lock() {
            queue.push(command);
        }
        self.ctx.request_repaint();
    }

    fn label(&self, key: &str) -> String {
        crate::i18n::tr(&self.lang, key).to_owned()
    }

    /// A menu entry that sends `command` when picked.
    fn entry(&self, key: &str, command: Command) -> MenuItem<Self> {
        StandardItem {
            label: self.label(key),
            activate: Box::new(move |tray: &mut Self| tray.send(command)),
            ..Default::default()
        }
        .into()
    }
}

impl Tray for MonitorTray {
    fn id(&self) -> String {
        "com.github.wenyinos.deepseek-balance-monitor".to_owned()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        let fill = self.theme.state_color(self.status.state);
        let rendered = icon::render(&IconSpec {
            label: &self.status.label,
            background: fill,
            foreground: icon::readable_on(fill),
            size: 64,
        });
        vec![Icon {
            width: rendered.width as i32,
            height: rendered.height as i32,
            data: argb32(&rendered.rgba),
        }]
    }

    fn title(&self) -> String {
        dsmon_core::APP_NAME.to_owned()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: dsmon_core::APP_NAME.to_owned(),
            description: self.status.tooltip.clone(),
            ..Default::default()
        }
    }

    /// Left click: the reading, as a notification.
    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Command::ShowBalance);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            self.entry("open_window", Command::OpenWindow),
            self.entry("check_now", Command::Refresh),
            MenuItem::Separator,
            self.entry("settings", Command::OpenSettings),
            self.entry("quit", Command::Quit),
        ]
    }
}

impl TrayHandle {
    /// Draws a new icon and hover text. ksni signals the change to the panel.
    pub fn draw(&self, status: &Status, theme: &IconTheme) {
        let status = status.clone();
        let theme = theme.clone();
        let _ = self.handle.update(move |tray| {
            tray.status = status;
            tray.theme = theme;
        });
    }

    pub fn set_language(&self, lang: &str) {
        let lang = lang.to_owned();
        let _ = self.handle.update(move |tray| tray.lang = lang);
    }
}

/// Registers the tray icon; the interface drives it through the returned handle.
pub fn spawn(
    lang: &str,
    theme: &IconTheme,
    commands: Arc<Mutex<Vec<Command>>>,
    ctx: egui::Context,
) -> TrayHandle {
    let tray = MonitorTray {
        lang: lang.to_owned(),
        status: Status {
            label: "...".to_owned(),
            state: icon::State::NoData,
            tooltip: crate::i18n::tr(lang, "checking").to_owned(),
        },
        theme: theme.clone(),
        commands,
        ctx,
    };

    let handle = tray
        .spawn()
        .expect("the session bus accepts a StatusNotifierItem");
    TrayHandle { handle }
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
        let fill = theme.state_color(icon::State::Ok);
        let rendered = icon::render(&IconSpec {
            label: "42",
            background: fill,
            foreground: icon::readable_on(fill),
            size: 64,
        });
        assert_eq!(rendered.width, 64);
        // Centre pixel carries the fill colour from the preset table.
        let centre = ((32 * 64 + 32) * 4) as usize;
        assert_eq!(rendered.rgba[centre], 0x3c);
        assert_eq!(rendered.rgba[centre + 1], 0x69);
        assert_eq!(rendered.rgba[centre + 2], 0x66);
        assert_eq!(rendered.rgba[centre + 3], 255);
    }
}
