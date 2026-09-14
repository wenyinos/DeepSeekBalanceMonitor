//! Tray integration, one implementation per platform.
//!
//! Linux speaks StatusNotifierItem over D-Bus (ksni), Windows uses the Win32
//! notification area (tray-icon). Everything above this module is shared.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::{spawn, TrayHandle};
#[cfg(target_os = "windows")]
pub use windows::{spawn, TrayHandle};
