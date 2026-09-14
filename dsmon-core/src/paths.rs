//! Filesystem locations for configuration and state.
//!
//! The paths deliberately match the ones the previous Rust implementations
//! used, so an existing installation keeps reading its own configuration and
//! history after upgrading.

use std::path::PathBuf;

/// Directory name used under the XDG base directories on Linux.
#[cfg(unix)]
const APP_DIR: &str = "deepseek-balance-monitor";

/// Directory name used under `%APPDATA%` on Windows.
#[cfg(windows)]
const APP_DIR: &str = "DeepSeek Balance Monitor";

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

/// Configuration directory: `%APPDATA%\DeepSeek Balance Monitor` on Windows,
/// `$XDG_CONFIG_HOME/deepseek-balance-monitor` on Linux.
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
/// XDG split and uses `$XDG_STATE_HOME/deepseek-balance-monitor`.
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

pub fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

pub fn log_file() -> PathBuf {
    state_dir().join("app.log")
}

pub fn history_db_file() -> PathBuf {
    state_dir().join("balance_history.db")
}

pub fn history_db_marker_file() -> PathBuf {
    state_dir().join(".balance_history.db.initialized")
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

    #[test]
    fn config_file_sits_in_the_config_directory() {
        assert_eq!(config_file(), config_dir().join("config.json"));
    }

    #[test]
    fn state_files_sit_in_the_state_directory() {
        assert_eq!(log_file(), state_dir().join("app.log"));
        assert_eq!(history_db_file(), state_dir().join("balance_history.db"));
        assert_eq!(secret_key_file(), state_dir().join(".secure_settings.key"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_keeps_state_next_to_config() {
        assert_eq!(state_dir(), config_dir());
    }
}
