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
use dsmon_core::model::{preferred_balance, Balance, ConsumptionRate};
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
    /// The day whose brisk spending has been reported, so a day is only said
    /// once.
    brisk_reported_on: Option<chrono::NaiveDate>,
    /// Whether the off-peak discount was in force at the last judgement, so a
    /// phase change is noticed once and only once.
    off_peak: Option<bool>,
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
        if let Some(brisk) = self.brisk_message(snapshot, config, lang) {
            messages.push(brisk);
        }
        if let Some(peak) = self.peak_message(config, lang) {
            messages.push(peak);
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

    /// DeepSeek's off-peak discount starting or ending, said once each way.
    ///
    /// The previous build tied this to its "preferred platform" being DeepSeek;
    /// there is no preferred platform here, so it speaks whenever the discount
    /// changes phase — which is a fact about the vendor's clock, not about any
    /// account.
    fn peak_message(&mut self, config: &AppConfig, lang: &str) -> Option<Message> {
        if !config.peak_alert_enabled {
            return None;
        }

        let off_peak = dsmon_core::time::is_off_peak();
        let previous = self.off_peak.replace(off_peak)?;
        if previous == off_peak {
            return None;
        }

        Some(if off_peak {
            Message {
                title: tr(lang, "off_peak_title").to_owned(),
                body: tr(lang, "off_peak_body").to_owned(),
            }
        } else {
            Message {
                title: tr(lang, "peak_title").to_owned(),
                body: tr(lang, "peak_body").to_owned(),
            }
        })
    }

    /// A day whose spending passed the line, said once for that day.
    ///
    /// The memory is the date, so a new day speaks on its own; a day that never
    /// crosses the line clears it, in case the line was lowered since.
    fn brisk_message(
        &mut self,
        snapshot: &Snapshot,
        config: &AppConfig,
        lang: &str,
    ) -> Option<Message> {
        if !snapshot.spending_is_brisk(config) {
            self.brisk_reported_on = None;
            return None;
        }

        let (currency, spent) = snapshot.today_spend.as_ref()?;
        let today = Local::now().date_naive();
        if self.brisk_reported_on == Some(today) {
            return None;
        }
        self.brisk_reported_on = Some(today);

        Some(Message {
            title: tr(lang, "brisk_title").to_owned(),
            body: format!(
                "{} {} {}, {} {} {}",
                tr(lang, "brisk_body"),
                format_amount(*spent),
                currency,
                tr(lang, "threshold"),
                format_amount(config.brisk_threshold_yuan),
                currency,
            ),
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

/// The summary the previous build raised when the tray icon was clicked: what
/// the balance is, how fast it is going, how the service is and how old the
/// reading is.
pub fn balance_message(snapshot: &Snapshot, lang: &str) -> Message {
    let separator = if lang == "en" { ": " } else { "：" };
    let mut lines = Vec::new();

    if let Some((currency, balance)) = snapshot
        .balances
        .get(storage::KEY_DEEPSEEK)
        .and_then(preferred_balance)
    {
        lines.push(format!("💰 {}", balance_line(lang, currency, balance)));
        if let Some(rate) = snapshot.consumption_rates.get(storage::KEY_DEEPSEEK) {
            lines.push(format!("📊 {}", rate_line(lang, rate)));
        }
    }

    let status = if snapshot.service_status.is_empty() {
        "unknown"
    } else {
        snapshot.service_status.as_str()
    };
    // The previous build ran the state straight into the label here while the
    // line below it kept its separator; one of the two had to give.
    lines.push(format!(
        "📡 {}{separator}{}",
        tr(lang, "service_status"),
        status_label(lang, status)
    ));

    if let Some(error) = &snapshot.last_error {
        lines.push(format!("🕐 {}{separator}{error}", tr(lang, "query_error")));
    } else if let Some(checked) = snapshot.last_check {
        lines.push(format!(
            "🕐 {}{separator}{}",
            tr(lang, "last_check"),
            relative_time(lang, checked, Local::now())
        ));
    } else {
        lines.push(format!("🕐 {}", tr(lang, "not_checked")));
    }

    Message {
        title: tr(lang, "bal_title").to_owned(),
        body: lines.join("\n"),
    }
}

/// The balance as one line, with what it is made of.
fn balance_line(lang: &str, currency: &str, balance: &Balance) -> String {
    if lang == "en" {
        format!(
            "{} {} (Topped {}, Granted {})",
            format_amount(balance.total_balance),
            currency,
            format_amount(balance.topped_up_balance),
            format_amount(balance.granted_balance)
        )
    } else {
        format!(
            "{} {}（充值 {}，赠送 {}）",
            format_amount(balance.total_balance),
            currency,
            format_amount(balance.topped_up_balance),
            format_amount(balance.granted_balance)
        )
    }
}

/// How fast the balance is going and how long it lasts at that rate.
fn rate_line(lang: &str, rate: &ConsumptionRate) -> String {
    let days = (rate.busy_hours_left / 24.0).floor() as i64;
    let hours = (rate.busy_hours_left % 24.0).floor() as i64;

    if lang == "en" {
        format!(
            "Busy: {:.2}/hr | Est. {}d {}h remaining",
            rate.hourly_rate, days, hours
        )
    } else {
        format!(
            "忙时消耗 {:.2}/小时 | 预计可用 {} 天 {} 小时",
            rate.hourly_rate, days, hours
        )
    }
}

/// The service state as a lamp and its words.
fn status_label(lang: &str, status: &str) -> String {
    let lamp = match status {
        "none" => "🟢",
        "minor" | "maintenance" => "🟡",
        "major" => "🟠",
        "critical" => "🔴",
        _ => "⚪",
    };
    format!("{lamp} {}", status_text(lang, status))
}

/// How long ago a reading was taken, in words rather than a timestamp.
fn relative_time(lang: &str, value: DateTime<Local>, now: DateTime<Local>) -> String {
    let seconds = (now - value).num_seconds().max(0);
    let chinese = lang != "en";

    if seconds < 60 {
        return if chinese { "刚刚" } else { "just now" }.to_owned();
    }

    let (count, unit, english_unit) = if seconds < 3600 {
        (seconds / 60, "分钟", "minutes")
    } else if seconds < 86400 {
        (seconds / 3600, "小时", "hours")
    } else {
        (seconds / 86400, "天", "days")
    };

    if chinese {
        format!("{count} {unit}前")
    } else {
        format!("{count} {english_unit} ago")
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
    fn the_tray_summary_carries_the_reading_the_rate_and_the_time() {
        let mut snapshot = reading(12.5, "none");
        snapshot
            .balances
            .get_mut(storage::KEY_DEEPSEEK)
            .unwrap()
            .insert(
                "CNY".to_owned(),
                Balance {
                    total_balance: 12.5,
                    topped_up_balance: 10.0,
                    granted_balance: 2.5,
                },
            );
        snapshot.consumption_rates.insert(
            storage::KEY_DEEPSEEK.to_owned(),
            ConsumptionRate {
                hourly_rate: 0.25,
                busy_hours_left: 30.0,
                currency: "CNY".to_owned(),
            },
        );
        snapshot.last_check = Some(Local::now() - chrono::Duration::minutes(3));

        let message = balance_message(&snapshot, "zh");
        assert_eq!(message.title, "DeepSeek 余额：");
        assert!(
            message
                .body
                .contains("💰 12.50 CNY（充值 10.00，赠送 2.50）"),
            "{}",
            message.body
        );
        assert!(
            message
                .body
                .contains("📊 忙时消耗 0.25/小时 | 预计可用 1 天 6 小时"),
            "{}",
            message.body
        );
        assert!(
            message.body.contains("📡 服务状态：🟢 服务正常"),
            "{}",
            message.body
        );
        assert!(
            message.body.contains("🕐 上次查询：3 分钟前"),
            "{}",
            message.body
        );
    }

    #[test]
    fn a_failed_reading_takes_the_place_of_the_timestamp() {
        let mut snapshot = reading(0.0, "unknown");
        snapshot.last_error = Some("boom".to_owned());

        let message = balance_message(&snapshot, "en");
        assert!(
            message.body.contains("🕐 Query error: boom"),
            "{}",
            message.body
        );
    }

    #[test]
    fn how_long_ago_a_reading_was_is_said_in_words() {
        let now = Local::now();
        let ago = |seconds: i64| relative_time("zh", now - chrono::Duration::seconds(seconds), now);

        assert_eq!(ago(30), "刚刚");
        assert_eq!(ago(90), "1 分钟前");
        assert_eq!(ago(2 * 3600), "2 小时前");
        assert_eq!(ago(3 * 86400), "3 天前");
        assert_eq!(
            relative_time("en", now - chrono::Duration::seconds(90), now),
            "1 minutes ago"
        );
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

    #[test]
    fn a_brisk_day_is_reported_once() {
        let mut config = AppConfig::default();
        config.brisk_threshold_yuan = 5.0;
        let snapshot = Snapshot {
            today_spend: Some(("CNY".to_owned(), 9.0)),
            ..Snapshot::default()
        };

        let mut watch = Watch::default();
        assert!(
            watch.brisk_message(&snapshot, &config, "en").is_some(),
            "the first judgement of the day speaks"
        );
        assert!(
            watch.brisk_message(&snapshot, &config, "en").is_none(),
            "and the same day does not speak twice"
        );

        // A day that never crossed the line clears the memory, so a line
        // lowered afterwards can still speak.
        let quiet = Snapshot {
            today_spend: Some(("CNY".to_owned(), 1.0)),
            ..Snapshot::default()
        };
        assert!(watch.brisk_message(&quiet, &config, "en").is_none());
        assert!(
            watch.brisk_message(&snapshot, &config, "en").is_some(),
            "and then it can speak again"
        );
    }
}
