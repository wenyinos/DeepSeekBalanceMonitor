//! COSMIC-inspired theme: two palettes plus the egui styles built from them.
//!
//! The light/dark switch rides on egui's own theme preference, so following the
//! desktop is handled by the framework and the choice survives between frames.
//! Views read the semantic colour names off [`Palette`] instead of hard-coding
//! values.

use egui::{Color32, CornerRadius, Shadow, Stroke, Theme, Visuals};

/// What the user picked: follow the desktop, or force one scheme.
pub use egui::ThemePreference as ThemeMode;

/// Semantic colours shared by every view.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
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
    pub const DARK: Self = Self {
        dark: true,
        bg_app: Color32::from_rgb(0x2b, 0x2b, 0x2b),
        bg_sidebar: Color32::from_rgb(0x30, 0x30, 0x30),
        bg_panel: Color32::from_rgb(0x36, 0x36, 0x36),
        bg_input: Color32::from_rgb(0x3f, 0x3f, 0x3f),
        bg_hover: Color32::from_rgb(0x40, 0x40, 0x40),
        border: Color32::from_rgb(0x45, 0x45, 0x45),
        accent: Color32::from_rgb(0x5e, 0xe0, 0xc8),
        on_accent: Color32::from_rgb(0x1a, 0x1a, 0x1a),
        text_primary: Color32::from_rgb(0xe6, 0xe6, 0xe6),
        text_secondary: Color32::from_rgb(0xa0, 0xa0, 0xa0),
        positive: Color32::from_rgb(0x7e, 0xe0, 0xa8),
        warning: Color32::from_rgb(0xe8, 0xc0, 0x7a),
        destructive: Color32::from_rgb(0xf0, 0xa0, 0xa8),
    };

    pub const LIGHT: Self = Self {
        dark: false,
        bg_app: Color32::from_rgb(0xf0, 0xf0, 0xf0),
        bg_sidebar: Color32::from_rgb(0xfa, 0xfa, 0xfa),
        bg_panel: Color32::from_rgb(0xff, 0xff, 0xff),
        bg_input: Color32::from_rgb(0xf5, 0xf5, 0xf5),
        bg_hover: Color32::from_rgb(0xe8, 0xe8, 0xe8),
        border: Color32::from_rgb(0xe0, 0xe0, 0xe0),
        accent: Color32::from_rgb(0x1a, 0x6a, 0x94),
        on_accent: Color32::from_rgb(0xff, 0xff, 0xff),
        text_primary: Color32::from_rgb(0x1a, 0x1a, 0x1a),
        text_secondary: Color32::from_rgb(0x5a, 0x5a, 0x5a),
        positive: Color32::from_rgb(0x2e, 0x7d, 0x32),
        warning: Color32::from_rgb(0x8a, 0x61, 0x00),
        destructive: Color32::from_rgb(0xb3, 0x26, 0x1e),
    };

    pub fn of(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::DARK,
            Theme::Light => Self::LIGHT,
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

/// Installs the palettes and the shared spacing rules for both schemes.
pub fn install(ctx: &egui::Context) {
    for theme in [Theme::Dark, Theme::Light] {
        let mut style = (*ctx.style_of(theme)).clone();
        style.visuals = Palette::of(theme).visuals();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        ctx.set_style_of(theme, style);
    }
}

/// Selects the scheme to draw with; `System` follows the desktop.
pub fn set_mode(ctx: &egui::Context, mode: ThemeMode) {
    ctx.set_theme(mode);
}

/// The palette in effect right now.
pub fn current(ctx: &egui::Context) -> Palette {
    Palette::of(ctx.theme())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance.
    fn luminance(color: Color32) -> f32 {
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

    /// WCAG contrast ratio, 1.0 (identical) to 21.0 (black on white).
    fn contrast(a: Color32, b: Color32) -> f32 {
        let (first, second) = (luminance(a), luminance(b));
        let (high, low) = if first > second {
            (first, second)
        } else {
            (second, first)
        };
        (high + 0.05) / (low + 0.05)
    }

    #[test]
    fn palettes_keep_text_readable() {
        for palette in [Palette::DARK, Palette::LIGHT] {
            for (label, foreground, background) in [
                ("primary text on panel", palette.text_primary, palette.bg_panel),
                ("primary text on app", palette.text_primary, palette.bg_app),
                ("label on accent", palette.on_accent, palette.accent),
                (
                    "secondary text on panel",
                    palette.text_secondary,
                    palette.bg_panel,
                ),
            ] {
                let ratio = contrast(foreground, background);
                assert!(
                    ratio >= 4.5,
                    "{label} must reach WCAG AA, got {ratio:.2}"
                );
            }
        }
    }

    #[test]
    fn config_round_trips_every_mode() {
        for mode in [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark] {
            assert_eq!(mode_from_config(mode_to_config(mode)), mode);
        }
        assert_eq!(mode_from_config("nonsense"), ThemeMode::System);
    }
}
