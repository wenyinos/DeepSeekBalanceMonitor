//! The widget's own tray entry.
//!
//! The application has one too, and both can be in the tray at once. They are
//! told apart by their artwork — every mark the widget shows carries the badge
//! (see `dsmon_core::icon::widget_icon`) — and this menu holds the widget's own
//! business: put the panel away or bring it back, raise the application, or end
//! the widget.

use std::sync::{Arc, Mutex};

/// What the tray asks the widget to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Put the panel away, or bring it back.
    ToggleWindow,
    /// Raise the application, starting it when it is not running.
    OpenApplication,
    /// End the widget, exactly as its close button does.
    Quit,
}

/// Where the tray writes what the user picked. The widget reads it once a frame
/// from `logic`, so a menu pick works whether or not the window is on screen.
pub type Queue = Arc<Mutex<Vec<Command>>>;

/// The tray entry, alive for as long as it is held.
pub struct Tray {
    handle: Option<platform::Handle>,
}

impl Tray {
    /// Registers the icon and its menu.
    pub fn spawn(lang: &str, commands: Queue, ctx: &egui::Context) -> Self {
        let handle = match platform::spawn(lang, commands, ctx.clone()) {
            Ok(handle) => Some(handle),
            Err(error) => {
                let _ = dsmon_core::storage::log_line(&format!(
                    "The widget has no tray icon this run: {error}"
                ));
                None
            }
        };

        Self { handle }
    }

    /// Relabels the menu after the interface language changed.
    pub fn set_language(&self, lang: &str) {
        if let Some(handle) = &self.handle {
            handle.set_language(lang);
        }
    }

    /// Points the show/hide entry at the other action.
    pub fn set_visible(&self, visible: bool) {
        if let Some(handle) = &self.handle {
            handle.set_visible(visible);
        }
    }

    /// Writes a line the first time the answer to "is the icon really there"
    /// changes.
    ///
    /// Only Windows has a question to ask: on Linux the item travels over the
    /// bus from the moment it is built, and whether a panel then draws it is
    /// the panel's business. A Windows shell can refuse the icon without the
    /// library saying so, which leaves "the widget is running and there is
    /// nothing in the tray" with no explanation anywhere — the answer arrives a
    /// frame or two late, hence the check every frame rather than once.
    #[cfg(windows)]
    pub fn report(&self) {
        use std::sync::atomic::{AtomicU8, Ordering};

        static REPORTED: AtomicU8 = AtomicU8::new(0);

        let Some(handle) = &self.handle else {
            return;
        };
        let state = if handle.is_registered() { 1 } else { 2 };
        if REPORTED.swap(state, Ordering::SeqCst) != state {
            let _ = dsmon_core::storage::log_line(match state {
                1 => "The widget's tray icon is registered.",
                _ => "The shell is not holding the widget's tray icon.",
            });
        }
    }

    /// Nothing to ask on this platform.
    #[cfg(not(windows))]
    pub fn report(&self) {}
}

/// The wording of the show/hide entry: the action rather than the state,
/// because neither menu can show a tick (see `crate::tray::widget_label`).
pub fn toggle_label(lang: &str, visible: bool) -> &'static str {
    crate::i18n::tr(
        lang,
        if visible {
            "widget_menu_hide"
        } else {
            "widget_menu_show"
        },
    )
}

/// What the tray calls itself, in the menu as well as in a hover.
fn title(lang: &str) -> String {
    crate::i18n::tr(lang, "widget_title").to_owned()
}

#[cfg(target_os = "linux")]
mod platform {
    use ksni::{
        blocking::{Handle as KsniHandle, TrayMethods},
        menu::{MenuItem, StandardItem},
        Icon, ToolTip, Tray as KsniTray,
    };

    use super::{title, toggle_label, Command, Queue};

    /// Keeps the item registered for as long as it is held.
    pub struct Handle {
        handle: KsniHandle<WidgetTray>,
    }

    /// The item the panel draws, and everything its menu needs.
    pub struct WidgetTray {
        lang: String,
        /// Whether the panel is on screen, which is what the menu entry offers
        /// to change.
        visible: bool,
        commands: Queue,
        ctx: egui::Context,
    }

    impl WidgetTray {
        fn send(&self, command: Command) {
            if let Ok(mut queue) = self.commands.lock() {
                queue.push(command);
            }
            self.ctx.request_repaint();
        }

        fn entry(&self, key: &str, command: Command) -> MenuItem<Self> {
            StandardItem {
                label: crate::i18n::tr(&self.lang, key).to_owned(),
                activate: Box::new(move |tray: &mut Self| tray.send(command)),
                ..Default::default()
            }
            .into()
        }
    }

    impl KsniTray for WidgetTray {
        fn id(&self) -> String {
            "com.github.wenyinos.deepseek-balance-monitor-widget".to_owned()
        }

        fn icon_pixmap(&self) -> Vec<Icon> {
            match dsmon_core::icon::widget_icon(64) {
                Some(rendered) => vec![Icon {
                    width: rendered.width as i32,
                    height: rendered.height as i32,
                    data: dsmon_core::icon::argb32(&rendered.rgba),
                }],
                None => Vec::new(),
            }
        }

        fn title(&self) -> String {
            title(&self.lang)
        }

        fn tool_tip(&self) -> ToolTip {
            ToolTip {
                title: title(&self.lang),
                ..Default::default()
            }
        }

        /// Left click: put the panel away, or bring it back.
        fn activate(&mut self, _x: i32, _y: i32) {
            self.send(Command::ToggleWindow);
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            vec![
                StandardItem {
                    label: toggle_label(&self.lang, self.visible).to_owned(),
                    activate: Box::new(|tray: &mut Self| tray.send(Command::ToggleWindow)),
                    ..Default::default()
                }
                .into(),
                MenuItem::Separator,
                self.entry("widget_start_app", Command::OpenApplication),
                self.entry("quit", Command::Quit),
            ]
        }
    }

    impl Handle {
        pub fn set_language(&self, lang: &str) {
            let lang = lang.to_owned();
            let _ = self.handle.update(move |tray| tray.lang = lang);
        }

        pub fn set_visible(&self, visible: bool) {
            let _ = self.handle.update(move |tray| tray.visible = visible);
        }
    }

    pub fn spawn(lang: &str, commands: Queue, ctx: egui::Context) -> Result<Handle, String> {
        let tray = WidgetTray {
            lang: lang.to_owned(),
            visible: true,
            commands,
            ctx,
        };

        let handle = tray
            .spawn()
            .map_err(|error| format!("the session bus refused the item: {error}"))?;
        Ok(Handle { handle })
    }
}

#[cfg(windows)]
mod platform {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use tray_icon::{
        menu::{Menu, MenuEvent, MenuItem},
        MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    };

    use super::{title, toggle_label, Command, Queue};

    const ID_TOGGLE: &str = "widget-toggle";
    const ID_OPEN: &str = "widget-open";
    const ID_QUIT: &str = "widget-quit";

    /// Keeps the icon registered for as long as it is held.
    pub struct Handle {
        icon: TrayIcon,
        toggle: MenuItem,
        lang: Mutex<String>,
        visible: AtomicBool,
    }

    impl Handle {
        /// Whether the shell is really holding the icon.
        ///
        /// The library keeps a refused registration to itself —
        /// `Shell_NotifyIconW` answering "no" is not an error it reports — so
        /// the only way to learn the truth is to ask the shell for the icon's
        /// rectangle, which it answers only for an icon it holds. The
        /// application learned this the hard way (see the AGENTS.md note about
        /// the Windows tray); the widget needs the same answer for its log.
        pub fn is_registered(&self) -> bool {
            self.icon.rect().is_some()
        }

        pub fn set_language(&self, lang: &str) {
            if let Ok(mut stored) = self.lang.lock() {
                *stored = lang.to_owned();
            }
            self.relabel(lang);
        }

        pub fn set_visible(&self, visible: bool) {
            self.visible.store(visible, Ordering::Relaxed);
            let lang = self
                .lang
                .lock()
                .map(|lang| lang.clone())
                .unwrap_or_else(|_| "en".to_owned());
            self.relabel(&lang);
        }

        fn relabel(&self, lang: &str) {
            self.toggle
                .set_text(toggle_label(lang, self.visible.load(Ordering::Relaxed)));
        }
    }

    pub fn spawn(lang: &str, commands: Queue, ctx: egui::Context) -> Result<Handle, String> {
        let text = |key: &str| crate::i18n::tr(lang, key).to_owned();

        let menu = Menu::new();
        let toggle = MenuItem::with_id(ID_TOGGLE, toggle_label(lang, true), true, None);
        let open = MenuItem::with_id(ID_OPEN, text("widget_start_app"), true, None);
        let quit = MenuItem::with_id(ID_QUIT, text("quit"), true, None);
        let separator = tray_icon::menu::PredefinedMenuItem::separator();
        for entry in [
            &toggle as &dyn tray_icon::menu::IsMenuItem,
            &separator,
            &open,
            &quit,
        ] {
            let _ = menu.append(entry);
        }

        let rendered =
            dsmon_core::icon::widget_icon(64).ok_or("the widget's own icon could not be drawn")?;
        let image = tray_icon::Icon::from_rgba(rendered.rgba, rendered.width, rendered.height)
            .map_err(|error| format!("the icon it draws was refused: {error}"))?;

        // A left click puts the panel away, the way the application's icon
        // shows the balance: the menu waits for the right button.
        let icon = TrayIconBuilder::new()
            .with_tooltip(title(lang))
            .with_icon(image)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|error| format!("the shell refused an icon: {error}"))?;

        install_handlers(commands, ctx);

        Ok(Handle {
            icon,
            toggle,
            lang: Mutex::new(lang.to_owned()),
            visible: AtomicBool::new(true),
        })
    }

    /// Routes menu picks and clicks into the queue and wakes the widget.
    fn install_handlers(commands: Queue, ctx: egui::Context) {
        let menu_commands = Arc::clone(&commands);
        let menu_ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let command = match event.id.0.as_str() {
                ID_TOGGLE => Some(Command::ToggleWindow),
                ID_OPEN => Some(Command::OpenApplication),
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
                push(&commands, Command::ToggleWindow);
                ctx.request_repaint();
            }
        }));
    }

    fn push(commands: &Queue, command: Command) {
        if let Ok(mut queue) = commands.lock() {
            queue.push(command);
        }
    }
}
