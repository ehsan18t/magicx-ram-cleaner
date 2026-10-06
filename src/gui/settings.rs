//! GUI settings: what is persisted, the defaults, and the valid ranges the
//! settings panels and the loader both use.

use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::engine::CleanLevel;

/// Valid range of the monitor threshold slider (percent).
pub const THRESHOLD_RANGE: RangeInclusive<u32> = 50..=99;

/// Valid range of the monitor cooldown slider (seconds).
pub const COOLDOWN_RANGE_SECS: RangeInclusive<u64> = 10..=300;

/// Valid range of the "Show top" process count slider.
pub const TOP_PROCESSES_RANGE: RangeInclusive<usize> = 5..=50;

/// Default monitor threshold percentage.
pub const DEFAULT_THRESHOLD: u32 = 80;

/// Default monitor cooldown in seconds.
pub const DEFAULT_COOLDOWN_SECS: u64 = 30;

/// Default auto-clean level.
pub const DEFAULT_CLEAN_LEVEL: CleanLevel = CleanLevel::Aggressive;

/// Default number of top processes to display.
pub const DEFAULT_TOP_PROCESSES: usize = 20;

/// Persistent user settings.
///
/// Contains several independent boolean preferences; no meaningful two-variant
/// enum reduction exists without obscuring what each field controls.
///
/// Fields missing from a saved file take their [`Default`] values.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GuiSettings {
    /// Minimize to the system tray when the close button is clicked.
    ///
    /// When enabled, clicking ✕ hides the window to the notification area
    /// rather than quitting. The tray icon provides "Open" and "Quit" actions.
    #[serde(alias = "tray_enabled")]
    pub minimize_to_tray: bool,
    /// Launch automatically at Windows startup (current user only).
    ///
    /// Creates (or removes) a Task Scheduler logon task.
    pub auto_start: bool,
    /// Auto-clean threshold percentage (50 to 99).
    pub monitor_threshold: u32,
    /// Cooldown between auto-cleans (10 to 300 seconds).
    pub monitor_cooldown_secs: u64,
    /// Default cleaning level.
    pub default_clean_level: CleanLevel,
    /// Number of top processes to show.
    pub top_process_count: usize,
    /// Theme preference (`true` = dark).
    pub dark_mode: bool,
    /// Show tooltip with level details on circle hover (`true` = enabled).
    pub show_level_tooltips: bool,
    /// Whether auto-clean monitoring is enabled.
    ///
    /// Persisted so the monitor resumes automatically when the app is
    /// restarted.
    pub auto_clean_enabled: bool,
}

impl GuiSettings {
    /// Clamp numeric settings to the ranges their sliders allow.
    ///
    /// Applied after loading or importing a file, which may have been edited
    /// by hand or written by another version.
    pub fn sanitize(&mut self) {
        self.monitor_threshold = self
            .monitor_threshold
            .clamp(*THRESHOLD_RANGE.start(), *THRESHOLD_RANGE.end());
        self.monitor_cooldown_secs = self
            .monitor_cooldown_secs
            .clamp(*COOLDOWN_RANGE_SECS.start(), *COOLDOWN_RANGE_SECS.end());
        self.top_process_count = self
            .top_process_count
            .clamp(*TOP_PROCESSES_RANGE.start(), *TOP_PROCESSES_RANGE.end());
    }
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            minimize_to_tray: false,
            auto_start: false,
            monitor_threshold: DEFAULT_THRESHOLD,
            monitor_cooldown_secs: DEFAULT_COOLDOWN_SECS,
            default_clean_level: DEFAULT_CLEAN_LEVEL,
            top_process_count: DEFAULT_TOP_PROCESSES,
            dark_mode: true,
            show_level_tooltips: true,
            auto_clean_enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_within_their_ranges() {
        let defaults = GuiSettings::default();
        let mut sanitized = defaults.clone();
        sanitized.sanitize();
        assert_eq!(sanitized, defaults);
    }

    #[test]
    fn sanitize_clamps_out_of_range_values() {
        let mut settings = GuiSettings {
            monitor_threshold: 0,
            monitor_cooldown_secs: 100_000,
            top_process_count: 0,
            ..GuiSettings::default()
        };
        settings.sanitize();
        assert_eq!(settings.monitor_threshold, *THRESHOLD_RANGE.start());
        assert_eq!(settings.monitor_cooldown_secs, *COOLDOWN_RANGE_SECS.end());
        assert_eq!(settings.top_process_count, *TOP_PROCESSES_RANGE.start());
    }

    #[test]
    fn missing_fields_take_defaults_so_old_files_still_load() {
        let settings: GuiSettings =
            serde_json::from_str(r#"{ "dark_mode": false }"#).expect("partial file loads");
        assert_eq!(
            settings,
            GuiSettings {
                dark_mode: false,
                ..GuiSettings::default()
            }
        );
    }

    #[test]
    fn legacy_tray_field_name_is_accepted() {
        let settings: GuiSettings =
            serde_json::from_str(r#"{ "tray_enabled": true }"#).expect("legacy name loads");
        assert!(settings.minimize_to_tray);
    }

    #[test]
    fn settings_round_trip_through_json() {
        let settings = GuiSettings {
            auto_start: true,
            default_clean_level: CleanLevel::Gentle,
            ..GuiSettings::default()
        };
        let json = serde_json::to_string(&settings).expect("serializes");
        assert_eq!(
            serde_json::from_str::<GuiSettings>(&json).ok(),
            Some(settings)
        );
    }
}
