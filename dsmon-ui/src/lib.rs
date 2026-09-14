//! Shared interface for DeepSeek Balance Monitor.
//!
//! Both platform binaries are thin wrappers around [`run`], so the window,
//! the widget and the tray behave identically on Windows and Linux.

mod app;
pub mod fonts;
pub mod theme;
pub mod tray;

pub use app::run;
