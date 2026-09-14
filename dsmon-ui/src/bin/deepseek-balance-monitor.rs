//! The Windows executable, `deepseek-balance-monitor.exe`.
//!
//! The interface and the backend are shared with the Linux build, so there is
//! nothing to do here but start them. The subsystem matters: without it a
//! console window comes up behind the application.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    if let Err(error) = dsmon_ui::run() {
        let _ = dsmon_core::storage::log_line(&format!("The window could not be opened: {error}"));
        std::process::exit(1);
    }
}
