//! Shared interface for DeepSeek Balance Monitor.
//!
//! Both platform binaries are thin wrappers around [`run`], so the window and
//! the tray behave identically on Windows and Linux.

mod app;
pub mod fonts;
pub mod i18n;
mod instance;
pub mod notify;
pub mod theme;
pub mod tray;
pub mod views;

pub use app::run;
