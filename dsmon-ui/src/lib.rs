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
pub mod widget;

pub use app::run;

/// Which rendering backend this process should use.
///
/// Glow (OpenGL) is the default: on Windows the wgpu backend runs on D3D12,
/// whose device and driver working set costs each process about 300 MB even
/// when idle, while both programs are small panels. `DSMON_RENDERER=wgpu`
/// opts back into wgpu for machines where OpenGL is unavailable (remote
/// desktops, some virtual machines) — at that memory cost.
pub fn renderer() -> eframe::Renderer {
    match std::env::var("DSMON_RENDERER").ok().as_deref() {
        Some("wgpu") | Some("dx12") => eframe::Renderer::Wgpu,
        _ => eframe::Renderer::Glow,
    }
}
