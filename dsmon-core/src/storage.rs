//! SQLite storage: the balance history table and the encrypted settings table.
//!
//! The schema matches what the previous builds created, so an existing
//! installation keeps its records and API keys after upgrading.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Local};
use rusqlite::{params, Connection, Error as SqlError};

use crate::crypto;
use crate::model::{Balances, HistoryRecord, SubscriptionPoint};
use crate::paths;
use crate::time;

/// Two readings closer than this count as the same one.
const DEDUP_SECONDS: i64 = 120;

/// Subscription allowances move slowly, so identical readings are skipped for
/// longer than balance ones.
const SUBSCRIPTION_DEDUP_SECONDS: i64 = 600;

/// Provider keys used in the `subscription_history` table.
pub const PROVIDER_OPENCODE_GO: &str = "opencode_go";
pub const PROVIDER_COMMAND_CODE: &str = "command_code";

/// Secret names used in the `secure_settings` table.
pub const KEY_DEEPSEEK: &str = "api_key";
pub const KEY_OPENCODE_GO: &str = "opencode_go_api_key";
pub const KEY_COMMAND_CODE: &str = "command_code_api_key";

static DATABASE_RECREATED: AtomicBool = AtomicBool::new(false);

/// Opens the history database, creating and migrating it as needed.
pub fn open_db() -> Result<Connection, String> {
    paths::ensure_dir(&paths::state_dir()).map_err(|error| error.to_string())?;
    let path = paths::history_db_file();
    note_recreated_database(&path);

    let conn = Connection::open(&path).map_err(|error| error.to_string())?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(|error| error.to_string())?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|error| error.to_string())?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS balance_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            currency TEXT NOT NULL,
            total REAL NOT NULL,
            topped REAL NOT NULL,
            granted REAL NOT NULL,
            service_status TEXT NOT NULL DEFAULT 'unknown'
        )",
        [],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_balance_history_timestamp ON balance_history (timestamp)",
        [],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_balance_history_currency_timestamp ON balance_history (currency, timestamp)",
        [],
    )
    .map_err(|error| error.to_string())?;
    ensure_service_status_column(&conn)?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS subscription_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            provider TEXT NOT NULL,
            used REAL NOT NULL,
            cap REAL NOT NULL
        )",
        [],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_subscription_history_provider_timestamp
            ON subscription_history (provider, timestamp)",
        [],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS secure_settings (
            key TEXT PRIMARY KEY,
            value BLOB NOT NULL,
            updated_at TEXT NOT NULL
        )",
        [],
    )
    .map_err(|error| error.to_string())?;

    mark_initialized().map_err(|error| error.to_string())?;
    Ok(conn)
}

/// Adds the `service_status` column to databases created before it existed.
fn ensure_service_status_column(conn: &Connection) -> Result<(), String> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(balance_history)")
        .map_err(|error| error.to_string())?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?;
    for column in columns {
        if column.map_err(|error| error.to_string())? == "service_status" {
            return Ok(());
        }
    }
    conn.execute(
        "ALTER TABLE balance_history ADD COLUMN service_status TEXT NOT NULL DEFAULT 'unknown'",
        [],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Flags a database that disappeared while its marker file survived.
fn note_recreated_database(path: &std::path::Path) {
    let marker = paths::history_db_marker_file();
    if marker.exists() && !path.exists() {
        DATABASE_RECREATED.store(true, Ordering::SeqCst);
    }
}

/// Whether the database had to be recreated since the last check. Clears the
/// flag so the notice is shown once.
pub fn take_recreated_notice() -> bool {
    DATABASE_RECREATED.swap(false, Ordering::SeqCst)
}

fn mark_initialized() -> std::io::Result<()> {
    let marker = paths::history_db_marker_file();
    if !marker.exists() {
        std::fs::write(marker, "1\n")?;
    }
    Ok(())
}

/// Appends the current balances, skipping readings identical to a recent one.
pub fn save_balance_history(balances: &Balances, service_status: &str) -> Result<(), String> {
    let mut conn = open_db()?;
    let timestamp = time::now();
    let dedup_cutoff = time::format_local(Local::now() - ChronoDuration::seconds(DEDUP_SECONDS));
    let tx = conn.transaction().map_err(|error| error.to_string())?;

    for (currency, balance) in balances {
        let duplicate: i64 = tx
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM balance_history
                    WHERE currency = ?1
                      AND timestamp >= ?2
                      AND ABS(total - ?3) < 0.000001
                      AND ABS(topped - ?4) < 0.000001
                      AND ABS(granted - ?5) < 0.000001
                    LIMIT 1
                )",
                params![
                    currency.as_str(),
                    &dedup_cutoff,
                    balance.total_balance,
                    balance.topped_up_balance,
                    balance.granted_balance
                ],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if duplicate != 0 {
            continue;
        }

        tx.execute(
            "INSERT INTO balance_history (timestamp, currency, total, topped, granted, service_status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &timestamp,
                currency.as_str(),
                balance.total_balance,
                balance.topped_up_balance,
                balance.granted_balance,
                service_status
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    tx.commit().map_err(|error| error.to_string())
}

/// The most recent `limit` records within `days`, oldest first.
pub fn recent_balance_history(days: u64, limit: usize) -> Result<Vec<HistoryRecord>, String> {
    let conn = open_db()?;
    let cutoff = time::format_local(Local::now() - ChronoDuration::days(days as i64));
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, currency, total, topped, granted, service_status FROM (
                SELECT timestamp, currency, total, topped, granted, service_status
                FROM balance_history
                WHERE timestamp >= ?1
                ORDER BY timestamp DESC
                LIMIT ?2
             ) ORDER BY timestamp ASC",
        )
        .map_err(|error| error.to_string())?;
    collect(stmt.query_map(params![cutoff, limit], record_from_row))
}

/// Records within `days`, optionally filtered to one currency, oldest first.
pub fn history_records(
    days: u64,
    currency: Option<&str>,
    limit: usize,
) -> Result<Vec<HistoryRecord>, String> {
    let conn = open_db()?;
    let cutoff = time::format_local(Local::now() - ChronoDuration::days(days as i64));
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);

    let mut stmt = match currency {
        Some(_) => conn
            .prepare(
                "SELECT timestamp, currency, total, topped, granted, service_status FROM balance_history \
                 WHERE timestamp >= ?1 AND currency = ?2 ORDER BY timestamp ASC LIMIT ?3",
            )
            .map_err(|error| error.to_string())?,
        None => conn
            .prepare(
                "SELECT timestamp, currency, total, topped, granted, service_status FROM balance_history \
                 WHERE timestamp >= ?1 ORDER BY timestamp ASC LIMIT ?2",
            )
            .map_err(|error| error.to_string())?,
    };

    match currency {
        Some(currency) => {
            collect(stmt.query_map(params![cutoff, currency, limit], record_from_row))
        }
        None => collect(stmt.query_map(params![cutoff, limit], record_from_row)),
    }
}

/// Currencies that appear in the last `days` of history.
pub fn history_currencies(days: u64) -> Result<Vec<String>, String> {
    let conn = open_db()?;
    let cutoff = time::format_local(Local::now() - ChronoDuration::days(days as i64));
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT currency FROM balance_history WHERE timestamp >= ?1 ORDER BY currency",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![cutoff], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    let mut currencies = Vec::new();
    for row in rows {
        currencies.push(row.map_err(|error| error.to_string())?);
    }
    Ok(currencies)
}

/// Drops records older than the retention window.
pub fn prune_balance_history(retention_days: u64) -> Result<(), String> {
    let conn = open_db()?;
    let cutoff = time::format_local(Local::now() - ChronoDuration::days(retention_days as i64));
    conn.execute(
        "DELETE FROM balance_history WHERE timestamp < ?1",
        params![cutoff],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryRecord> {
    Ok(HistoryRecord {
        timestamp: row.get(0)?,
        currency: row.get(1)?,
        total: row.get(2)?,
        topped: row.get(3)?,
        granted: row.get(4)?,
        service_status: row.get(5)?,
    })
}

fn collect(
    rows: Result<
        rusqlite::MappedRows<'_, impl Fn(&rusqlite::Row<'_>) -> rusqlite::Result<HistoryRecord>>,
        SqlError,
    >,
) -> Result<Vec<HistoryRecord>, String> {
    let mut records = Vec::new();
    for row in rows.map_err(|error| error.to_string())? {
        records.push(row.map_err(|error| error.to_string())?);
    }
    Ok(records)
}

/// Appends a monthly-allowance reading, skipping ones identical to a recent
/// entry so an idle allowance does not fill the table.
pub fn save_subscription_usage(provider: &str, used: f64, cap: f64) -> Result<(), String> {
    let conn = open_db()?;
    let timestamp = time::now();
    let cutoff =
        time::format_local(Local::now() - ChronoDuration::seconds(SUBSCRIPTION_DEDUP_SECONDS));

    let duplicate: i64 = conn
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM subscription_history
                WHERE provider = ?1
                  AND timestamp >= ?2
                  AND ABS(used - ?3) < 0.000001
                  AND ABS(cap - ?4) < 0.000001
                LIMIT 1
            )",
            params![provider, &cutoff, used, cap],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if duplicate != 0 {
        return Ok(());
    }

    conn.execute(
        "INSERT INTO subscription_history (timestamp, provider, used, cap) VALUES (?1, ?2, ?3, ?4)",
        params![&timestamp, provider, used, cap],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Readings for one provider within `days`, oldest first.
pub fn subscription_usage_history(
    provider: &str,
    days: u64,
) -> Result<Vec<SubscriptionPoint>, String> {
    let conn = open_db()?;
    let cutoff = time::format_local(Local::now() - ChronoDuration::days(days as i64));
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, used, cap FROM subscription_history
             WHERE provider = ?1 AND timestamp >= ?2
             ORDER BY timestamp ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![provider, cutoff], |row| {
            Ok(SubscriptionPoint {
                timestamp: row.get(0)?,
                used: row.get(1)?,
                cap: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?;

    let mut points = Vec::new();
    for row in rows {
        points.push(row.map_err(|error| error.to_string())?);
    }
    Ok(points)
}

/// Drops subscription readings older than the retention window.
pub fn prune_subscription_history(retention_days: u64) -> Result<(), String> {
    let conn = open_db()?;
    let cutoff = time::format_local(Local::now() - ChronoDuration::days(retention_days as i64));
    conn.execute(
        "DELETE FROM subscription_history WHERE timestamp < ?1",
        params![cutoff],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Reads and decrypts a stored secret. Empty values read as absent.
pub fn read_secret(key: &str) -> Result<Option<String>, String> {
    let conn = open_db()?;
    let encrypted = match conn.query_row(
        "SELECT value FROM secure_settings WHERE key = ?1",
        params![key],
        |row| row.get::<_, Vec<u8>>(0),
    ) {
        Ok(value) => value,
        Err(SqlError::QueryReturnedNoRows) => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let value = crypto::decrypt(&encrypted)?;
    Ok((!value.trim().is_empty()).then_some(value))
}

/// Encrypts and stores a secret.
pub fn store_secret(key: &str, value: &str) -> Result<(), String> {
    let encrypted = crypto::encrypt(value.trim())?;
    let conn = open_db()?;
    conn.execute(
        "INSERT OR REPLACE INTO secure_settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
        params![key, encrypted, time::now()],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Removes a stored secret.
pub fn delete_secret(key: &str) -> Result<(), String> {
    let conn = open_db()?;
    conn.execute("DELETE FROM secure_settings WHERE key = ?1", params![key])
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// What the text typed into a key field means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyInput<'a> {
    /// Blank: leave whatever is stored alone.
    Keep,
    /// A single `0`: remove the stored value.
    Clear,
    /// Anything else: store this value.
    Set(&'a str),
}

/// Classifies a key field's contents.
///
/// The fields start empty and never show what is stored, so blank has to mean
/// "no change". `0` is the one value reserved for removal, and the settings page
/// spells that out next to the fields.
pub fn classify_key_input(value: &str) -> KeyInput<'_> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        KeyInput::Keep
    } else if trimmed == "0" {
        KeyInput::Clear
    } else {
        KeyInput::Set(trimmed)
    }
}

/// Appends a line to `app.log`.
pub fn log_line(message: &str) -> std::io::Result<()> {
    use std::io::Write;
    paths::ensure_dir(&paths::state_dir())?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths::log_file())?;
    writeln!(file, "[{}] {}", time::now(), message)
}

/// Drops log lines older than the retention window.
pub fn prune_logs(retention_days: u64) -> std::io::Result<()> {
    paths::ensure_dir(&paths::state_dir())?;
    let path = paths::log_file();
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };

    let cutoff = Local::now().naive_local() - ChronoDuration::days(retention_days as i64);
    let mut changed = false;
    let mut retained = String::new();
    for line in content.lines() {
        if keep_log_line(line, cutoff) {
            retained.push_str(line);
            retained.push('\n');
        } else {
            changed = true;
        }
    }
    if changed {
        std::fs::write(&path, retained)?;
    }
    Ok(())
}

fn keep_log_line(line: &str, cutoff: chrono::NaiveDateTime) -> bool {
    let Some(timestamp) = line.strip_prefix('[').and_then(|rest| rest.get(..19)) else {
        return true;
    };
    chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S")
        .map(|logged_at| logged_at >= cutoff)
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_key_field_input() {
        assert_eq!(classify_key_input(""), KeyInput::Keep);
        assert_eq!(classify_key_input("   "), KeyInput::Keep);
        assert_eq!(classify_key_input("0"), KeyInput::Clear);
        assert_eq!(classify_key_input(" 0 "), KeyInput::Clear);
        assert_eq!(classify_key_input("sk-abc"), KeyInput::Set("sk-abc"));
        assert_eq!(classify_key_input(" sk-abc "), KeyInput::Set("sk-abc"));
    }

    #[test]
    fn keeps_recent_log_lines_only() {
        let cutoff =
            chrono::NaiveDateTime::parse_from_str("2026-01-02 00:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap();
        assert!(keep_log_line("[2026-01-02 12:00:00] later", cutoff));
        assert!(keep_log_line("[2026-01-01 23:59:59] earlier", cutoff) == false);
        assert!(keep_log_line("no timestamp here", cutoff));
    }
}
