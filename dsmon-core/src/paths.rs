//! Filesystem locations for configuration and state.
//!
//! This build keeps everything in a directory of its own, named after its
//! executable. The earlier builds keep theirs under a name of their own, and
//! that separation is the point: this build used to share their directory, where
//! its `config.json` and log were written over theirs and theirs over its.

use std::path::PathBuf;

/// Directory this build keeps its files under.
const APP_DIR: &str = "dsmon2";

/// Directory the earlier builds use, for finding their data — and for moving
/// this build's files out of it, once.
#[cfg(unix)]
const EARLIER_DIR: &str = "deepseek-balance-monitor";

/// Directory name used under `%APPDATA%` by the earlier Windows build.
#[cfg(windows)]
const EARLIER_DIR: &str = "DeepSeek Balance Monitor";

#[cfg(windows)]
fn base_dir(env_var: &str, _fallback: &str) -> PathBuf {
    std::env::var_os(env_var)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(unix)]
fn base_dir(env_var: &str, fallback: &str) -> PathBuf {
    if let Some(value) = std::env::var_os(env_var) {
        if !value.is_empty() {
            return PathBuf::from(value);
        }
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(fallback)
}

/// Configuration directory: `%APPDATA%\dsmon2` on Windows,
/// `$XDG_CONFIG_HOME/dsmon2` on Linux.
pub fn config_dir() -> PathBuf {
    #[cfg(windows)]
    {
        base_dir("APPDATA", "").join(APP_DIR)
    }
    #[cfg(unix)]
    {
        base_dir("XDG_CONFIG_HOME", ".config").join(APP_DIR)
    }
}

/// State directory holding the log, the history database and the secret key.
///
/// Windows keeps everything in the configuration directory; Linux follows the
/// XDG split and uses `$XDG_STATE_HOME/dsmon2`.
pub fn state_dir() -> PathBuf {
    #[cfg(windows)]
    {
        config_dir()
    }
    #[cfg(unix)]
    {
        base_dir("XDG_STATE_HOME", ".local/state").join(APP_DIR)
    }
}

/// The directory the earlier builds keep their configuration in.
///
/// Nothing is written here; it is where this build's own files may still be
/// sitting, waiting to be moved to [`config_dir`].
pub fn earlier_config_dir() -> PathBuf {
    #[cfg(windows)]
    {
        base_dir("APPDATA", "").join(EARLIER_DIR)
    }
    #[cfg(unix)]
    {
        base_dir("XDG_CONFIG_HOME", ".config").join(EARLIER_DIR)
    }
}

/// The directory the earlier builds keep their state in.
pub fn earlier_state_dir() -> PathBuf {
    #[cfg(windows)]
    {
        earlier_config_dir()
    }
    #[cfg(unix)]
    {
        base_dir("XDG_STATE_HOME", ".local/state").join(EARLIER_DIR)
    }
}

/// The entry a desktop reads at login, for starting with the session.
///
/// Always under the user's own autostart directory: the XDG one on Linux, and
/// nothing at all on Windows, where the registry holds the same idea.
pub fn autostart_file() -> PathBuf {
    base_dir("XDG_CONFIG_HOME", ".config")
        .join("autostart")
        .join("deepseek-balance-monitor.desktop")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

pub fn log_file() -> PathBuf {
    state_dir().join("app.log")
}

/// The database this build owns.
pub fn history_db_file() -> PathBuf {
    state_dir().join("dsmon.db")
}

/// The database the earlier builds keep, in their own directory.
///
/// Opened read-only, and only by an explicit import: this build never writes
/// there, so the other versions keep working alongside it.
pub fn legacy_db_file() -> PathBuf {
    earlier_state_dir().join("balance_history.db")
}

pub fn history_db_marker_file() -> PathBuf {
    state_dir().join(".dsmon.db.initialized")
}

pub fn secret_key_file() -> PathBuf {
    state_dir().join(".secure_settings.key")
}

/// Creates a directory (and its parents) if it does not exist yet.
pub fn ensure_dir(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The directories are read out of the environment on every call, and the
    // crypto tests move them to a scratch directory. Asking for that
    // redirection first is what keeps it from landing between the two sides of
    // a comparison below.
    use crate::test_support::state_in_a_scratch_directory;

    #[test]
    fn config_file_sits_in_the_config_directory() {
        state_in_a_scratch_directory();

        assert_eq!(config_file(), config_dir().join("config.json"));
    }

    #[test]
    fn state_files_sit_in_the_state_directory() {
        state_in_a_scratch_directory();

        assert_eq!(log_file(), state_dir().join("app.log"));
        assert_eq!(secret_key_file(), state_dir().join(".secure_settings.key"));
    }

    #[test]
    fn this_build_keeps_its_own_database() {
        state_in_a_scratch_directory();

        // The earlier builds keep `balance_history.db`; this build must not
        // write there.
        assert_eq!(history_db_file(), state_dir().join("dsmon.db"));
        assert_ne!(
            history_db_file(),
            state_dir().join("balance_history.db"),
            "the earlier builds' database is off limits"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_keeps_state_next_to_config() {
        state_in_a_scratch_directory();

        assert_eq!(state_dir(), config_dir());
    }

    /// The two versions must not share a directory: this build used to write
    /// its own `config.json` and log over the earlier build's, in the place the
    /// earlier build reads them from.
    #[test]
    fn this_build_keeps_out_of_the_earlier_builds_directory() {
        state_in_a_scratch_directory();

        assert_ne!(config_dir(), earlier_config_dir());
        assert_ne!(state_dir(), earlier_state_dir());
        assert_eq!(
            legacy_db_file(),
            earlier_state_dir().join("balance_history.db"),
            "the import reads the database where the earlier build keeps it"
        );
    }
}
