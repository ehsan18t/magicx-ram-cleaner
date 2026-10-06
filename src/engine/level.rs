//! Cleaning levels.

use serde::{Deserialize, Serialize};

/// Cleaning aggressiveness level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CleanLevel {
    /// Gentle: Purge ALL standby pages (priorities 0-7).
    /// Standby pages are already outside every process's working set;
    /// purging them is completely safe and frees the disk-page cache.
    Gentle,
    /// Moderate: Flush modified pages to disk, then purge ALL standby.
    /// No process working sets are touched - safe for running apps.
    /// More thorough than Gentle because it also drains the modified list.
    Moderate,
    /// Aggressive: File cache flush + registry flush + empty working sets + flush modified + purge ALL standby.
    /// Frees maximum RAM but may cause brief I/O spike as apps re-fault pages.
    Aggressive,
    /// Nuclear: Everything aggressive does, plus memory combining.
    /// Use when you need every last byte freed. May cause temporary slowdown.
    Nuclear,
}

impl std::fmt::Display for CleanLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gentle => write!(f, "gentle"),
            Self::Moderate => write!(f, "moderate"),
            Self::Aggressive => write!(f, "aggressive"),
            Self::Nuclear => write!(f, "nuclear"),
        }
    }
}

impl CleanLevel {
    /// Returns the Title Case display name for use in GUI labels, status
    /// messages, and terminal output where the level is shown as a proper name.
    ///
    /// Use this instead of [`Display`](std::fmt::Display) (which returns
    /// lowercase for CLI argument compatibility) whenever the context requires
    /// a capitalised label: result cards, combo boxes, log messages, etc.
    #[must_use]
    pub const fn title_case_name(self) -> &'static str {
        match self {
            Self::Gentle => crate::strings::levels::GENTLE_NAME,
            Self::Moderate => crate::strings::levels::MODERATE_NAME,
            Self::Aggressive => crate::strings::levels::AGGRESSIVE_NAME,
            Self::Nuclear => crate::strings::levels::NUCLEAR_NAME,
        }
    }

    /// Whether this level empties process working sets (and therefore
    /// honours process exclusions).
    #[must_use]
    pub const fn empties_working_sets(self) -> bool {
        matches!(self, Self::Aggressive | Self::Nuclear)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_level_ordering() {
        assert!(CleanLevel::Gentle < CleanLevel::Moderate);
        assert!(CleanLevel::Moderate < CleanLevel::Aggressive);
        assert!(CleanLevel::Aggressive < CleanLevel::Nuclear);
    }

    #[test]
    fn clean_level_display() {
        assert_eq!(CleanLevel::Gentle.to_string(), "gentle");
        assert_eq!(CleanLevel::Moderate.to_string(), "moderate");
        assert_eq!(CleanLevel::Aggressive.to_string(), "aggressive");
        assert_eq!(CleanLevel::Nuclear.to_string(), "nuclear");
    }

    #[test]
    fn only_top_levels_empty_working_sets() {
        assert!(!CleanLevel::Gentle.empties_working_sets());
        assert!(!CleanLevel::Moderate.empties_working_sets());
        assert!(CleanLevel::Aggressive.empties_working_sets());
        assert!(CleanLevel::Nuclear.empties_working_sets());
    }
}
