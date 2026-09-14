//! System notifications.
//!
//! The four occasions the previous build raised one, with the policy that stood
//! behind each: a low balance per the alert mode, a service status that moved,
//! a start with nothing configured, and a database that had to be recreated.
//!
//! What is said and when is decided here, in a form that can be tested; how it
//! is delivered belongs to the platform modules.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
pub(crate) mod windows;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "windows")]
use windows as platform;

use chrono::{DateTime, Local};

use dsmon_core::config::AppConfig;
use dsmon_core::history::format_amount;
use dsmon_core::model::preferred_balance;
use dsmon_core::monitor::Snapshot;
use dsmon_core::storage;

use crate::i18n::{status_text, tr};

/// One notification, ready to hand to the desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub title: String,
    pub body: String,
}

/// Hands a message to the desktop without holding up the interface: a
/// notification service that stops answering must not freeze the window.
pub fn send(message: Message) {
    std::thread::spawn(move || {
        if let Err(error) = platform::send(&message) {
            let _ = storage::log_line(&format!("Notification failed: {error}"));
        }
    });
}

/// What the last reading left behind, so the next one can tell what changed.
#[derive(Debug, Default, Clone)]
pub struct Watch {
    /// Set while a low balance has been reported and has stayed low.
    low_balance_reported: bool,
    /// The service status of the previous reading.
    service_status: Option<String>,
    /// The reading the last judgement was passed on, so redrawing the window
    /// does not repeat a notification.
    judged: Option<DateTime<Local>>,
}

impl Watch {
    /// What this snapshot calls for, or nothing when nothing has moved.
    pub fn judge(&mut self, snapshot: &Snapshot, config: &AppConfig, lang: &str) -> Vec<Message> {
        let mut messages = Vec::new();

        // One judgement per reading. This runs on every frame otherwise.
        let Some(checked) = snapshot.last_check else {
            return messages;
        };
        if self.judged == Some(checked) {
            return messages;
        }
        self.judged = Some(checked);

        if snapshot.demo {
            return messages;
        }

        if let Some(status) = self.service_status_message(snapshot, config, lang) {
            messages.push(status);
        }
        if let Some(balance) = self.low_balance_message(snapshot, config, lang) {
            messages.push(balance);
        }

        messages
    }

    /// A status page that moved from one known state to another, when alerts
    /// for it are on. "unknown" is not a move: it only means the page could not
    /// be read this time.
    fn service_status_message(
        &mut self,
        snapshot: &Snapshot,
        config: &AppConfig,
        lang: &str,
    ) -> Option<Message> {
        let status = snapshot.service_status.as_str();
        let previous = self.service_status.replace(status.to_owned())?;

        if !config.api_alert_enabled
            || status.is_empty()
            || previous.is_empty()
            || status == "unknown"
            || previous == "unknown"
            || status == previous
        {
            return None;
        }

        Some(if is_degraded(status) {
            Message {
                title: tr(lang, "api_degraded_title").to_owned(),
                body: format!(
                    "{}{}",
                    tr(lang, "api_degraded_msg"),
                    status_text(lang, status)
                ),
            }
        } else {
            Message {
                title: tr(lang, "api_recovered_title").to_owned(),
                body: tr(lang, "api_recovered_msg").to_owned(),
            }
        })
    }

    /// A balance under the alert line, as often as the alert mode asks for.
    ///
    /// The line is the one setting that exists, and it is the DeepSeek account
    /// it was written for, so that is the reading it is compared against.
    fn low_balance_message(
        &mut self,
        snapshot: &Snapshot,
        config: &AppConfig,
        lang: &str,
    ) -> Option<Message> {
        let (currency, balance) = preferred_balance(snapshot.balances.get(storage::KEY_DEEPSEEK)?)?;

        if balance.total_balance >= config.threshold_yuan {
            self.low_balance_reported = false;
            return None;
        }

        let report = match config.alert_mode.as_str() {
            "never" => false,
            "always" => true,
            _ if self.low_balance_reported => false,
            _ => {
                self.low_balance_reported = true;
                true
            }
        };
        if !report {
            return None;
        }

        Some(Message {
            title: tr(lang, "low_balance_title").to_owned(),
            body: format!(
                "{} {} {}, {} {} {}",
                tr(lang, "low_balance_body"),
                format_amount(balance.total_balance),
                currency,
                tr(lang, "threshold"),
                format_amount(config.threshold_yuan),
                currency,
            ),
        })
    }
}

/// A start with nothing configured at all: there is nothing to monitor yet.
pub fn missing_key_message(lang: &str) -> Message {
    Message {
        title: tr(lang, "api_key_missing_title").to_owned(),
        body: tr(lang, "api_key_missing_body").to_owned(),
    }
}

/// A start after the database had to be built again, which may have taken the
/// stored keys and history with it.
pub fn recreated_database_message(lang: &str) -> Message {
    Message {
        title: tr(lang, "database_recreated_title").to_owned(),
        body: tr(lang, "database_recreated_body").to_owned(),
    }
}

/// Whether a service status means trouble.
fn is_degraded(status: &str) -> bool {
    matches!(status, "maintenance" | "minor" | "major" | "critical")
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsmon_core::model::{Balance, Balances};

    fn reading(total: f64, status: &str) -> Snapshot {
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
        snapshot
            .balances
            .insert(storage::KEY_DEEPSEEK.to_owned(), balances);
        snapshot.service_status = status.to_owned();
        snapshot.last_check = Some(Local::now());
        snapshot
    }

    fn config() -> AppConfig {
        AppConfig::default()
    }

    #[test]
    fn a_reading_is_judged_once() {
        let mut watch = Watch::default();
        let snapshot = reading(0.5, "none");

        let first = watch.judge(&snapshot, &config(), "en");
        assert_eq!(first.len(), 1, "a low balance is reported once");
        assert!(first[0].title.contains("Low Balance"), "{}", first[0].title);

        assert!(
            watch.judge(&snapshot, &config(), "en").is_empty(),
            "the same reading is not judged twice"
        );
    }

    #[test]
    fn the_alert_mode_decides_how_often_a_low_balance_is_reported() {
        let mut always = config();
        always.alert_mode = "always".to_owned();
        let mut watch = Watch::default();
        let mut snapshot = reading(0.5, "none");

        assert_eq!(watch.judge(&snapshot, &always, "en").len(), 1);
        snapshot.last_check = Some(Local::now() + chrono::Duration::minutes(1));
        assert_eq!(
            watch.judge(&snapshot, &always, "en").len(),
            1,
            "always reports every reading"
        );

        let mut never = config();
        never.alert_mode = "never".to_owned();
        let mut watch = Watch::default();
        assert!(watch.judge(&snapshot, &never, "en").is_empty());
    }

    #[test]
    fn a_balance_that_recovers_and_drops_again_is_reported_again() {
        let mut watch = Watch::default();
        let mut snapshot = reading(0.5, "none");
        assert_eq!(watch.judge(&snapshot, &config(), "en").len(), 1);

        snapshot = reading(5.0, "none");
        assert!(watch.judge(&snapshot, &config(), "en").is_empty());

        snapshot = reading(0.4, "none");
        assert_eq!(
            watch.judge(&snapshot, &config(), "en").len(),
            1,
            "the once-only rule resets when the balance recovers"
        );
    }

    #[test]
    fn a_service_status_that_moves_is_reported_both_ways() {
        let mut watch = Watch::default();
        let mut snapshot = reading(5.0, "none");
        assert!(
            watch.judge(&snapshot, &config(), "en").is_empty(),
            "the first reading only establishes what to compare against"
        );

        snapshot.service_status = "major".to_owned();
        snapshot.last_check = Some(Local::now() + chrono::Duration::minutes(1));
        let degraded = watch.judge(&snapshot, &config(), "en");
        assert_eq!(degraded.len(), 1);
        assert!(
            degraded[0].title.contains("Degraded"),
            "{}",
            degraded[0].title
        );

        snapshot.service_status = "none".to_owned();
        snapshot.last_check = Some(Local::now() + chrono::Duration::minutes(2));
        let recovered = watch.judge(&snapshot, &config(), "en");
        assert_eq!(recovered.len(), 1);
        assert!(
            recovered[0].title.contains("Recovered"),
            "{}",
            recovered[0].title
        );
    }

    /// A status page that could not be read is not a change of state, in either
    /// direction — the same rule the previous build applied.
    #[test]
    fn an_unreadable_status_page_is_not_a_change() {
        let mut watch = Watch::default();
        let mut snapshot = reading(5.0, "unknown");
        watch.judge(&snapshot, &config(), "en");

        snapshot.service_status = "major".to_owned();
        snapshot.last_check = Some(Local::now() + chrono::Duration::minutes(1));
        assert!(watch.judge(&snapshot, &config(), "en").is_empty());

        snapshot.service_status = "unknown".to_owned();
        snapshot.last_check = Some(Local::now() + chrono::Duration::minutes(2));
        assert!(watch.judge(&snapshot, &config(), "en").is_empty());
    }

    #[test]
    fn status_alerts_can_be_switched_off() {
        let mut config = config();
        config.api_alert_enabled = false;

        let mut watch = Watch::default();
        let mut snapshot = reading(5.0, "none");
        watch.judge(&snapshot, &config, "en");

        snapshot.service_status = "major".to_owned();
        snapshot.last_check = Some(Local::now() + chrono::Duration::minutes(1));
        assert!(watch.judge(&snapshot, &config, "en").is_empty());
    }

    #[test]
    fn demo_readings_never_notify() {
        let mut watch = Watch::default();
        let mut snapshot = reading(0.1, "critical");
        snapshot.demo = true;
        assert!(watch.judge(&snapshot, &config(), "en").is_empty());
    }

    #[test]
    fn the_two_start_up_notices_carry_their_own_words() {
        for lang in ["zh", "en"] {
            let missing = missing_key_message(lang);
            let recreated = recreated_database_message(lang);

            assert!(!missing.title.is_empty() && !missing.body.is_empty());
            assert!(!recreated.title.is_empty() && !recreated.body.is_empty());
            assert_ne!(missing.title, recreated.title, "the two are distinct");
        }
    }
}
