//! Timestamps, in the exact shape the history table and the previous builds
//! used: `YYYY-MM-DD HH:MM:SS` in local time.

use chrono::{DateTime, Datelike, Local, Timelike, Utc, Weekday};

/// Formats a timestamp the way `balance_history.timestamp` stores it.
pub fn format_local(value: DateTime<Local>) -> String {
    value.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Current local time in storage format.
pub fn now() -> String {
    format_local(Local::now())
}

/// Whether DeepSeek's off-peak discount is in force at `now`.
///
/// The discount is suspended during the peak hours — 09:00–12:00 and
/// 14:00–18:00 Beijing time, Monday to Friday — and applies the rest of the
/// time: evenings, nights, the lunch break and the whole weekend. Compared
/// against +08:00 rather than the machine's own zone, because the discount
/// follows the vendor's clock and not the user's.
pub fn is_off_peak_at(now: DateTime<Utc>) -> bool {
    let beijing = now + chrono::Duration::hours(8);
    if matches!(beijing.weekday(), Weekday::Sat | Weekday::Sun) {
        return true;
    }

    let minutes = beijing.hour() * 60 + beijing.minute();
    ![(9 * 60, 12 * 60), (14 * 60, 18 * 60)]
        .iter()
        .any(|(from, to)| (*from..*to).contains(&minutes))
}

/// The same question about this moment.
pub fn is_off_peak() -> bool {
    is_off_peak_at(Utc::now())
}

/// Parses a stored timestamp back into local time.
pub fn parse_local(value: &str) -> Option<DateTime<Local>> {
    chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .ok()
        .and_then(|naive| naive.and_local_timezone(Local).single())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_is_beijing_weekday_mornings_and_afternoons() {
        let at = |iso: &str| is_off_peak_at(iso.parse::<DateTime<Utc>>().expect("a timestamp"));

        // 2026-09-16 is a Wednesday; Beijing is eight hours ahead of UTC.
        assert!(!at("2026-09-16T02:00:00Z"), "10:00 Beijing is peak");
        assert!(!at("2026-09-16T07:00:00Z"), "15:00 Beijing is peak");
        assert!(at("2026-09-16T04:00:00Z"), "12:00-14:00 is the lunch break");
        assert!(at("2026-09-16T00:30:00Z"), "08:30 Beijing is off-peak");
        assert!(at("2026-09-16T13:00:00Z"), "21:00 Beijing is off-peak");

        // The weekend is off-peak all day, 10:00 on Saturday included.
        assert!(at("2026-09-19T02:00:00Z"), "Saturday is off-peak");
        assert!(at("2026-09-20T02:00:00Z"), "Sunday is off-peak");
    }

    #[test]
    fn round_trips_a_timestamp() {
        let text = "2026-01-02 03:04:05";
        let parsed = parse_local(text).expect("timestamp parses");
        assert_eq!(format_local(parsed), text);
    }

    #[test]
    fn rejects_malformed_input() {
        assert!(parse_local("2026-01-02T03:04:05Z").is_none());
        assert!(parse_local("").is_none());
    }
}
