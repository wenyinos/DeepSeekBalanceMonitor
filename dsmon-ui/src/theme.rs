//! Themes: six colour styles, each with a light and a dark variant.
//!
//! The styles come from the previous build, where they coloured the tray icon.
//! They now drive the whole interface, so picking "mono" or "contrast" changes
//! the window as well as the icon. Light and dark are chosen separately, which
//! gives twelve combinations.
//!
//! Text is black on light surfaces and white on dark ones; labels sitting on an
//! accent fill pick black or white from that fill's luminance, the same rule the
//! icon renderer has always used.

use std::collections::BTreeMap;

use egui::{Color32, CornerRadius, Shadow, Stroke, Theme, Visuals};

/// What the user picked: follow the desktop, or force one scheme.
pub use egui::ThemePreference as ThemeMode;

/// The colour styles offered in the settings page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Style {
    #[default]
    Default,
    Contrast,
    Bright,
    DarkMode,
    Mono,
    Custom,
}

impl Style {
    pub const ALL: [Style; 6] = [
        Style::Default,
        Style::Contrast,
        Style::Bright,
        Style::DarkMode,
        Style::Mono,
        Style::Custom,
    ];

    pub fn from_config(value: &str) -> Self {
        match value {
            "contrast" => Style::Contrast,
            "bright" => Style::Bright,
            "dark_mode" => Style::DarkMode,
            "mono" => Style::Mono,
            "custom" => Style::Custom,
            _ => Style::Default,
        }
    }

    pub fn as_config(self) -> &'static str {
        match self {
            Style::Default => "default",
            Style::Contrast => "contrast",
            Style::Bright => "bright",
            Style::DarkMode => "dark_mode",
            Style::Mono => "mono",
            Style::Custom => "custom",
        }
    }

    /// Key in the text table for this style's name.
    pub fn label_key(self) -> &'static str {
        match self {
            Style::Default => "theme_default",
            Style::Contrast => "theme_contrast",
            Style::Bright => "theme_bright",
            Style::DarkMode => "theme_dark_mode",
            Style::Mono => "theme_mono",
            Style::Custom => "theme_custom",
        }
    }

    /// Accent and status colours. `custom` reads the user's four hex values.
    fn accents(self, dark: bool, custom: &BTreeMap<String, String>) -> Accents {
        if self == Style::Custom {
            let fallback = Style::Default.accents(dark, custom);
            return Accents {
                accent: custom_color(custom, "ok").unwrap_or(fallback.accent),
                positive: custom_color(custom, "ok").unwrap_or(fallback.positive),
                warning: custom_color(custom, "degraded").unwrap_or(fallback.warning),
                destructive: custom_color(custom, "low").unwrap_or(fallback.destructive),
            };
        }

        match (self, dark) {
            (Style::Default, true) => Accents {
                accent: Color32::from_rgb(0x5e, 0xe0, 0xc8),
                positive: Color32::from_rgb(0x7e, 0xe0, 0xa8),
                warning: Color32::from_rgb(0xe8, 0xc0, 0x7a),
                destructive: Color32::from_rgb(0xf0, 0xa0, 0xa8),
            },
            (Style::Default, false) => Accents {
                accent: Color32::from_rgb(0x1a, 0x6a, 0x94),
                positive: Color32::from_rgb(0x2e, 0x7d, 0x32),
                warning: Color32::from_rgb(0x8a, 0x61, 0x00),
                destructive: Color32::from_rgb(0xb3, 0x26, 0x1e),
            },

            (Style::Contrast, true) => Accents {
                accent: Color32::from_rgb(0x3f, 0xd4, 0xbd),
                positive: Color32::from_rgb(0x4f, 0xe0, 0x8a),
                warning: Color32::from_rgb(0xf0, 0xb0, 0x2a),
                destructive: Color32::from_rgb(0xff, 0x6b, 0x5e),
            },
            (Style::Contrast, false) => Accents {
                accent: Color32::from_rgb(0x0f, 0x5f, 0x52),
                positive: Color32::from_rgb(0x0b, 0x6b, 0x32),
                warning: Color32::from_rgb(0x7a, 0x5a, 0x00),
                destructive: Color32::from_rgb(0xc1, 0x12, 0x1f),
            },

            (Style::Bright, true) => Accents {
                accent: Color32::from_rgb(0x8e, 0xe8, 0xd8),
                positive: Color32::from_rgb(0x9c, 0xe8, 0xb4),
                warning: Color32::from_rgb(0xf0, 0xd0, 0x90),
                destructive: Color32::from_rgb(0xff, 0xb3, 0xad),
            },
            (Style::Bright, false) => Accents {
                accent: Color32::from_rgb(0x2a, 0x8f, 0xa8),
                positive: Color32::from_rgb(0x2f, 0x9e, 0x5f),
                warning: Color32::from_rgb(0x9a, 0x70, 0x00),
                destructive: Color32::from_rgb(0xc9, 0x3a, 0x30),
            },

            (Style::DarkMode, true) => Accents {
                accent: Color32::from_rgb(0x5f, 0xb3, 0xaa),
                positive: Color32::from_rgb(0x6c, 0xbf, 0x9c),
                warning: Color32::from_rgb(0xc4, 0xa8, 0x84),
                destructive: Color32::from_rgb(0xe0, 0x7a, 0x70),
            },
            (Style::DarkMode, false) => Accents {
                accent: Color32::from_rgb(0x2d, 0x6f, 0x68),
                positive: Color32::from_rgb(0x2f, 0x6f, 0x5f),
                warning: Color32::from_rgb(0x7a, 0x64, 0x40),
                destructive: Color32::from_rgb(0xa8, 0x44, 0x3c),
            },

            (Style::Mono, true) => Accents {
                accent: Color32::from_rgb(0xd0, 0xd0, 0xd0),
                positive: Color32::from_rgb(0xd0, 0xd0, 0xd0),
                warning: Color32::from_rgb(0xa8, 0xa8, 0xa8),
                destructive: Color32::from_rgb(0x80, 0x80, 0x80),
            },
            (Style::Mono, false) => Accents {
                accent: Color32::from_rgb(0x40, 0x40, 0x40),
                positive: Color32::from_rgb(0x40, 0x40, 0x40),
                warning: Color32::from_rgb(0x6a, 0x6a, 0x6a),
                destructive: Color32::from_rgb(0x90, 0x90, 0x90),
            },

            (Style::Custom, _) => unreachable!("handled above"),
        }
    }
}

struct Accents {
    accent: Color32,
    positive: Color32,
    warning: Color32,
    destructive: Color32,
}

/// Surfaces and text, chosen by the light/dark scheme alone.
struct Surfaces {
    bg_app: Color32,
    bg_sidebar: Color32,
    bg_panel: Color32,
    bg_input: Color32,
    bg_hover: Color32,
    border: Color32,
    text_primary: Color32,
    text_secondary: Color32,
}

impl Surfaces {
    const DARK: Self = Self {
        bg_app: Color32::from_rgb(0x2b, 0x2b, 0x2b),
        bg_sidebar: Color32::from_rgb(0x30, 0x30, 0x30),
        bg_panel: Color32::from_rgb(0x36, 0x36, 0x36),
        bg_input: Color32::from_rgb(0x3f, 0x3f, 0x3f),
        bg_hover: Color32::from_rgb(0x40, 0x40, 0x40),
        border: Color32::from_rgb(0x45, 0x45, 0x45),
        text_primary: Color32::WHITE,
        text_secondary: Color32::from_rgb(0xb4, 0xb4, 0xb4),
    };

    const LIGHT: Self = Self {
        bg_app: Color32::from_rgb(0xf0, 0xf0, 0xf0),
        bg_sidebar: Color32::from_rgb(0xfa, 0xfa, 0xfa),
        bg_panel: Color32::from_rgb(0xff, 0xff, 0xff),
        bg_input: Color32::from_rgb(0xf5, 0xf5, 0xf5),
        bg_hover: Color32::from_rgb(0xe8, 0xe8, 0xe8),
        border: Color32::from_rgb(0xe0, 0xe0, 0xe0),
        text_primary: Color32::BLACK,
        text_secondary: Color32::from_rgb(0x4a, 0x4a, 0x4a),
    };

    fn of(dark: bool) -> Self {
        if dark {
            Self::DARK
        } else {
            Self::LIGHT
        }
    }
}

/// Everything a view draws with.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub style: Style,
    pub bg_app: Color32,
    pub bg_sidebar: Color32,
    pub bg_panel: Color32,
    pub bg_input: Color32,
    pub bg_hover: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub positive: Color32,
    pub warning: Color32,
    pub destructive: Color32,
}

impl Palette {
    pub fn new(theme: Theme, style: Style, custom: &BTreeMap<String, String>) -> Self {
        let dark = theme == Theme::Dark;
        let surfaces = Surfaces::of(dark);
        let accents = style.accents(dark, custom);

        Self {
            dark,
            style,
            bg_app: surfaces.bg_app,
            bg_sidebar: surfaces.bg_sidebar,
            bg_panel: surfaces.bg_panel,
            bg_input: surfaces.bg_input,
            bg_hover: surfaces.bg_hover,
            border: surfaces.border,
            accent: accents.accent,
            on_accent: readable_on(accents.accent),
            text_primary: surfaces.text_primary,
            text_secondary: surfaces.text_secondary,
            positive: accents.positive,
            warning: accents.warning,
            destructive: accents.destructive,
        }
    }

    /// Builds the egui visuals. Flat by design: no shadows, no glow.
    pub fn visuals(&self) -> Visuals {
        let mut visuals = if self.dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        visuals.panel_fill = self.bg_app;
        visuals.window_fill = self.bg_panel;
        visuals.extreme_bg_color = self.bg_input;
        visuals.faint_bg_color = self.bg_hover;
        visuals.override_text_color = Some(self.text_primary);
        visuals.hyperlink_color = self.accent;
        visuals.window_stroke = Stroke::new(1.0, self.border);
        visuals.window_corner_radius = CornerRadius::same(12);
        visuals.menu_corner_radius = CornerRadius::same(8);
        visuals.slider_trailing_fill = true;
        visuals.selection.bg_fill = self.accent.gamma_multiply(0.35);
        visuals.selection.stroke = Stroke::new(1.0, self.accent);
        visuals.window_shadow = Shadow::NONE;
        visuals.popup_shadow = Shadow::NONE;

        let corner = CornerRadius::same(8);

        visuals.widgets.noninteractive.bg_fill = self.bg_panel;
        visuals.widgets.noninteractive.weak_bg_fill = self.bg_panel;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.border);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.text_secondary);
        visuals.widgets.noninteractive.corner_radius = corner;
        visuals.widgets.noninteractive.expansion = 0.0;

        visuals.widgets.inactive.bg_fill = self.bg_input;
        visuals.widgets.inactive.weak_bg_fill = self.bg_input;
        visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, self.text_primary);
        visuals.widgets.inactive.corner_radius = corner;
        visuals.widgets.inactive.expansion = 0.0;

        visuals.widgets.hovered.bg_fill = self.bg_hover;
        visuals.widgets.hovered.weak_bg_fill = self.bg_hover;
        visuals.widgets.hovered.bg_stroke = Stroke::NONE;
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, self.text_primary);
        visuals.widgets.hovered.corner_radius = corner;
        visuals.widgets.hovered.expansion = 0.0;

        visuals.widgets.active.bg_fill = self.accent;
        visuals.widgets.active.weak_bg_fill = self.accent.gamma_multiply(0.8);
        visuals.widgets.active.bg_stroke = Stroke::NONE;
        visuals.widgets.active.fg_stroke = Stroke::new(1.5, self.on_accent);
        visuals.widgets.active.corner_radius = corner;
        visuals.widgets.active.expansion = 0.0;

        visuals.widgets.open.bg_fill = self.bg_panel;
        visuals.widgets.open.weak_bg_fill = self.bg_panel;
        visuals.widgets.open.bg_stroke = Stroke::new(1.0, self.border);
        visuals.widgets.open.fg_stroke = Stroke::new(1.0, self.text_primary);
        visuals.widgets.open.corner_radius = corner;
        visuals.widgets.open.expansion = 0.0;

        visuals
    }
}

/// Installs both schemes for the given style and spacing rules.
pub fn install(ctx: &egui::Context, style: Style, custom: &BTreeMap<String, String>) {
    for theme in [Theme::Dark, Theme::Light] {
        let mut egui_style = (*ctx.style_of(theme)).clone();
        egui_style.visuals = Palette::new(theme, style, custom).visuals();
        egui_style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        egui_style.spacing.button_padding = egui::vec2(12.0, 6.0);
        ctx.set_style_of(theme, egui_style);
    }
}

/// Selects the scheme to draw with; `System` follows the desktop.
pub fn set_mode(ctx: &egui::Context, mode: ThemeMode) {
    ctx.set_theme(mode);
}

/// The palette in effect right now.
pub fn current(ctx: &egui::Context, style: Style, custom: &BTreeMap<String, String>) -> Palette {
    Palette::new(ctx.theme(), style, custom)
}

/// Parses the value stored in the configuration file.
pub fn mode_from_config(value: &str) -> ThemeMode {
    match value {
        "light" => ThemeMode::Light,
        "dark" => ThemeMode::Dark,
        _ => ThemeMode::System,
    }
}

/// Serialises the mode for the configuration file.
pub fn mode_to_config(mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::Light => "light",
        ThemeMode::Dark => "dark",
        ThemeMode::System => "system",
    }
}

/// The opposite of the scheme in effect, for the quick toggle.
pub fn toggled(current: Theme) -> ThemeMode {
    match current {
        Theme::Dark => ThemeMode::Light,
        Theme::Light => ThemeMode::Dark,
    }
}

/// Black or white, whichever reads better on `fill`.
///
/// Comparing the two WCAG ratios also guarantees the better choice clears 4.5,
/// since their geometric mean is sqrt(21).
fn readable_on(fill: Color32) -> Color32 {
    let luminance = relative_luminance(fill);
    let against_black = (luminance + 0.05) / 0.05;
    let against_white = 1.05 / (luminance + 0.05);
    if against_black >= against_white {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

/// WCAG relative luminance.
fn relative_luminance(color: Color32) -> f32 {
    fn channel(value: u8) -> f32 {
        let value = value as f32 / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
}

/// Reads one of the four custom hex colours.
fn custom_color(custom: &BTreeMap<String, String>, key: &str) -> Option<Color32> {
    let value = custom.get(key)?.trim().trim_start_matches('#');
    if value.len() != 6 {
        return None;
    }
    let rgb = u32::from_str_radix(value, 16).ok()?;
    Some(Color32::from_rgb(
        ((rgb >> 16) & 0xff) as u8,
        ((rgb >> 8) & 0xff) as u8,
        (rgb & 0xff) as u8,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (first, second) = (relative_luminance(a), relative_luminance(b));
        let (high, low) = if first > second {
            (first, second)
        } else {
            (second, first)
        };
        (high + 0.05) / (low + 0.05)
    }

    #[test]
    fn text_is_black_on_light_and_white_on_dark() {
        let custom = BTreeMap::new();
        for style in Style::ALL {
            let light = Palette::new(Theme::Light, style, &custom);
            assert_eq!(light.text_primary, Color32::BLACK, "{style:?} light text");
            let dark = Palette::new(Theme::Dark, style, &custom);
            assert_eq!(dark.text_primary, Color32::WHITE, "{style:?} dark text");
        }
    }

    #[test]
    fn every_style_keeps_text_readable() {
        let custom = BTreeMap::new();
        for style in Style::ALL {
            for theme in [Theme::Light, Theme::Dark] {
                let palette = Palette::new(theme, style, &custom);
                for (label, foreground, background) in [
                    ("text on panel", palette.text_primary, palette.bg_panel),
                    ("text on app", palette.text_primary, palette.bg_app),
                    ("label on accent", palette.on_accent, palette.accent),
                ] {
                    let ratio = contrast(foreground, background);
                    assert!(
                        ratio >= 4.5,
                        "{style:?} {theme:?}: {label} contrast {ratio:.2}"
                    );
                }
            }
        }
    }

    #[test]
    fn styles_map_to_and_from_their_config_names() {
        for style in Style::ALL {
            assert_eq!(Style::from_config(style.as_config()), style);
        }
        assert_eq!(Style::from_config("nonsense"), Style::Default);
    }

    #[test]
    fn custom_style_reads_the_user_colours() {
        let mut custom = BTreeMap::new();
        custom.insert("ok".to_owned(), "#112233".to_owned());
        custom.insert("low".to_owned(), "#445566".to_owned());
        custom.insert("degraded".to_owned(), "#778899".to_owned());

        let palette = Palette::new(Theme::Dark, Style::Custom, &custom);
        assert_eq!(palette.accent, Color32::from_rgb(0x11, 0x22, 0x33));
        assert_eq!(palette.destructive, Color32::from_rgb(0x44, 0x55, 0x66));
        assert_eq!(palette.warning, Color32::from_rgb(0x77, 0x88, 0x99));
    }

    #[test]
    fn custom_style_falls_back_when_colours_are_missing() {
        let palette = Palette::new(Theme::Dark, Style::Custom, &BTreeMap::new());
        let fallback = Palette::new(Theme::Dark, Style::Default, &BTreeMap::new());
        assert_eq!(palette.accent, fallback.accent);
    }

    #[test]
    fn config_round_trips_every_mode() {
        for mode in [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark] {
            assert_eq!(mode_from_config(mode_to_config(mode)), mode);
        }
        assert_eq!(mode_from_config("nonsense"), ThemeMode::System);
    }

    #[test]
    fn toggling_flips_the_scheme() {
        assert_eq!(toggled(Theme::Dark), ThemeMode::Light);
        assert_eq!(toggled(Theme::Light), ThemeMode::Dark);
    }
}
