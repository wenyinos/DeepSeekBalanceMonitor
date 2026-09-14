//! The executable: `dsmon2`, and `dsmon2.exe` on Windows.
//!
//! The interface and the backend are shared by both platforms, so there is
//! nothing to do here but start them.
//!
//! The name is deliberately not one the 1.x build used. That version installs
//! `dsmon` and `deepseek-balance-monitor.exe`, and it is meant to keep working
//! beside this one: two programs with the same name would be a nuisance to tell
//! apart, in a process list as much as on disk.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    if let Err(error) = dsmon_ui::run() {
        let _ = dsmon_core::storage::log_line(&format!("The window could not be opened: {error}"));
        eprintln!("{}: {error}", dsmon_core::APP_NAME);
        std::process::exit(1);
    }
}
