//! The Linux executable, `dsmon`.
//!
//! The interface and the backend are shared with the Windows build, so there is
//! nothing to do here but start them.

fn main() {
    if let Err(error) = dsmon_ui::run() {
        let _ = dsmon_core::storage::log_line(&format!("The window could not be opened: {error}"));
        eprintln!("{}: {error}", dsmon_core::APP_NAME);
        std::process::exit(1);
    }
}
