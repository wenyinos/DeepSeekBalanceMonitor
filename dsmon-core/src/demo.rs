//! Demo mode: a fixed set of readings used for screenshots and offline trials.
//!
//! Entering `demo` as the API key switches every reader to the
//! `demo_mode_balance` table instead of the live API.

use chrono::{Duration as ChronoDuration, Local};
use rusqlite::{params, Connection};

use crate::model::{Balance, Balances, ConsumptionRate, HistoryRecord};
use crate::time;

const API_KEY: &str = "demo";
const CURRENCY: &str = "CNY";
const TOTAL: f64 = 666.0;
const TOPPED: f64 = 114_514.0;
const GRANTED: f64 = 1_919_810.0;
const HOURLY_RATE: f64 = 114_514.0;
const BUSY_HOURS_LEFT: f64 = 1_919_810.0;

/// `(minutes ago, total, topped, granted, service status)`, newest last.
const SNAPSHOTS: &[(i64, f64, f64, f64, &str)] = &[
    (240, 1_919_810.0, 114_514.0, 1_805_296.0, "none"),
    (180, 1_145_140.0, 114_514.0, 1_030_626.0, "none"),
    (120, 666_666.0, 114_514.0, 552_152.0, "minor"),
    (60, 114_514.0, 66_600.0, 47_914.0, "none"),
    (0, TOTAL, TOPPED, GRANTED, "none"),
];

/// Whether the configured key asks for the demo data set.
pub fn is_enabled(api_key: &str) -> bool {
    api_key.trim().eq_ignore_ascii_case(API_KEY)
}

/// Rebuilds the demo table, timestamped relative to now.
pub fn prepare(conn: &Connection) -> Result<(), String> {
    conn.execute("DROP TABLE IF EXISTS demo_mode_balance", [])
        .map_err(|error| error.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS demo_mode_balance (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            currency TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            total REAL NOT NULL,
            topped REAL NOT NULL,
            granted REAL NOT NULL,
            hourly_rate REAL NOT NULL,
            busy_hours_left REAL NOT NULL,
            service_status TEXT NOT NULL
        )",
        [],
    )
    .map_err(|error| error.to_string())?;

    for (minutes_ago, total, topped, granted, service_status) in SNAPSHOTS {
        conn.execute(
            "INSERT INTO demo_mode_balance
             (currency, timestamp, total, topped, granted, hourly_rate, busy_hours_left, service_status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                CURRENCY,
                time::format_local(Local::now() - ChronoDuration::minutes(*minutes_ago)),
                total,
                topped,
                granted,
                HOURLY_RATE,
                BUSY_HOURS_LEFT,
                service_status
            ],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// The newest demo reading.
pub fn balances(conn: &Connection) -> Result<Balances, String> {
    let mut stmt = conn
        .prepare(
            "SELECT currency, total, topped, granted
             FROM demo_mode_balance
             WHERE timestamp = (SELECT MAX(timestamp) FROM demo_mode_balance)
             ORDER BY currency",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                Balance {
                    total_balance: row.get(1)?,
                    topped_up_balance: row.get(2)?,
                    granted_balance: row.get(3)?,
                },
            ))
        })
        .map_err(|error| error.to_string())?;

    let mut balances = Balances::new();
    for row in rows {
        let (currency, balance) = row.map_err(|error| error.to_string())?;
        balances.insert(currency, balance);
    }
    Ok(balances)
}

/// The canned burn-rate estimate.
pub fn consumption_rate(conn: &Connection) -> Result<ConsumptionRate, String> {
    conn.query_row(
        "SELECT currency, hourly_rate, busy_hours_left FROM demo_mode_balance LIMIT 1",
        [],
        |row| {
            Ok(ConsumptionRate {
                currency: row.get(0)?,
                hourly_rate: row.get(1)?,
                busy_hours_left: row.get(2)?,
            })
        },
    )
    .map_err(|error| error.to_string())
}

/// The demo readings, oldest first.
pub fn history(conn: &Connection, limit: usize) -> Result<Vec<HistoryRecord>, String> {
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, currency, total, topped, granted, service_status
             FROM demo_mode_balance
             ORDER BY timestamp ASC
             LIMIT ?1",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(HistoryRecord {
                timestamp: row.get(0)?,
                currency: row.get(1)?,
                total: row.get(2)?,
                topped: row.get(3)?,
                granted: row.get(4)?,
                service_status: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(|error| error.to_string())?);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_demo_key_enables_it() {
        assert!(is_enabled("demo"));
        assert!(is_enabled(" DEMO "));
        assert!(!is_enabled("sk-live"));
        assert!(!is_enabled(""));
    }

    #[test]
    fn prepares_and_reads_the_demo_data() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite opens");
        prepare(&conn).expect("demo table prepares");

        let balances = balances(&conn).expect("balances load");
        let (currency, balance) = balances.iter().next().expect("one currency");
        assert_eq!(currency, "CNY");
        assert_eq!(balance.total_balance, TOTAL);
        assert_eq!(balance.topped_up_balance, TOPPED);
        assert_eq!(balance.granted_balance, GRANTED);

        let rate = consumption_rate(&conn).expect("rate loads");
        assert_eq!(rate.hourly_rate, HOURLY_RATE);
        assert_eq!(rate.busy_hours_left, BUSY_HOURS_LEFT);

        let history = history(&conn, 24).expect("history loads");
        assert_eq!(history.len(), SNAPSHOTS.len());
        assert!(history[0].timestamp < history[history.len() - 1].timestamp);
    }
}
