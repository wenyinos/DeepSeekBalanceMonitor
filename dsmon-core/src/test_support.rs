//! Helpers for the tests, which the shipped build does not carry.
//!
//! One thing has to be arranged before any test runs. The tests that encrypt
//! and decrypt reach the key file through `paths::secret_key_file()`, and that
//! call creates the file — and the directory holding it — when it is not there.
//! A test run must not write into the real directories of the machine it runs
//! on: the key it leaves behind is not only clutter, it also takes the place of
//! the key a database needs, since the move out of the earlier build's
//! directory skips a file that is already in place.

use std::sync::Once;

/// Points the state and configuration directories at a scratch directory, once
/// per test process.
///
/// Every test that looks at a path calls this first, so that the redirection
/// cannot land in the middle of one of their comparisons: `Once` settles it
/// before any of them goes on.
pub fn state_in_a_scratch_directory() {
    static ONCE: Once = Once::new();

    ONCE.call_once(|| {
        let scratch = std::env::temp_dir().join(format!("dsmon-tests-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&scratch);

        // Linux reads the XDG variables, Windows reads its own; HOME is set as
        // well, since it is what the XDG ones fall back to.
        std::env::set_var("HOME", scratch.join("home"));
        std::env::set_var("XDG_STATE_HOME", scratch.join("state"));
        std::env::set_var("XDG_CONFIG_HOME", scratch.join("config"));
        std::env::set_var("APPDATA", scratch.join("appdata"));

        // And then the key file, here and now, while one thread is running this
        // and no other test has started looking.
        //
        // Every test that encrypts creates that file when it is missing, and
        // several of them do it at the same time: on a Windows runner the one
        // that finds the file another thread is still writing answers "access
        // is denied", and the release build fails on a test rather than on the
        // code (v2.1.0, 2026-09-15). Made once here, every test after it only
        // ever reads it. The directories are made too, for the same reason:
        // `create_dir_all` racing against itself answers "the system cannot
        // find the path specified" for a directory that is about to exist.
        for directory in [crate::paths::state_dir(), crate::paths::config_dir()] {
            let _ = std::fs::create_dir_all(&directory);
        }
        let _ = crate::crypto::encrypt("warm-up");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The point of the helper: nothing a test writes lands in the directories
    /// of whoever is running the suite.
    #[test]
    fn the_directories_a_test_writes_to_are_scratch_ones() {
        state_in_a_scratch_directory();

        let temporary = std::env::temp_dir();
        for directory in [crate::paths::state_dir(), crate::paths::config_dir()] {
            assert!(
                directory.starts_with(&temporary),
                "{directory:?} is not under {temporary:?}"
            );
        }
    }
}
