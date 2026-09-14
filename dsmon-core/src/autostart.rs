//! Starting with the session.
//!
//! Windows keeps this in the per-user `Run` key, Linux in the autostart
//! directory the desktop reads (`~/.config/autostart`). Both are the user's own
//! settings, so no service manager is involved and nothing needs privileges.

use crate::paths;

/// What the entry runs: the application itself, asking for a start that stays
/// out of the way.
const ARGUMENT: &str = "--minimized";

/// Makes the system's start-up entry agree with `enabled`.
///
/// This is a *state*, not an event: the setting lives in the configuration, and
/// the entry the session reads lives outside it, so the two are reconciled
/// whenever the application starts as well as when the setting changes. A
/// setting carried over from another build, or an executable that has moved,
/// therefore still starts with the session. Writing only happens when the entry
/// is missing or says something else.
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let path = paths::autostart_file();
    if enabled {
        let command = command_line()?;
        if entry_is_current(&path, &command) {
            return Ok(());
        }
        install(&path, &command)
    } else {
        remove(&path)
    }
}

/// Whether the entry is already the one this build would write.
#[cfg(not(windows))]
fn entry_is_current(path: &std::path::Path, command: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|written| written == entry_text(command))
        .unwrap_or(false)
}

#[cfg(windows)]
fn entry_is_current(_path: &std::path::Path, command: &str) -> bool {
    entry().as_deref() == Some(command)
}

/// Whether the entry this build would write is in place.
pub fn is_enabled() -> bool {
    #[cfg(windows)]
    {
        entry().is_some()
    }

    #[cfg(not(windows))]
    {
        paths::autostart_file().is_file()
    }
}

/// The executable and its argument, quoted for a command line.
fn command_line() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    Ok(format!("\"{}\" {ARGUMENT}", exe.display()))
}

/// The text of the desktop entry, which is also what tells whether the file on
/// disk is still the right one.
#[cfg(not(windows))]
fn entry_text(command: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={}\n\
         Comment=Shows the account balance in the tray\n\
         Exec={command}\n\
         Terminal=false\n\
         Hidden=false\n\
         X-GNOME-Autostart-enabled=true\n",
        crate::APP_NAME
    )
}

#[cfg(not(windows))]
fn install(path: &std::path::Path, command: &str) -> Result<(), String> {
    if let Some(directory) = path.parent() {
        paths::ensure_dir(directory).map_err(|error| error.to_string())?;
    }

    std::fs::write(path, entry_text(command)).map_err(|error| error.to_string())?;

    // Some sessions only launch an autostart entry that is marked executable,
    // and the check costs nothing.
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn remove(path: &std::path::Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(windows)]
mod windows_impl {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
        RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
        REG_SZ,
    };

    /// Where the per-user start-up entries live.
    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "DeepSeek Balance Monitor";

    pub fn install(_path: &std::path::Path, command: &str) -> Result<(), String> {
        let mut key: HKEY = std::ptr::null_mut();
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                wide(RUN_KEY).as_ptr(),
                0,
                std::ptr::null_mut(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                std::ptr::null_mut(),
                &mut key,
                std::ptr::null_mut(),
            )
        };
        if status != ERROR_SUCCESS {
            return Err(format!("the Run key could not be opened ({status})"));
        }

        let value = wide(command);
        let bytes: Vec<u8> = value.iter().flat_map(|unit| unit.to_le_bytes()).collect();
        let status = unsafe {
            RegSetValueExW(
                key,
                wide(VALUE_NAME).as_ptr(),
                0,
                REG_SZ,
                bytes.as_ptr(),
                bytes.len() as u32,
            )
        };
        unsafe { RegCloseKey(key) };

        if status != ERROR_SUCCESS {
            return Err(format!("the entry could not be written ({status})"));
        }
        Ok(())
    }

    pub fn remove(_path: &std::path::Path) -> Result<(), String> {
        let mut key: HKEY = std::ptr::null_mut();
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                wide(RUN_KEY).as_ptr(),
                0,
                KEY_SET_VALUE,
                &mut key,
            )
        };
        if status != ERROR_SUCCESS {
            // Nothing to remove when the key itself is not there.
            return Ok(());
        }

        unsafe { RegDeleteValueW(key, wide(VALUE_NAME).as_ptr()) };
        unsafe { RegCloseKey(key) };
        Ok(())
    }

    /// The command line stored for this application, if there is one.
    pub fn entry() -> Option<String> {
        let mut key: HKEY = std::ptr::null_mut();
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                wide(RUN_KEY).as_ptr(),
                0,
                KEY_READ,
                &mut key,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }

        let mut size = 0u32;
        let name = wide(VALUE_NAME);
        let status = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS && status != ERROR_FILE_NOT_FOUND {
            unsafe { RegCloseKey(key) };
            return None;
        }

        let mut buffer = vec![0u8; size as usize];
        let status = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                buffer.as_mut_ptr(),
                &mut size,
            )
        };
        unsafe { RegCloseKey(key) };
        if status != ERROR_SUCCESS {
            return None;
        }

        let units: Vec<u16> = buffer
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|unit| *unit != 0)
            .collect();
        Some(String::from_utf16_lossy(&units))
    }

    /// A NUL-terminated UTF-16 copy, for the wide-string APIs.
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(windows)]
use windows_impl::{entry, install, remove};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_asks_for_a_quiet_start() {
        let command = command_line().expect("the running executable has a path");
        assert!(command.ends_with(ARGUMENT), "{command}");
        assert!(
            command.starts_with('"'),
            "a path may hold spaces: {command}"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn the_desktop_entry_says_what_the_desktop_needs() {
        let path = std::env::temp_dir().join("dsmon-autostart-test.desktop");
        let _ = std::fs::remove_file(&path);

        install(&path, "\"/opt/dsmon\" --minimized").expect("the entry is written");
        let written = std::fs::read_to_string(&path).expect("the entry is readable");

        assert!(written.starts_with("[Desktop Entry]"), "{written}");
        assert!(written.contains("Type=Application"));
        assert!(written.contains("Name=DeepSeek Balance Monitor"));
        assert!(
            written.contains("Exec=\"/opt/dsmon\" --minimized"),
            "{written}"
        );
        assert!(written.contains("Terminal=false"));

        remove(&path).expect("the entry is removed");
        assert!(!path.exists());
        remove(&path).expect("removing it twice is not an error");
    }

    /// Reconciling means the file is only rewritten when it says something
    /// else, so an executable that has moved is noticed and a correct entry is
    /// left alone.
    #[cfg(not(windows))]
    #[test]
    fn an_entry_that_is_already_right_is_left_alone() {
        let path = std::env::temp_dir().join("dsmon-autostart-current.desktop");
        let _ = std::fs::remove_file(&path);

        let command = "\"/opt/dsmon2\" --minimized";
        assert!(!entry_is_current(&path, command), "nothing is there yet");

        install(&path, command).expect("the entry is written");
        assert!(entry_is_current(&path, command), "the entry is current");
        assert!(
            !entry_is_current(&path, "\"/elsewhere/dsmon2\" --minimized"),
            "an executable that has moved is not current"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .expect("the entry exists")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755, "an autostart entry is executable");
        }

        remove(&path).expect("the entry is removed");
        assert!(!entry_is_current(&path, command), "and then it is gone");
    }

    #[test]
    fn the_desktop_entry_lives_under_the_autostart_directory() {
        let path = paths::autostart_file();
        assert_eq!(
            path.file_name().unwrap(),
            "deepseek-balance-monitor.desktop"
        );
        assert_eq!(
            path.parent().unwrap().file_name().unwrap(),
            "autostart",
            "{path:?}"
        );
    }
}
