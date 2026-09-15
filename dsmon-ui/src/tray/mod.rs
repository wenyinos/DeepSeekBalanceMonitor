//! Tray integration, one implementation per platform.
//!
//! Linux speaks StatusNotifierItem over D-Bus (ksni), Windows uses the Win32
//! notification area (tray-icon). The menu, the commands and the rules for what
//! the icon shows are written here so both platforms behave alike.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "windows")]
use windows as platform;

use std::sync::{Arc, Mutex};

use dsmon_core::catalog::{self, Mode, PlatformMeta};
use dsmon_core::config::AppConfig;
use dsmon_core::history::format_amount;
use dsmon_core::icon::{self, IconTheme, State};
use dsmon_core::model::preferred_balance;
use dsmon_core::monitor::Snapshot;

use crate::i18n::tr;

/// What the tray asks the running application to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Show the reading as a notification, which is what a click on the icon
    /// has always meant.
    ShowBalance,
    /// Bring the main window to the front.
    OpenWindow,
    /// Poll again straight away.
    Refresh,
    /// Bring up the main window on the settings page.
    OpenSettings,
    Quit,
}

/// The figure, the colour and the hover text the icon carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub label: String,
    pub state: State,
    pub tooltip: String,
}

/// What the icon shows for one snapshot.
///
/// The rules are the ones the previous build used, so the icon keeps behaving
/// the way its users know it: an ellipsis while a poll runs, an exclamation
/// mark after a failure, and the rounded balance — two digits at most — the
/// rest of the time.
pub fn status(snapshot: &Snapshot, config: &AppConfig, lang: &str) -> Status {
    if snapshot.checking {
        return Status {
            label: "...".to_owned(),
            state: State::NoData,
            tooltip: tr(lang, "checking").to_owned(),
        };
    }

    if let Some(error) = &snapshot.last_error {
        return Status {
            label: "!".to_owned(),
            state: State::Low,
            tooltip: format!("{}: {error}", tr(lang, "error")),
        };
    }

    match reading(snapshot) {
        Some((meta, currency, total)) => Status {
            label: icon::icon_label(total),
            state: state_of(snapshot, config, total),
            tooltip: format!(
                "{} {}: {} {}",
                meta.display_name,
                tr(lang, "balance_word"),
                format_amount(total),
                currency
            ),
        },
        None => Status {
            label: "...".to_owned(),
            state: State::NoData,
            tooltip: tr(lang, "checking").to_owned(),
        },
    }
}

/// The reading the icon stands for: DeepSeek when it is configured, otherwise
/// the first other balance provider that answered.
fn reading(snapshot: &Snapshot) -> Option<(PlatformMeta, String, f64)> {
    catalog::implemented()
        .filter(|meta| meta.mode == Mode::Payg)
        .find_map(|meta| {
            snapshot
                .balances
                .get(meta.key)
                .and_then(preferred_balance)
                .map(|(currency, balance)| (*meta, currency.clone(), balance.total_balance))
        })
}

/// Low balance first, then a degraded service, the order the previous build
/// used.
fn state_of(snapshot: &Snapshot, config: &AppConfig, total: f64) -> State {
    if total < config.threshold_yuan {
        State::Low
    } else if matches!(
        snapshot.service_status.as_str(),
        "maintenance" | "minor" | "major" | "critical"
    ) {
        State::Degraded
    } else {
        State::Ok
    }
}

/// Handle to the tray icon: draws it, and hands what the user picked to the
/// queue the application shares with everything else that can ask it to do
/// something.
pub struct Tray {
    /// Nothing at all when the desktop would not take an icon: the application
    /// keeps working, and the window stays on screen because there is no tray
    /// to bring it back from.
    handle: Option<platform::TrayHandle>,
    /// What the icon shows right now, so an unchanged reading costs nothing.
    published: Mutex<Option<(Status, IconTheme)>>,
}

impl Tray {
    /// Registers the icon and its menu.
    ///
    /// Menu picks land in the queue read by [`Tray::take_commands`]; `ctx` is
    /// woken as they arrive, so the interface reacts without waiting for the
    /// next frame.
    pub fn spawn(
        ctx: &egui::Context,
        lang: &str,
        theme: &IconTheme,
        commands: Arc<Mutex<Vec<Command>>>,
    ) -> Self {
        let handle = match platform::spawn(lang, theme, commands, ctx.clone()) {
            Ok(handle) => Some(handle),
            Err(error) => {
                let _ = dsmon_core::storage::log_line(&format!(
                    "There is no tray icon this run: {error}"
                ));
                None
            }
        };

        Self {
            handle,
            published: Mutex::new(None),
        }
    }

    /// Draws `status`, unless the icon already shows it.
    pub fn publish(&self, status: &Status, theme: &IconTheme) {
        let Some(handle) = &self.handle else {
            return;
        };

        let Ok(mut published) = self.published.lock() else {
            return;
        };
        if let Some((last_status, last_theme)) = published.as_ref() {
            if last_status == status && last_theme == theme {
                return;
            }
        }
        handle.draw(status, theme);
        *published = Some((status.clone(), theme.clone()));
    }

    /// Whether the desktop is really showing the icon.
    ///
    /// The question to ask before anything relies on the tray to reach the
    /// application, since the window is the only other way in.
    pub fn is_registered(&self) -> bool {
        self.handle
            .as_ref()
            .is_some_and(|handle| handle.is_registered())
    }

    /// Relabels the menu after the interface language changed.
    pub fn set_language(&self, lang: &str) {
        if let Some(handle) = &self.handle {
            handle.set_language(lang);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsmon_core::model::{Balance, Balances};
    use std::collections::BTreeMap;

    fn snapshot_with(total: f64) -> Snapshot {
        let mut balances = Balances::default();
        balances.insert(
            "CNY".to_owned(),
            Balance {
                total_balance: total,
                topped_up_balance: total,
                granted_balance: 0.0,
            },
        );
        let mut snapshot = Snapshot::default();
        snapshot.balances.insert("deepseek".to_owned(), balances);
        snapshot
    }

    #[test]
    fn the_icon_carries_the_balance_figure() {
        let config = AppConfig::default();
        let reported = status(&snapshot_with(12.4), &config, "zh");
        assert_eq!(reported.label, "12");
        assert_eq!(reported.state, State::Ok);
        assert!(
            reported.tooltip.contains("12.40 CNY"),
            "{}",
            reported.tooltip
        );

        // Above two digits the figure collapses, below the threshold it turns
        // into the low-balance colour.
        assert_eq!(status(&snapshot_with(1234.0), &config, "zh").label, "OK");
        assert_eq!(status(&snapshot_with(0.5), &config, "zh").state, State::Low);
    }

    #[test]
    fn a_running_poll_and_a_failure_replace_the_figure() {
        let config = AppConfig::default();

        let mut checking = snapshot_with(12.4);
        checking.checking = true;
        assert_eq!(status(&checking, &config, "zh").label, "...");

        let mut failed = snapshot_with(12.4);
        failed.last_error = Some("boom".to_owned());
        let reported = status(&failed, &config, "zh");
        assert_eq!(reported.label, "!");
        assert!(reported.tooltip.contains("boom"));
    }

    #[test]
    fn a_reading_from_another_provider_still_reaches_the_icon() {
        let mut snapshot = Snapshot::default();
        let mut balances = Balances::default();
        balances.insert(
            "USD".to_owned(),
            Balance {
                total_balance: 7.0,
                topped_up_balance: 7.0,
                granted_balance: 0.0,
            },
        );
        snapshot.balances.insert("openrouter".to_owned(), balances);

        let reported = status(&snapshot, &AppConfig::default(), "zh");
        assert_eq!(
            reported.label, "7.0",
            "below ten the figure keeps a decimal"
        );
        assert!(
            reported.tooltip.contains("OpenRouter"),
            "{}",
            reported.tooltip
        );
    }

    #[test]
    fn a_degraded_service_colours_the_icon_without_changing_the_figure() {
        let mut snapshot = snapshot_with(12.4);
        snapshot.service_status = "major".to_owned();
        let reported = status(&snapshot, &AppConfig::default(), "zh");
        assert_eq!(reported.label, "12");
        assert_eq!(reported.state, State::Degraded);
    }

    #[test]
    fn an_empty_snapshot_says_it_is_still_working() {
        let reported = status(&Snapshot::default(), &AppConfig::default(), "en");
        assert_eq!(reported.label, "...");
        assert_eq!(reported.state, State::NoData);
        assert_eq!(reported.tooltip, "Checking...");
    }

    #[test]
    fn the_icon_follows_the_configured_colour_style() {
        let theme = IconTheme {
            style: "custom".to_owned(),
            custom: BTreeMap::from([("ok".to_owned(), "#112233".to_owned())]),
        };
        assert_eq!(theme.state_color(State::Ok), [0x11, 0x22, 0x33]);
    }
}
