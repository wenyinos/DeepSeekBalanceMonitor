//! Timestamps, in the exact shape the history table and the previous builds
//! used: `YYYY-MM-DD HH:MM:SS` in local time.

use chrono::{DateTime, Local};

/// Formats a timestamp the way `balance_history.timestamp` stores it.
pub fn format_local(value: DateTime<Local>) -> String {
    value.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Current local time in storage format.
pub fn now() -> String {
    format_local(Local::now())
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
