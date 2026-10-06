//! GUI settings: what is persisted, the defaults, and the valid ranges the
//! settings panels and the loader both use.

use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::engine::CleanLevel;

/// Valid range of the monitor threshold slider (percent).
pub const THRESHOLD_RANGE: RangeInclusive<u32> = 50..=99;

/// Valid range of the monitor cooldown slider (seconds).
pub const COOLDOWN_RANGE_SECS: RangeInclusive<u64> = 10..=300;

/// The "Top N" choices on the Processes page.
pub const TOP_PROCESS_CHOICES: [usize; 3] = [10, 20, 50];

/// Default monitor threshold percentage.
pub const DEFAULT_THRESHOLD: u32 = 80;

/// Default monitor cooldown in seconds.
pub const DEFAULT_COOLDOWN_SECS: u64 = 30;

/// Default auto-clean level.
pub const DEFAULT_CLEAN_LEVEL: CleanLevel = CleanLevel::Aggressive;

/// Default level picked on the Overview.
pub const DEFAULT_MANUAL_CLEAN_LEVEL: CleanLevel = CleanLevel::Gentle;

/// Default number of top processes to display.
pub const DEFAULT_TOP_PROCESSES: usize = 20;

/// Which theme the app uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// Follow the Windows app theme, switching live when it changes.
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

impl ThemeMode {
    /// Whether the app is dark, given whether Windows currently uses the
    /// light theme for apps.
    #[must_use]
    pub const fn is_dark(self, system_light: bool) -> bool {
        match self {
            Self::System => !system_light,
            Self::Light => false,
            Self::Dark => true,
        }
    }
}

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
    /// Level auto-clean runs (set on the Monitor page).
    pub default_clean_level: CleanLevel,
    /// Level picked on the Overview, kept separate from the auto-clean level
    /// so a one-off clean never changes what auto-clean does.
    pub manual_clean_level: CleanLevel,
    /// Number of top processes to show.
    pub top_process_count: usize,
    /// Theme preference.
    ///
    /// Files written before this field existed stored a `dark_mode` flag
    /// instead; the loader converts it (see `persistence`).
    pub theme: ThemeMode,
    /// Whether the navigation pane is expanded (on windows wide enough).
    pub nav_expanded: bool,
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
        // Snap to the nearest choice; older versions allowed any count.
        self.top_process_count = TOP_PROCESS_CHOICES
            .into_iter()
            .min_by_key(|choice| choice.abs_diff(self.top_process_count))
            .unwrap_or(DEFAULT_TOP_PROCESSES);
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
            manual_clean_level: DEFAULT_MANUAL_CLEAN_LEVEL,
            top_process_count: DEFAULT_TOP_PROCESSES,
            theme: ThemeMode::System,
            nav_expanded: true,
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
        assert_eq!(settings.top_process_count, TOP_PROCESS_CHOICES[0]);
    }

    #[test]
    fn missing_fields_take_defaults_so_old_files_still_load() {
        let settings: GuiSettings =
            serde_json::from_str(r#"{ "auto_start": true }"#).expect("partial file loads");
        assert_eq!(
            settings,
            GuiSettings {
                auto_start: true,
                ..GuiSettings::default()
            }
        );
    }

    #[test]
    fn top_process_count_snaps_to_the_nearest_choice() {
        for (stored, expected) in [(5, 10), (15, 10), (25, 20), (35, 20), (45, 50), (500, 50)] {
            let mut settings = GuiSettings {
                top_process_count: stored,
                ..GuiSettings::default()
            };
            settings.sanitize();
            assert_eq!(settings.top_process_count, expected, "stored {stored}");
        }
    }

    #[test]
    fn theme_mode_resolves_against_the_system_theme() {
        assert!(ThemeMode::System.is_dark(false));
        assert!(!ThemeMode::System.is_dark(true));
        assert!(ThemeMode::Dark.is_dark(true));
        assert!(!ThemeMode::Light.is_dark(false));
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
