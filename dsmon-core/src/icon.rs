//! Rasterises the tray icon: a rounded square carrying the balance figure.
//!
//! The digits come from an embedded font so that both platforms produce the
//! same bitmap regardless of what is installed on the host.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};

/// Digital display face used for the figure drawn on the icon.
const FONT_BYTES: &[u8] = include_bytes!("../../assets/font/ShareTech-Regular.ttf");

/// The application's own artwork, the one a desktop shows in a task bar, a
/// window list or an about box.
const APP_ICON_BYTES: &[u8] = include_bytes!("../../assets/app.ico");

/// An icon ready to be handed to a tray library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayIcon {
    pub width: u32,
    pub height: u32,
    /// RGBA, 8 bits per channel, row-major, top-left origin.
    pub rgba: Vec<u8>,
}

/// Everything needed to draw one icon.
#[derive(Debug, Clone)]
pub struct IconSpec<'a> {
    /// Figure drawn on the icon. Callers cap the length, see [`icon_label`].
    pub label: &'a str,
    pub background: [u8; 3],
    pub foreground: [u8; 3],
    /// Icon is square; this is the edge length in pixels.
    pub size: u32,
}

/// Shortens a figure so it still renders legibly at tray size.
///
/// Below ten the tenths are kept — that is where a balance is worth watching
/// closely — and above it only the whole part is shown, which is all the room
/// an icon has. Either way the figure is truncated rather than rounded: the
/// icon answers "is there enough left to keep going", and rounding 1.55 up to
/// 1.6 would claim more than the account holds. Above two digits it collapses
/// to `OK`.
pub fn icon_label(amount: f64) -> String {
    if !amount.is_finite() || amount < 0.0 {
        return "--".to_string();
    }

    if amount < 10.0 {
        let tenths = (amount * 10.0).trunc() / 10.0;
        return format!("{tenths:.1}");
    }

    let whole = amount.trunc();
    if whole > 99.0 {
        "OK".to_string()
    } else {
        format!("{whole:.0}")
    }
}

/// Decodes the application icon at `size`.
///
/// The file carries several sizes; the largest is scaled when none of them is
/// the one asked for.
pub fn app_icon(size: u32) -> Option<TrayIcon> {
    let decoded = image::load_from_memory(APP_ICON_BYTES).ok()?;
    let scaled = if decoded.width() == size && decoded.height() == size {
        decoded.into_rgba8()
    } else {
        decoded
            .resize_exact(size, size, image::imageops::FilterType::Lanczos3)
            .into_rgba8()
    };

    Some(TrayIcon {
        width: size,
        height: size,
        rgba: scaled.into_raw(),
    })
}

/// Draws the icon described by `spec`.
pub fn render(spec: &IconSpec<'_>) -> TrayIcon {
    let size = spec.size.max(8);
    let mut buffer = vec![0u8; (size * size * 4) as usize];

    let radius = (size as f32 * 0.22).max(2.0);
    let background = [
        spec.background[0],
        spec.background[1],
        spec.background[2],
        255,
    ];

    for y in 0..size {
        for x in 0..size {
            let coverage =
                rounded_rect_coverage(x as f32 + 0.5, y as f32 + 0.5, size as f32, radius);
            if coverage > 0.0 {
                let index = ((y * size + x) * 4) as usize;
                buffer[index] = background[0];
                buffer[index + 1] = background[1];
                buffer[index + 2] = background[2];
                buffer[index + 3] = (255.0 * coverage).round() as u8;
            }
        }
    }

    draw_label(&mut buffer, size, spec.label, spec.foreground);

    TrayIcon {
        width: size,
        height: size,
        rgba: buffer,
    }
}

/// Coverage of a rounded square at `(px, py)`, 0.0 to 1.0.
fn rounded_rect_coverage(px: f32, py: f32, size: f32, radius: f32) -> f32 {
    let inset = size - radius;
    let dx = if px < radius {
        radius - px
    } else if px > inset {
        px - inset
    } else {
        0.0
    };
    let dy = if py < radius {
        radius - py
    } else if py > inset {
        py - inset
    } else {
        0.0
    };
    if dx == 0.0 || dy == 0.0 {
        return 1.0;
    }
    let distance = (dx * dx + dy * dy).sqrt();
    (radius - distance + 0.5).clamp(0.0, 1.0)
}

/// Blends `label` into the centre of the buffer.
fn draw_label(buffer: &mut [u8], size: u32, label: &str, foreground: [u8; 3]) {
    if label.is_empty() {
        return;
    }
    let Ok(font) = FontRef::try_from_slice(FONT_BYTES) else {
        return;
    };

    let scale = PxScale::from(label_scale(label, size));
    let scaled = font.as_scaled(scale);

    let mut width = 0.0f32;
    let mut previous = None;
    for character in label.chars() {
        let glyph_id = scaled.glyph_id(character);
        width += scaled.kern(previous.unwrap_or(glyph_id), glyph_id);
        width += scaled.h_advance(glyph_id);
        previous = Some(glyph_id);
    }

    let origin_x = (size as f32 - width) / 2.0;
    let ascent = scaled.ascent();
    let descent = scaled.descent();
    let origin_y = (size as f32 - (ascent - descent)) / 2.0 + ascent;

    let mut pen_x = origin_x;
    let mut previous = None;
    for character in label.chars() {
        let glyph_id = scaled.glyph_id(character);
        if let Some(previous_id) = previous {
            pen_x += scaled.kern(previous_id, glyph_id);
        }
        let glyph = glyph_id.with_scale_and_position(scale, ab_glyph::point(pen_x, origin_y));
        pen_x += scaled.h_advance(glyph_id);
        previous = Some(glyph_id);

        let Some(outlined) = font.outline_glyph(glyph) else {
            continue;
        };
        let bounds = outlined.px_bounds();
        outlined.draw(|x, y, coverage| {
            let px = bounds.min.x as i32 + x as i32;
            let py = bounds.min.y as i32 + y as i32;
            if px < 0 || py < 0 || px >= size as i32 || py >= size as i32 {
                return;
            }
            let index = ((py as u32 * size + px as u32) * 4) as usize;
            if buffer[index + 3] == 0 {
                return;
            }
            let alpha = coverage.clamp(0.0, 1.0);
            for channel in 0..3 {
                let base = buffer[index + channel] as f32;
                let top = foreground[channel] as f32;
                buffer[index + channel] = (base + (top - base) * alpha).round() as u8;
            }
        });
    }
}

/// Picks a font size that keeps the figure inside the icon.
fn label_scale(label: &str, size: u32) -> f32 {
    let size = size as f32;
    match label.chars().count() {
        0 | 1 => size * 0.62,
        2 => size * 0.54,
        3 => size * 0.40,
        _ => size * 0.30,
    }
}

/// Which reading the icon reflects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ok,
    Low,
    Degraded,
    NoData,
}

/// The tray icon's colour scheme: one of the six presets, or custom hex values.
///
/// The presets are the ones the previous build shipped, so the icon keeps the
/// look users already recognise.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IconTheme {
    pub style: String,
    pub custom: std::collections::BTreeMap<String, String>,
}

impl IconTheme {
    /// Fill colour for a state.
    pub fn state_color(&self, state: State) -> [u8; 3] {
        if self.style == "custom" {
            if let Some(color) = self.custom_color(state) {
                return color;
            }
        }
        preset_color(&self.style, state)
    }

    fn custom_color(&self, state: State) -> Option<[u8; 3]> {
        let key = match state {
            State::Ok => "ok",
            State::Low => "low",
            State::Degraded => "degraded",
            State::NoData => "nodata",
        };
        let value = self.custom.get(key)?.trim().trim_start_matches('#');
        if value.len() != 6 {
            return None;
        }
        let rgb = u32::from_str_radix(value, 16).ok()?;
        Some([
            ((rgb >> 16) & 0xff) as u8,
            ((rgb >> 8) & 0xff) as u8,
            (rgb & 0xff) as u8,
        ])
    }
}

/// The six presets, as `(ok, low, degraded, nodata)`.
fn preset_color(style: &str, state: State) -> [u8; 3] {
    let (ok, low, degraded, nodata) = match style {
        "contrast" => (
            [0x2d, 0x80, 0x74],
            [0xd4, 0x34, 0x2e],
            [0x8b, 0x69, 0x14],
            [0x55, 0x55, 0x55],
        ),
        "bright" => (
            [0xc8, 0xeb, 0xe6],
            [0xf5, 0xd2, 0xcd],
            [0xeb, 0xdc, 0xcd],
            [0xd7, 0xd7, 0xdc],
        ),
        "dark_mode" => (
            [0x50, 0x9b, 0x94],
            [0xd7, 0x64, 0x5a],
            [0x9b, 0x8c, 0x73],
            [0x7d, 0x7d, 0x82],
        ),
        "mono" => (
            [0x55, 0x55, 0x55],
            [0x22, 0x22, 0x22],
            [0x77, 0x77, 0x77],
            [0x99, 0x99, 0x99],
        ),
        _ => (
            [0x3c, 0x69, 0x66],
            [0xb9, 0x46, 0x3c],
            [0x78, 0x69, 0x5a],
            [0x69, 0x69, 0x6e],
        ),
    };

    match state {
        State::Ok => ok,
        State::Low => low,
        State::Degraded => degraded,
        State::NoData => nodata,
    }
}

/// Black or white, whichever reads better on `fill`.
pub fn readable_on(fill: [u8; 3]) -> [u8; 3] {
    let luminance =
        0.299 * f64::from(fill[0]) + 0.587 * f64::from(fill[1]) + 0.114 * f64::from(fill[2]);
    if luminance > 170.0 {
        [0, 0, 0]
    } else {
        [255, 255, 255]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_covers_every_state() {
        let default_theme = IconTheme {
            style: "default".to_owned(),
            custom: Default::default(),
        };
        assert_eq!(default_theme.state_color(State::Ok), [0x3c, 0x69, 0x66]);
        assert_eq!(default_theme.state_color(State::Low), [0xb9, 0x46, 0x3c]);

        for style in [
            "default",
            "contrast",
            "bright",
            "dark_mode",
            "mono",
            "custom",
        ] {
            let theme = IconTheme {
                style: style.to_owned(),
                custom: Default::default(),
            };
            for state in [State::Ok, State::Low, State::Degraded, State::NoData] {
                let color = theme.state_color(state);
                assert_ne!(color, [0, 0, 0], "{style} {state:?} should be defined");
            }
        }
    }

    #[test]
    fn custom_style_prefers_the_user_colours() {
        let mut custom = std::collections::BTreeMap::new();
        custom.insert("ok".to_owned(), "#112233".to_owned());
        let theme = IconTheme {
            style: "custom".to_owned(),
            custom,
        };
        assert_eq!(theme.state_color(State::Ok), [0x11, 0x22, 0x33]);
        // A missing entry falls back to the default preset.
        assert_eq!(theme.state_color(State::Low), [0xb9, 0x46, 0x3c]);
    }

    #[test]
    fn readable_on_picks_the_contrasting_ink() {
        assert_eq!(readable_on([0x3c, 0x69, 0x66]), [255, 255, 255]);
        assert_eq!(readable_on([0xc8, 0xeb, 0xe6]), [0, 0, 0]);
    }

    #[test]
    fn icon_label_collapses_long_figures() {
        assert_eq!(icon_label(0.0), "0");
        assert_eq!(icon_label(12.4), "12");
        assert_eq!(icon_label(99.0), "99");
        assert_eq!(icon_label(100.0), "OK");
        assert_eq!(icon_label(f64::NAN), "--");
    }

    /// Below ten the icon keeps a decimal, which is where a balance is worth
    /// watching closely; above it the whole part is all that fits.
    #[test]
    fn icon_label_keeps_a_decimal_below_ten() {
        assert_eq!(icon_label(1.55), "1.5");
        assert_eq!(icon_label(0.99), "0.9");
        assert_eq!(icon_label(9.99), "9.9");
        assert_eq!(icon_label(9.0), "9.0");
        assert_eq!(icon_label(10.0), "10");
        assert_eq!(icon_label(12.4), "12");
    }

    /// A balance is never rounded up: 1.55 must not read as 2 or 1.6.
    #[test]
    fn icon_label_never_overstates_the_balance() {
        assert_eq!(icon_label(1.99), "1.9");
        assert_eq!(icon_label(99.9), "99");
        assert_eq!(icon_label(0.09), "0.0");
    }

    #[test]
    fn the_application_icon_decodes_at_the_size_asked_for() {
        for size in [32, 64, 256] {
            let icon = app_icon(size).expect("the bundled icon decodes");
            assert_eq!((icon.width, icon.height), (size, size));
            assert_eq!(icon.rgba.len(), (size * size * 4) as usize);

            // The mark is drawn on a transparent frame, and covers a good part
            // of it rather than being an empty bitmap.
            let painted = icon
                .rgba
                .chunks_exact(4)
                .filter(|pixel| pixel[3] > 0)
                .count();
            assert!(
                painted > (size * size / 10) as usize,
                "{size}px frame came out nearly empty: {painted} pixels painted"
            );
        }
    }

    #[test]
    fn render_produces_a_square_rgba_bitmap() {
        let icon = render(&IconSpec {
            label: "42",
            background: [0x36, 0x36, 0x36],
            foreground: [0x5e, 0xe0, 0xc8],
            size: 32,
        });
        assert_eq!(icon.width, 32);
        assert_eq!(icon.height, 32);
        assert_eq!(icon.rgba.len(), 32 * 32 * 4);

        // The corners stay transparent, the centre is painted.
        assert_eq!(icon.rgba[3], 0);
        let centre = ((16 * 32 + 16) * 4) as usize;
        assert_eq!(icon.rgba[centre + 3], 255);
    }
}
