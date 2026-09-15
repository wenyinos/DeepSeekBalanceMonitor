//! Moving this build's files out of the directory it used to share.
//!
//! This build kept its configuration and state beside the earlier build's, in
//! one directory: it wrote `dsmon.db` and its key file there, and — the part
//! that mattered — its own `config.json` and `app.log`, the two names the
//! earlier build uses as well, so each was writing over the other's. It now has
//! a directory of its own, and what it left behind is moved there once, on the
//! first start, so nothing has to be entered again.
//!
//! The earlier build's own files are not touched: its database stays where its
//! import expects to find it.

use std::path::Path;

use crate::paths;

/// The files this build keeps, in the state directory.
const STATE_FILES: [&str; 3] = ["dsmon.db", ".dsmon.db.initialized", ".secure_settings.key"];

/// The file this build keeps in the configuration directory, which the earlier
/// build also keeps one of under the same name.
const CONFIG_FILE: &str = "config.json";

/// Fields only this build writes, enough to tell its configuration from the
/// earlier build's when both are called `config.json`.
const ONLY_THIS_BUILD: [&str; 3] = [
    "billing_day_command_code",
    "tray_hint_shown",
    "widget_enabled",
];

/// Moves this build's files to its own directories, if they are still in the
/// shared one. Does nothing when they are not, or when they are already here.
pub fn earlier_files() -> Result<(), String> {
    let mut moved = Vec::new();

    for name in STATE_FILES {
        let from = paths::earlier_state_dir().join(name);
        if move_file(&from, &paths::state_dir().join(name))? {
            moved.push(name);
        }
    }

    let from = paths::earlier_config_dir().join(CONFIG_FILE);
    if is_this_builds_configuration(&from) && move_file(&from, &paths::config_file())? {
        moved.push(CONFIG_FILE);
    }

    if !moved.is_empty() {
        let _ = crate::storage::log_line(&format!(
            "Moved out of the shared directory: {}",
            moved.join(", ")
        ));
    }
    Ok(())
}

/// Moves `from` onto `to` when the first is there and the second is not, and
/// says whether it did.
///
/// A file that is already in place is left alone: the move happens once, and a
/// start that was interrupted halfway must not undo the half that was done.
fn move_file(from: &Path, to: &Path) -> Result<bool, String> {
    if !from.is_file() || to.exists() {
        return Ok(false);
    }

    if let Some(directory) = to.parent() {
        paths::ensure_dir(directory).map_err(|error| error.to_string())?;
    }

    if std::fs::rename(from, to).is_ok() {
        return Ok(true);
    }

    // A rename cannot cross a filesystem boundary, which is what a state
    // directory of its own may well be on.
    std::fs::copy(from, to).map_err(|error| format!("{}: {error}", from.display()))?;
    std::fs::remove_file(from).map_err(|error| error.to_string())?;
    Ok(true)
}

/// Whether the file is this build's configuration rather than the earlier
/// build's, which keeps its own settings under the same name.
fn is_this_builds_configuration(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    ONLY_THIS_BUILD
        .iter()
        .any(|field| value.get(field).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!("dsmon-adopt-{name}"));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch directory is made");
        directory
    }

    #[test]
    fn a_file_is_moved_to_where_it_belongs() {
        let directory = scratch("move");
        let from = directory.join("from.db");
        let to = directory.join("sub").join("to.db");
        std::fs::write(&from, b"contents").expect("the file is written");

        assert!(move_file(&from, &to).expect("the move works"));
        assert!(!from.exists(), "the original is gone");
        assert_eq!(
            std::fs::read(&to).expect("the file is there"),
            b"contents",
            "and it arrived whole"
        );
    }

    #[test]
    fn a_file_that_should_not_move_is_left_alone() {
        let directory = scratch("leave");
        let from = directory.join("from.db");
        let to = directory.join("to.db");

        assert!(
            !move_file(&from, &to).expect("nothing to do"),
            "nothing there"
        );
        assert!(!to.exists());

        std::fs::write(&from, b"old").expect("the file is written");
        std::fs::write(&to, b"new").expect("the other is written");
        assert!(
            !move_file(&from, &to).expect("nothing to do"),
            "the file that is already here wins"
        );
        assert_eq!(
            std::fs::read(&to).expect("the file is there"),
            b"new",
            "and is not written over"
        );
    }

    /// The earlier build keeps its settings in a `config.json` too, so only the
    /// one this build wrote is carried over.
    #[test]
    fn only_this_builds_configuration_is_recognised() {
        let directory = scratch("config");

        let ours = directory.join("ours.json");
        std::fs::write(&ours, br#"{"interval_minutes":10,"tray_hint_shown":false}"#)
            .expect("the file is written");
        assert!(is_this_builds_configuration(&ours));

        let theirs = directory.join("theirs.json");
        std::fs::write(&theirs, br#"{"interval_minutes":10,"api_keys":[]}"#)
            .expect("the file is written");
        assert!(
            !is_this_builds_configuration(&theirs),
            "a configuration without this build's own fields is left for the other build"
        );

        assert!(!is_this_builds_configuration(
            &directory.join("absent.json")
        ));
    }
}
