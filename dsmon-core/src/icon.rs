//! Rasterises the tray icon: a rounded square carrying the balance figure.
//!
//! The digits come from an embedded font so that both platforms produce the
//! same bitmap regardless of what is installed on the host.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};

/// Digital display face used for the figure drawn on the icon.
const FONT_BYTES: &[u8] = include_bytes!("../../assets/font/ShareTech-Regular.ttf");

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
/// Anything above two characters collapses to `OK`, matching the behaviour of
/// the previous Windows build.
pub fn icon_label(amount: f64) -> String {
    if !amount.is_finite() || amount < 0.0 {
        return "--".to_string();
    }
    let rounded = amount.round();
    if rounded > 99.0 {
        "OK".to_string()
    } else {
        format!("{rounded:.0}")
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_label_collapses_long_figures() {
        assert_eq!(icon_label(0.0), "0");
        assert_eq!(icon_label(12.4), "12");
        assert_eq!(icon_label(99.0), "99");
        assert_eq!(icon_label(100.0), "OK");
        assert_eq!(icon_label(f64::NAN), "--");
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
