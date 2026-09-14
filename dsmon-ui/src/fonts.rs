//! Font installation.
//!
//! The interface and the Chinese text come from the host system, so the app
//! matches the desktop's own typography. Only the digits on the tray icon are
//! embedded, because that bitmap has to look the same on every platform.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// Family name carrying the embedded digital face. Balance figures opt in with
/// `FontFamily::Name(DIGITS_FAMILY.into())`.
pub const DIGITS_FAMILY: &str = "Digits";

const DIGITS_FONT: &[u8] = include_bytes!("../../assets/font/ShareTech-Regular.ttf");

/// Registers the embedded digits face and, when available, a system font that
/// covers CJK. Call once during start-up.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "share-tech".to_owned(),
        Arc::new(FontData::from_static(DIGITS_FONT)),
    );
    fonts.families.insert(
        FontFamily::Name(DIGITS_FAMILY.into()),
        vec!["share-tech".to_owned()],
    );

    if let Some((bytes, index)) = load_system_font() {
        fonts.font_data.insert(
            "system-ui".to_owned(),
            Arc::new(FontData {
                font: bytes.into(),
                index,
                tweak: Default::default(),
            }),
        );
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, "system-ui".to_owned());
        }
    }

    ctx.set_fonts(fonts);
}

/// Reads the desktop's default sans-serif face.
///
/// Linux asks fontconfig, which resolves whatever the user installed. Windows
/// walks the well-known CJK families in the system font directory. A failure
/// here is survivable: the embedded text fonts still render Latin characters,
/// and the packaged builds depend on a CJK font being present.
#[cfg(target_os = "linux")]
fn load_system_font() -> Option<(Vec<u8>, u32)> {
    let output = std::process::Command::new("fc-match")
        .args(["-f", "%{file}\n%{index}\n", "sans-serif:lang=zh-cn"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8(output.stdout).ok()?;
    let mut lines = text.lines();
    let path = lines.next()?.trim().to_owned();
    let index = lines
        .next()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(0);
    if path.is_empty() {
        return None;
    }

    let bytes = std::fs::read(path).ok()?;
    Some((bytes, index))
}

#[cfg(windows)]
fn load_system_font() -> Option<(Vec<u8>, u32)> {
    let mut fonts_dir = std::env::var_os("WINDIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("C:\\Windows"));
    fonts_dir.push("Fonts");

    for name in ["msyh.ttc", "msyh.ttf", "simhei.ttf", "simsun.ttc"] {
        if let Ok(bytes) = std::fs::read(fonts_dir.join(name)) {
            return Some((bytes, 0));
        }
    }
    None
}

#[cfg(not(any(target_os = "linux", windows)))]
fn load_system_font() -> Option<(Vec<u8>, u32)> {
    None
}
