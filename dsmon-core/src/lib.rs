//! Platform-independent core for DeepSeek Balance Monitor.
//!
//! This crate holds everything that does not depend on a GUI toolkit: the data
//! model, configuration, cryptographic storage, history, the per-platform API
//! clients and the tray icon rasteriser. Both the Linux and the Windows
//! binaries are thin entry points over `dsmon-ui`, which in turn builds on this
//! crate.

pub mod icon;
pub mod paths;

/// Application version, taken from the workspace package metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Display name shared by the window title, tray tooltip and notifications.
pub const APP_NAME: &str = "DeepSeek Balance Monitor";
