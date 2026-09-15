//! The desktop widget: `dsmon2-widget`, and `dsmon2-widget.exe` on Windows.
//!
//! It draws what the application reports over the local interface, and does
//! nothing else: no polling of its own, no database, no keys. The contract is
//! written down in `docs/INTERFACES.md`.
//!
//! Released as its own package so it can be installed — and upgraded — on its
//! own, while still versioning with the application it reads from.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    if let Err(error) = dsmon_ui::widget::run() {
        let _ = dsmon_core::storage::log_line(&format!(
            "The widget window could not be opened: {error}"
        ));
        eprintln!("DeepSeek Balance Monitor widget: {error}");
        std::process::exit(1);
    }
}
