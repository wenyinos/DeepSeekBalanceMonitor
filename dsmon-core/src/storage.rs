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

/// The six parameters, in order: platform, currency, cutoff, total, topped,
/// granted.
const DEDUP_SQL: &str = "SELECT EXISTS(
    SELECT 1 FROM balance_history
    WHERE platform = ?1
      AND currency = ?2
      AND timestamp >= ?3
      AND ABS(total - ?4) < 0.000001
      AND ABS(topped - ?5) < 0.000001
      AND ABS(granted - ?6) < 0.000001
    LIMIT 1
)";

/// Two readings closer than this count as the same one.
const DEDUP_SECONDS: i64 = 120;

/// Subscription allowances move slowly, so identical readings are skipped for
/// longer than balance ones.
const SUBSCRIPTION_DEDUP_SECONDS: i64 = 600;

/// Provider keys used in the `subscription_history` table.
pub const PROVIDER_OPENCODE_GO: &str = "opencode_go";
pub const PROVIDER_COMMAND_CODE: &str = "command_code";

/// Secret names used in the `secure_settings` table: one per platform, named
/// after its catalog key, so adding a platform needs no new constant.
pub const KEY_DEEPSEEK: &str = "deepseek";
pub const KEY_OPENCODE_GO: &str = "opencode_go";
pub const KEY_COMMAND_CODE: &str = "command_code";

static DATABASE_RECREATED: AtomicBool = AtomicBool::new(false);

/// Opens the history database, creating and migrating it as needed.
///
/// One connection at a time does the creating and the migrating. Both are
/// look-then-do statements — read the columns, add the one that is missing —
/// and two connections doing that at once collide twice over: both try to add
/// the same column, so one is told it is already there, and one takes the
/// write lock while the other holds it, which SQLite answers with "database is
/// locked". An upgrade is exactly when both connections arrive together: the
/// polling thread is started before the interface reads its keys, so on the
/// first start after 2.1.2 the interface's own lookup lost that race, read no
/// key at all, and asked the user to enter one. Every later call finds the
/// schema in place and only reads.
pub fn open_db() -> Result<Connection, String> {
    static MIGRATION: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _in_turn = MIGRATION.lock().unwrap_or_else(|error| error.into_inner());

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
            platform TEXT NOT NULL DEFAULT 'deepseek',
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
    conn.execute(
        "CREATE TABLE IF NOT EXISTS subscription_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            provider TEXT NOT NULL,
            used REAL NOT NULL,
            cap REAL NOT NULL,
            window TEXT NOT NULL DEFAULT 'monthly'
        )",
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

    // Every table above is made with the columns it carries now; these add the
    // ones a database from an earlier build is missing. They come after the
    // tables, and before the indexes: the `window` index names a column an
    // older database does not have yet.
    //
    // Rows already present belong to DeepSeek, which was the only provider then.
    add_column_if_missing(
        &conn,
        "balance_history",
        "platform",
        "platform TEXT NOT NULL DEFAULT 'deepseek'",
    )?;
    add_column_if_missing(
        &conn,
        "balance_history",
        "service_status",
        "service_status TEXT NOT NULL DEFAULT 'unknown'",
    )?;
    // Rows already present are monthly ones: that window was the only one this
    // build recorded, so the default says exactly what they are.
    add_column_if_missing(
        &conn,
        "subscription_history",
        "window",
        "window TEXT NOT NULL DEFAULT 'monthly'",
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_subscription_history_provider_timestamp
            ON subscription_history (provider, timestamp)",
        [],
    )
    .map_err(|error| error.to_string())?;
    // The window is part of what a query asks for now, so the index that
    // answers it carries the window too.
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_subscription_history_window
            ON subscription_history (provider, window, timestamp)",
        [],
    )
    .map_err(|error| error.to_string())?;

    mark_initialized().map_err(|error| error.to_string())?;
    Ok(conn)
}

/// Adds a column that databases made before it existed do not carry.
///
/// The look and the alter are two statements, so two connections can both find
/// the column missing and both add it: one succeeds, the other is told the
/// column is already there. That is the state the loser was after, so it is not
/// an error here. The lock in [`open_db`] keeps this process's own connections
/// from racing; another implementation sharing the database is what this is
/// left for.
fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|error| error.to_string())?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| error.to_string())?;
        for existing in columns {
            if existing.map_err(|error| error.to_string())? == column {
                return Ok(());
            }
        }
    }
    match conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {definition}"), []) {
        Ok(_) => Ok(()),
        Err(error) if error.to_string().contains("duplicate column name") => Ok(()),
        Err(error) => Err(error.to_string()),
    }
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
pub fn save_balance_history(
    platform: &str,
    balances: &Balances,
    service_status: &str,
) -> Result<(), String> {
    let mut conn = open_db()?;
    let timestamp = time::now();
    let dedup_cutoff = time::format_local(Local::now() - ChronoDuration::seconds(DEDUP_SECONDS));
    let tx = conn.transaction().map_err(|error| error.to_string())?;

    for (currency, balance) in balances {
        let duplicate: i64 = tx
            .query_row(
                DEDUP_SQL,
                params![
                    platform,
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
            "INSERT INTO balance_history (platform, timestamp, currency, total, topped, granted, service_status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                platform,
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
pub fn recent_balance_history(
    platform: &str,
    days: u64,
    limit: usize,
) -> Result<Vec<HistoryRecord>, String> {
    let conn = open_db()?;
    let cutoff = time::format_local(Local::now() - ChronoDuration::days(days as i64));
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, currency, total, topped, granted, service_status FROM (
                SELECT timestamp, currency, total, topped, granted, service_status
                FROM balance_history
                WHERE platform = ?1 AND timestamp >= ?2
                ORDER BY timestamp DESC
                LIMIT ?3
             ) ORDER BY timestamp ASC",
        )
        .map_err(|error| error.to_string())?;
    collect(stmt.query_map(params![platform, cutoff, limit], record_from_row))
}

/// Records within `days`, optionally filtered to one currency, oldest first.
pub fn history_records(
    platform: &str,
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
                 WHERE platform = ?1 AND timestamp >= ?2 AND currency = ?3 ORDER BY timestamp ASC LIMIT ?4",
            )
            .map_err(|error| error.to_string())?,
        None => conn
            .prepare(
                "SELECT timestamp, currency, total, topped, granted, service_status FROM balance_history \
                 WHERE platform = ?1 AND timestamp >= ?2 ORDER BY timestamp ASC LIMIT ?3",
            )
            .map_err(|error| error.to_string())?,
    };

    match currency {
        Some(currency) => {
            collect(stmt.query_map(params![platform, cutoff, currency, limit], record_from_row))
        }
        None => collect(stmt.query_map(params![platform, cutoff, limit], record_from_row)),
    }
}

/// Currencies that appear in the last `days` of history.
pub fn history_currencies(platform: &str, days: u64) -> Result<Vec<String>, String> {
    let conn = open_db()?;
    let cutoff = time::format_local(Local::now() - ChronoDuration::days(days as i64));
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT currency FROM balance_history WHERE platform = ?1 AND timestamp >= ?2 ORDER BY currency",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![platform, cutoff], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    let mut currencies = Vec::new();
    for row in rows {
        currencies.push(row.map_err(|error| error.to_string())?);
    }
    Ok(currencies)
}

/// What a manual cleanup removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cleared {
    pub balance_rows: usize,
    pub subscription_rows: usize,
    /// Bytes the file gave back, which is what a delete on its own would not.
    pub reclaimed: u64,
}

/// Bytes the database occupies, the write-ahead log included.
///
/// The log is part of what sits on disk, so reporting the main file alone
/// would understate what a growing history costs.
pub fn database_size() -> u64 {
    let base = paths::history_db_file();
    ["", "-wal"]
        .iter()
        .filter_map(|suffix| {
            let mut path = base.clone().into_os_string();
            path.push(suffix);
            std::fs::metadata(path).ok().map(|meta| meta.len())
        })
        .sum()
}

/// Renders a byte count the way a settings page wants to read it.
pub fn format_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;

    let bytes = bytes as f64;
    if bytes >= MIB {
        format!("{:.1} MB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.0} KB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

/// The two statements a cleanup runs. Constants so a test can run them against
/// a throwaway database instead of the one in use.
const CLEAR_BALANCE_SQL: &str = "DELETE FROM balance_history WHERE timestamp < ?1";
const CLEAR_SUBSCRIPTION_SQL: &str = "DELETE FROM subscription_history WHERE timestamp < ?1";

/// Drops every record older than `days` and compacts the file.
///
/// Each poll already prunes at the retention setting, but a `DELETE` only frees
/// pages *inside* the file, so its size on disk stays. This is the pass that
/// hands the space back.
pub fn clear_older_than(days: u64) -> Result<Cleared, String> {
    let before = database_size();
    let conn = open_db()?;
    let cutoff = time::format_local(Local::now() - ChronoDuration::days(days as i64));

    let balance_rows = conn
        .execute(CLEAR_BALANCE_SQL, params![cutoff])
        .map_err(|error| error.to_string())?;
    let subscription_rows = conn
        .execute(CLEAR_SUBSCRIPTION_SQL, params![cutoff])
        .map_err(|error| error.to_string())?;

    // Compaction cannot run inside a transaction, and the log has to be folded
    // back into the file first for its pages to be reclaimed too.
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")
        .map_err(|error| error.to_string())?;
    drop(conn);

    Ok(Cleared {
        balance_rows,
        subscription_rows,
        reclaimed: before.saturating_sub(database_size()),
    })
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

/// Appends a monthly-allowance reading taken now.
pub fn save_subscription_usage(
    provider: &str,
    window: &str,
    used: f64,
    cap: f64,
) -> Result<(), String> {
    let timestamp = time::now();
    save_subscription_usage_at(provider, window, used, cap, &timestamp)
}

/// Appends a reading with an explicit timestamp.
///
/// Separate from [`save_subscription_usage`] so imports and demo data can fill in
/// the past. Identical readings inside the dedup window are still skipped, so an
/// idle allowance does not fill the table.
pub fn save_subscription_usage_at(
    provider: &str,
    window: &str,
    used: f64,
    cap: f64,
    timestamp: &str,
) -> Result<(), String> {
    let conn = open_db()?;
    let cutoff =
        time::format_local(Local::now() - ChronoDuration::seconds(SUBSCRIPTION_DEDUP_SECONDS));

    let duplicate: i64 = conn
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM subscription_history
                WHERE provider = ?1
                  AND window = ?2
                  AND timestamp >= ?3
                  AND ABS(used - ?4) < 0.000001
                  AND ABS(cap - ?5) < 0.000001
                LIMIT 1
            )",
            params![provider, window, &cutoff, used, cap],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if duplicate != 0 {
        return Ok(());
    }

    conn.execute(
        "INSERT INTO subscription_history (timestamp, provider, used, cap, window)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![&timestamp, provider, used, cap, window],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Readings for one provider within `days`, oldest first.
pub fn subscription_usage_history(
    provider: &str,
    window: &str,
    days: u64,
) -> Result<Vec<SubscriptionPoint>, String> {
    let conn = open_db()?;
    let cutoff = time::format_local(Local::now() - ChronoDuration::days(days as i64));
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, used, cap FROM subscription_history
             WHERE provider = ?1 AND window = ?2 AND timestamp >= ?3
             ORDER BY timestamp ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![provider, window, cutoff], |row| {
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

/// Secret names used by the earlier builds, mapped to the platform they belong to.
const LEGACY_SECRET_NAMES: [(&str, &str); 3] = [
    ("api_key", KEY_DEEPSEEK),
    ("opencode_go_api_key", KEY_OPENCODE_GO),
    ("command_code_api_key", KEY_COMMAND_CODE),
];

/// What an import from the earlier build's database brought across.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportSummary {
    pub secrets: usize,
    /// Keys that were left behind because they cannot be read here — the
    /// earlier Windows build protected its own with DPAPI.
    pub unreadable_secrets: usize,
    pub history_records: usize,
}

/// Copies the earlier build's data into this build's database.
///
/// The earlier database is read through a copy of it, so it is never written
/// to — the CLI, Python and Windows builds keep working beside this one. Safe
/// to run more than once: secrets already present are left alone, and history
/// rows are matched on their timestamp and currency so a second run adds
/// nothing.
pub fn import_from_legacy() -> Result<ImportSummary, String> {
    // Only the earlier Rust build is imported from, and it kept its database
    // where this one keeps its state. The path is named in the error so that a
    // failure says where it looked.
    let legacy_path = paths::legacy_db_file();
    if !legacy_path.exists() {
        return Err(format!("no earlier database at {}", legacy_path.display()));
    }

    let target = open_db()?;

    // The earlier database runs in WAL mode, and a read-only open of one needs
    // its log files beside it — which are gone as soon as that version closes,
    // so reading it read-only fails on Windows with "unable to open database
    // file". A copy can be opened normally, and the original is left alone
    // whatever the other version happens to be doing.
    let copy = LegacyCopy::take(&legacy_path)?;
    let source = Connection::open(copy.database()).map_err(|error| error.to_string())?;

    let mut summary = ImportSummary::default();

    // A database that was never given a key has no secure_settings table — the
    // earlier build created it when the first key was stored — so each table is
    // read when it is there rather than being required.
    let secrets_present = table_exists(&source, "secure_settings")?;
    let history_present = table_exists(&source, "balance_history")?;
    if !secrets_present && !history_present {
        return Err(format!(
            "{} is not an earlier database: it holds neither table",
            legacy_path.display()
        ));
    }

    // Secrets first: they are what the user would have to re-enter by hand.
    if secrets_present {
        let mut stmt = source
            .prepare("SELECT key, value, updated_at FROM secure_settings")
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?;

        for row in rows {
            let (key, value, updated_at) = row.map_err(|error| error.to_string())?;

            // A key this build cannot read is worse than no key: it would look
            // configured and fail every poll. Counted instead, so the interface
            // can say that it has to be entered again.
            if crate::crypto::decrypt(&value).is_err() {
                summary.unreadable_secrets += 1;
                continue;
            }

            let inserted = target
                .execute(
                    "INSERT OR IGNORE INTO secure_settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
                    params![platform_name(&key), value, updated_at],
                )
                .map_err(|error| error.to_string())?;
            summary.secrets += inserted;
        }
    }

    // Then the balance history.
    if history_present {
        let mut stmt = source
            .prepare(
                "SELECT timestamp, currency, total, topped, granted, service_status FROM balance_history",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|error| error.to_string())?;

        for row in rows {
            let (timestamp, currency, total, topped, granted, status) =
                row.map_err(|error| error.to_string())?;
            let inserted = target
                .execute(
                    "INSERT INTO balance_history (timestamp, currency, total, topped, granted, service_status)
                     SELECT ?1, ?2, ?3, ?4, ?5, ?6
                     WHERE NOT EXISTS (
                         SELECT 1 FROM balance_history WHERE timestamp = ?1 AND currency = ?2
                     )",
                    params![timestamp, currency, total, topped, granted, status],
                )
                .map_err(|error| error.to_string())?;
            summary.history_records += inserted;
        }
    }

    let _ = log_line(&format!(
        "imported from {}: {} keys, {} unreadable, {} history rows",
        legacy_path.display(),
        summary.secrets,
        summary.unreadable_secrets,
        summary.history_records
    ));

    Ok(summary)
}

/// Whether the database holds a table of that name.
fn table_exists(conn: &Connection, name: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        params![name],
        |row| row.get::<_, i64>(0),
    )
    .map(|found| found == 1)
    .map_err(|error| error.to_string())
}

/// A throwaway copy of the earlier database, removed when it goes out of scope.
struct LegacyCopy {
    directory: std::path::PathBuf,
}

impl LegacyCopy {
    /// Copies the database, and whichever of its log files are beside it.
    fn take(path: &std::path::Path) -> Result<Self, String> {
        let directory = std::env::temp_dir().join(format!("dsmon-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;

        let database = directory.join("legacy.db");
        std::fs::copy(path, &database).map_err(|error| error.to_string())?;

        for suffix in ["-wal", "-shm"] {
            let mut side = path.as_os_str().to_owned();
            side.push(suffix);
            let side = std::path::PathBuf::from(side);
            if side.exists() {
                let mut target = database.clone().into_os_string();
                target.push(suffix);
                let _ = std::fs::copy(&side, std::path::PathBuf::from(target));
            }
        }

        Ok(Self { directory })
    }

    fn database(&self) -> std::path::PathBuf {
        self.directory.join("legacy.db")
    }
}

impl Drop for LegacyCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// Maps a legacy secret name onto the platform it belongs to.
fn platform_name(key: &str) -> &str {
    LEGACY_SECRET_NAMES
        .iter()
        .find(|(old, _)| *old == key)
        .map(|(_, new)| *new)
        .unwrap_or(key)
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

    /// Runs the dedup statement against an in-memory table. A placeholder that
    /// does not match its parameters fails right here, rather than silently
    /// breaking every history write at runtime.
    #[test]
    fn the_dedup_statement_matches_its_parameters() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite opens");
        conn.execute_batch(
            "CREATE TABLE balance_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                platform TEXT NOT NULL DEFAULT 'deepseek',
                timestamp TEXT NOT NULL,
                currency TEXT NOT NULL,
                total REAL NOT NULL,
                topped REAL NOT NULL,
                granted REAL NOT NULL,
                service_status TEXT NOT NULL DEFAULT 'unknown'
            );",
        )
        .expect("schema creates");

        let found: i64 = conn
            .query_row(
                DEDUP_SQL,
                params!["deepseek", "CNY", "2026-01-01 00:00:00", 1.0, 2.0, 3.0],
                |row| row.get(0),
            )
            .expect("statement accepts six parameters");
        assert_eq!(found, 0, "an empty table holds no duplicate");

        conn.execute(
            "INSERT INTO balance_history (platform, timestamp, currency, total, topped, granted, service_status)
             VALUES ('deepseek', '2026-01-01 00:00:00', 'CNY', 1.0, 2.0, 3.0, 'unknown')",
            [],
        )
        .expect("row inserts");

        let found: i64 = conn
            .query_row(
                DEDUP_SQL,
                params!["deepseek", "CNY", "2026-01-01 00:00:00", 1.0, 2.0, 3.0],
                |row| row.get(0),
            )
            .expect("statement still runs");
        assert_eq!(found, 1, "the row just written counts as a duplicate");
    }

    /// Both cleanup statements, run against a throwaway database: the cutoff
    /// keeps what is inside the window and drops what is outside it.
    #[test]
    fn a_cleanup_keeps_the_window_and_drops_the_rest() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite opens");
        conn.execute_batch(
            "CREATE TABLE balance_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                platform TEXT NOT NULL DEFAULT 'deepseek',
                timestamp TEXT NOT NULL,
                currency TEXT NOT NULL,
                total REAL NOT NULL,
                topped REAL NOT NULL,
                granted REAL NOT NULL,
                service_status TEXT NOT NULL DEFAULT 'unknown'
            );
            CREATE TABLE subscription_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                provider TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                used REAL NOT NULL,
                cap REAL NOT NULL
            );
            INSERT INTO balance_history (timestamp, currency, total, topped, granted)
                VALUES ('2026-01-01 00:00:00', 'CNY', 1.0, 1.0, 0.0),
                       ('2026-09-14 00:00:00', 'CNY', 2.0, 2.0, 0.0);
            INSERT INTO subscription_history (provider, timestamp, used, cap)
                VALUES ('opencode_go', '2026-01-01 00:00:00', 1.0, 100.0),
                       ('opencode_go', '2026-09-14 00:00:00', 2.0, 100.0);",
        )
        .expect("schema and rows are created");

        let cutoff = "2026-08-15 00:00:00";
        assert_eq!(
            conn.execute(CLEAR_BALANCE_SQL, params![cutoff]).unwrap(),
            1,
            "one balance row falls outside the window"
        );
        assert_eq!(
            conn.execute(CLEAR_SUBSCRIPTION_SQL, params![cutoff])
                .unwrap(),
            1,
            "one subscription row falls outside the window"
        );

        let kept: String = conn
            .query_row("SELECT MIN(timestamp) FROM balance_history", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(kept, "2026-09-14 00:00:00");
    }

    #[test]
    fn sizes_read_the_way_a_settings_page_wants() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
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

    /// Starts from no database at all, whatever another test left behind.
    fn remove_history_database() {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", paths::history_db_file().display()));
        }
    }

    fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
        conn.prepare(&format!("PRAGMA table_info({table})"))
            .expect("the table is there")
            .query_map([], |row| row.get::<_, String>(1))
            .expect("the columns list")
            .filter_map(Result::ok)
            .any(|name| name == column)
    }

    /// Opening a database that does not exist yet must not alter a table that
    /// is not there. The `window` migration ran before the table it belongs to
    /// was created, so a fresh installation could not open its own database.
    #[test]
    fn opens_a_database_that_does_not_exist_yet() {
        let _in_turn = crate::test_support::database_in_turn();
        crate::test_support::state_in_a_scratch_directory();
        remove_history_database();

        let conn = open_db().expect("a fresh database opens");
        assert!(
            has_column(&conn, "subscription_history", "window"),
            "the table is made with the column the migrations add"
        );
    }

    /// The look-then-alter is two statements, so two connections can both find
    /// the column missing and both add it. Being told it is already there is
    /// the state the loser of that race wanted, not a failure: on the first
    /// start after the 2.1.2 upgrade the interface's own key lookup lost
    /// exactly that race, read no key at all, and asked for one.
    #[test]
    fn two_connections_migrating_at_once_both_succeed() {
        let _in_turn = crate::test_support::database_in_turn();
        crate::test_support::state_in_a_scratch_directory();
        remove_history_database();

        // A database from before these columns existed.
        paths::ensure_dir(&paths::state_dir()).expect("the state directory");
        let old = Connection::open(paths::history_db_file()).expect("an older database");
        old.execute_batch(
            "CREATE TABLE balance_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                currency TEXT NOT NULL,
                total REAL NOT NULL,
                topped REAL NOT NULL,
                granted REAL NOT NULL
            );
            CREATE TABLE subscription_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                provider TEXT NOT NULL,
                used REAL NOT NULL,
                cap REAL NOT NULL
            );",
        )
        .expect("the schema of an earlier build");
        drop(old);

        // Both go at once, the way the polling thread and the interface do.
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    open_db().map(|_| ())
                })
            })
            .collect();

        for handle in handles {
            handle
                .join()
                .expect("the thread finishes")
                .expect("both connections migrate the same database");
        }
    }
}
