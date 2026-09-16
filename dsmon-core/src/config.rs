//! Application configuration, stored as `config.json` next to the history.
//!
//! The field set follows the previous Rust builds minus the entries that only
//! existed for the removed CLI (`language`, `api_key`) and the pass-through bag
//! that swallowed unknown keys. New fields cover the widget and window state.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::paths;

/// Alert policy for a low balance.
pub const ALERT_MODES: [&str; 3] = ["never", "always", "once"];
/// Tray icon colour presets.
pub const THEMES: [&str; 6] = [
    "default",
    "contrast",
    "bright",
    "dark_mode",
    "mono",
    "custom",
];
/// Interface schemes.
pub const UI_THEMES: [&str; 3] = ["system", "light", "dark"];
/// Interface languages.
pub const LANGUAGES: [&str; 2] = ["zh", "en"];
/// Widget sizes.
pub const WIDGET_SIZES: [&str; 2] = ["compact", "standard"];

pub const MIN_INTERVAL_MINUTES: u64 = 1;
pub const MAX_INTERVAL_MINUTES: u64 = 1440;
pub const MAX_THRESHOLD_YUAN: f64 = 10_000.0;
pub const MIN_RETENTION_DAYS: u64 = 1;
pub const MAX_RETENTION_DAYS: u64 = 3650;
/// Billing days are capped at 28 so every month has the date.
/// Highest billing day a month can be said to have; shorter months fall back
/// to their last day.
pub const MAX_BILLING_DAY: u8 = 31;
/// The four opacity levels the widget's title-bar button cycles through.
///
/// The button draws the step it is on rather than a percentage, so the steps
/// have to be far enough apart to be told apart at icon size — hence a fixed
/// list and not a continuous value. The lowest step has to be in
/// [`MIN_WIDGET_OPACITY`] or writing it back would be clamped away.
pub const WIDGET_OPACITY_LEVELS: [f32; 4] = [0.25, 0.50, 0.75, 0.90];
pub const MIN_WIDGET_WIDTH: f32 = 280.0;
pub const MAX_WIDGET_WIDTH: f32 = 2000.0;
pub const MIN_WIDGET_HEIGHT: f32 = 240.0;
pub const MAX_WIDGET_HEIGHT: f32 = 2000.0;
pub const MIN_WIDGET_OPACITY: f32 = 0.25;
pub const MAX_WIDGET_OPACITY: f32 = 0.90;

/// The level closest to `value`, ties going to the lower one.
pub fn widget_opacity_level(value: f32) -> f32 {
    WIDGET_OPACITY_LEVELS
        .iter()
        .copied()
        .min_by(|left, right| {
            (left - value)
                .abs()
                .partial_cmp(&(right - value).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(MAX_WIDGET_OPACITY)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_interval")]
    pub interval_minutes: u64,
    #[serde(default = "default_threshold")]
    pub threshold_yuan: f64,
    #[serde(default = "default_ui_language")]
    pub ui_language: String,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default = "default_alert_mode")]
    pub alert_mode: String,
    #[serde(default = "default_api_alert_enabled")]
    pub api_alert_enabled: bool,
    /// Spending in one day that is worth a notification, in the account's own
    /// currency. Zero turns the alert off, which is how it ships: the line is
    /// a figure each user has to pick for themselves.
    #[serde(default)]
    pub brisk_threshold_yuan: f64,
    /// Whether DeepSeek's off-peak discount starting and ending is announced.
    #[serde(default = "default_true")]
    pub peak_alert_enabled: bool,
    #[serde(default = "default_retention_days")]
    pub retention_days: u64,
    #[serde(default)]
    pub export_path: String,
    #[serde(default)]
    pub http_proxy: String,
    #[serde(default)]
    pub proxy_enabled: bool,

    #[serde(default = "default_ui_theme")]
    pub ui_theme: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub icon_colors: BTreeMap<String, String>,
    #[serde(default)]
    pub icon_stroke: bool,

    #[serde(default)]
    pub widget_enabled: bool,
    #[serde(default = "default_widget_size")]
    pub widget_size: String,
    #[serde(default = "default_widget_opacity")]
    pub widget_opacity: f32,
    #[serde(default = "default_true")]
    pub widget_always_on_top: bool,
    #[serde(default = "default_true")]
    pub widget_show_trend: bool,
    /// Whether the desktop widget asks the session to start it, the way
    /// `auto_start` does for the application. The two entries are separate: the
    /// widget can come up at login on its own — saying so when the application
    /// is not running — or wait to be started by the application.
    #[serde(default = "default_true")]
    pub widget_auto_start: bool,
    #[serde(default)]
    pub widget_pos: Option<[f32; 2]>,

    /// Day of the month Command Code renews on, 1-28. Its API reports no
    /// period end, unlike OpenCode Go, so the cycle is taken from here.
    #[serde(default = "default_billing_day")]
    pub billing_day_command_code: u8,

    /// Left over from the interface this project used to have: nothing reads
    /// it now, and the widget has its own size field below.
    #[serde(default)]
    pub window_size: Option<[f32; 2]>,
    /// The widget's own size, written when the user drags its edge. Kept apart
    /// from `window_size` because that one carries the previous build's window
    /// geometry, which has nothing to do with the widget.
    #[serde(default)]
    pub widget_window_size: Option<[f32; 2]>,
    #[serde(default)]
    pub window_pos: Option<[f32; 2]>,

    /// Set once the "still running in the tray" notice has been shown.
    #[serde(default)]
    pub tray_hint_shown: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            interval_minutes: default_interval(),
            threshold_yuan: default_threshold(),
            ui_language: default_ui_language(),
            auto_start: false,
            alert_mode: default_alert_mode(),
            api_alert_enabled: default_api_alert_enabled(),
            brisk_threshold_yuan: 0.0,
            peak_alert_enabled: true,
            retention_days: default_retention_days(),
            export_path: String::new(),
            http_proxy: String::new(),
            proxy_enabled: false,
            ui_theme: default_ui_theme(),
            theme: default_theme(),
            icon_colors: BTreeMap::new(),
            icon_stroke: false,
            widget_enabled: false,
            widget_size: default_widget_size(),
            widget_opacity: default_widget_opacity(),
            widget_always_on_top: true,
            widget_show_trend: true,
            widget_auto_start: true,
            widget_pos: None,
            billing_day_command_code: default_billing_day(),
            window_size: None,
            widget_window_size: None,
            window_pos: None,
            tray_hint_shown: false,
        }
    }
}

fn default_interval() -> u64 {
    10
}

fn default_threshold() -> f64 {
    1.0
}

fn default_ui_language() -> String {
    "zh".to_owned()
}

fn default_alert_mode() -> String {
    "once".to_owned()
}

fn default_api_alert_enabled() -> bool {
    true
}

fn default_retention_days() -> u64 {
    30
}

fn default_ui_theme() -> String {
    "system".to_owned()
}

fn default_theme() -> String {
    "default".to_owned()
}

fn default_billing_day() -> u8 {
    1
}

fn default_widget_size() -> String {
    "standard".to_owned()
}

fn default_widget_opacity() -> f32 {
    0.9
}

fn default_true() -> bool {
    true
}

impl AppConfig {
    /// Reads the configuration.
    ///
    /// A missing file yields the defaults. A file that fails to parse is
    /// renamed to `config.json.corrupt` first, so the broken content is
    /// preserved for the user instead of being silently overwritten.
    pub fn load() -> Self {
        let path = paths::config_file();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => return Self::default(),
        };

        match serde_json::from_str::<Self>(&text) {
            Ok(mut config) => {
                config.normalize();
                config
            }
            Err(_) => {
                let mut backup = path.clone();
                backup.set_extension("json.corrupt");
                let _ = std::fs::rename(&path, &backup);
                Self::default()
            }
        }
    }

    /// Writes the configuration, creating the directory if needed.
    ///
    /// The file is written beside itself and moved into place, because a second
    /// reader now exists: the desktop widget re-reads this file while the
    /// settings page writes it. A reader that catches a half-written file gets
    /// no parse, and [`Self::load`] answers a file that does not parse by
    /// renaming it to `config.json.corrupt` and falling back to the defaults —
    /// so an unlucky overlap would silently throw the user's settings away.
    /// A rename within one directory is atomic: the file a reader opens is
    /// either the old one or the new one, never a partial one.
    pub fn save(&self) -> Result<(), String> {
        paths::ensure_dir(&paths::config_dir()).map_err(|error| error.to_string())?;

        let mut normalized = self.clone();
        normalized.normalize();

        let text = serde_json::to_string_pretty(&normalized).map_err(|error| error.to_string())?;
        let path = paths::config_file();
        let mut temporary = path.clone();
        temporary.set_file_name("config.json.tmp");

        std::fs::write(&temporary, text).map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, &path).map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            error.to_string()
        })
    }

    /// Clamps numeric fields into range and replaces unknown enum values with
    /// their defaults. Anything the file got wrong stays usable.
    pub fn normalize(&mut self) {
        self.interval_minutes = self
            .interval_minutes
            .clamp(MIN_INTERVAL_MINUTES, MAX_INTERVAL_MINUTES);
        if !self.threshold_yuan.is_finite() || self.threshold_yuan < 0.0 {
            self.threshold_yuan = 0.0;
        }
        self.threshold_yuan = self.threshold_yuan.min(MAX_THRESHOLD_YUAN);

        if !self.brisk_threshold_yuan.is_finite() || self.brisk_threshold_yuan < 0.0 {
            self.brisk_threshold_yuan = 0.0;
        }
        self.brisk_threshold_yuan = self.brisk_threshold_yuan.min(MAX_THRESHOLD_YUAN);
        self.retention_days = self
            .retention_days
            .clamp(MIN_RETENTION_DAYS, MAX_RETENTION_DAYS);

        self.widget_window_size = self.widget_window_size.map(|[width, height]| {
            [
                width.clamp(MIN_WIDGET_WIDTH, MAX_WIDGET_WIDTH),
                height.clamp(MIN_WIDGET_HEIGHT, MAX_WIDGET_HEIGHT),
            ]
        });

        if !self.widget_opacity.is_finite() {
            self.widget_opacity = default_widget_opacity();
        }
        // Clamped and then snapped: the widget's button only offers the four
        // levels, so a value from between them (hand-edited, or left over from
        // when this was a free number) settles on the nearest one instead of
        // staying a state the interface cannot show.
        self.widget_opacity = widget_opacity_level(
            self.widget_opacity
                .clamp(MIN_WIDGET_OPACITY, MAX_WIDGET_OPACITY),
        );

        if !ALERT_MODES.contains(&self.alert_mode.as_str()) {
            self.alert_mode = default_alert_mode();
        }
        if !THEMES.contains(&self.theme.as_str()) {
            self.theme = default_theme();
        }
        if !UI_THEMES.contains(&self.ui_theme.as_str()) {
            self.ui_theme = default_ui_theme();
        }
        if !LANGUAGES.contains(&self.ui_language.as_str()) {
            self.ui_language = default_ui_language();
        }
        if !WIDGET_SIZES.contains(&self.widget_size.as_str()) {
            self.widget_size = default_widget_size();
        }
        self.billing_day_command_code = self.billing_day_command_code.clamp(1, MAX_BILLING_DAY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> AppConfig {
        let mut config: AppConfig = serde_json::from_str(json).expect("config parses");
        config.normalize();
        config
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let config = parse("{}");
        assert_eq!(config, AppConfig::default());
        assert_eq!(config.interval_minutes, 10);
        assert_eq!(config.ui_theme, "system");
        assert!(config.widget_always_on_top);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let config = parse(r#"{"language":"fr","api_key":"legacy","nonsense":1}"#);
        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn values_are_clamped_into_range() {
        let config = parse(
            r#"{"interval_minutes":0,"threshold_yuan":-5,"retention_days":99999,"widget_opacity":3.0}"#,
        );
        assert_eq!(config.interval_minutes, MIN_INTERVAL_MINUTES);
        assert_eq!(config.threshold_yuan, 0.0);
        assert_eq!(config.retention_days, MAX_RETENTION_DAYS);
        assert_eq!(config.widget_opacity, MAX_WIDGET_OPACITY);
    }

    #[test]
    fn unknown_enum_values_fall_back() {
        let config = parse(
            r#"{"alert_mode":"sometimes","theme":"neon","ui_theme":"neon","widget_size":"huge"}"#,
        );
        assert_eq!(config.alert_mode, "once");
        assert_eq!(config.theme, "default");
        assert_eq!(config.ui_theme, "system");
        assert_eq!(config.widget_size, "standard");
    }

    #[test]
    fn round_trips_through_json() {
        let mut config = AppConfig::default();
        config.widget_enabled = true;
        config.widget_pos = Some([120.0, 80.0]);
        config.window_size = Some([960.0, 620.0]);
        config.widget_window_size = Some([380.0, 600.0]);
        let text = serde_json::to_string(&config).unwrap();
        assert_eq!(parse(&text), config);
    }

    #[test]
    fn a_dragged_widget_size_is_kept_within_reach() {
        let config = parse(r#"{"widget_window_size":[20.0, 9000.0]}"#);
        assert_eq!(
            config.widget_window_size,
            Some([MIN_WIDGET_WIDTH, MAX_WIDGET_HEIGHT])
        );
        let config = parse(r#"{"widget_window_size":[400.0,600.0]}"#);
        assert_eq!(config.widget_window_size, Some([400.0, 600.0]));
    }

    #[test]
    fn the_opacity_settles_on_one_of_the_four_levels() {
        assert_eq!(parse(r#"{"widget_opacity":0.9}"#).widget_opacity, 0.90);
        assert_eq!(parse(r#"{"widget_opacity":0.8}"#).widget_opacity, 0.75);
        assert_eq!(parse(r#"{"widget_opacity":0.6}"#).widget_opacity, 0.50);
        assert_eq!(parse(r#"{"widget_opacity":0.1}"#).widget_opacity, 0.25);
        assert_eq!(parse(r#"{"widget_opacity":1.0}"#).widget_opacity, 0.90);
        // Ties go to the lower level, so the boundary between 0.5 and 0.75
        // cannot land on either side depending on the day.
        assert_eq!(parse(r#"{"widget_opacity":0.625}"#).widget_opacity, 0.50);
        assert!(WIDGET_OPACITY_LEVELS.contains(&AppConfig::default().widget_opacity));
        assert!(WIDGET_OPACITY_LEVELS.contains(&MIN_WIDGET_OPACITY));
        assert!(WIDGET_OPACITY_LEVELS.contains(&MAX_WIDGET_OPACITY));
    }

    /// The file a reader opens must always be a whole one. A second reader
    /// exists now (the widget), and it runs into a half-written file often
    /// enough to matter — `load` answers a file that does not parse by moving
    /// it aside, which would throw the settings away.
    #[test]
    fn saving_leaves_a_whole_file_and_no_leftovers() {
        let _guard = crate::test_support::state_in_a_scratch_directory();
        let dir = paths::config_dir();
        let _ = std::fs::remove_dir_all(&dir);

        let mut config = AppConfig::default();
        config.interval_minutes = 42;
        config.widget_opacity = 0.25;
        config.save().expect("the configuration is written");

        assert_eq!(AppConfig::load().interval_minutes, 42);
        assert_eq!(AppConfig::load().widget_opacity, 0.25);

        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .expect("the directory exists")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temporary files left behind: {leftovers:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
