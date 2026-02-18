//! # Settings Persistence
//!
//! Loads and saves [`super::app::GuiSettings`] to
//! `%APPDATA%\MagicX\RAM Cleaner\settings.json`.
//!
//! Gracefully falls back to [`Default`] on any read error so a missing or
//! corrupted file never prevents the app from starting. Save errors are
//! silently ignored — a failed write must not crash the app on exit.

use std::path::PathBuf;

use super::app::GuiSettings;

/// Returns the absolute path to the settings JSON file.
///
/// Path: `%APPDATA%\MagicX\RAM Cleaner\settings.json`
///
/// Returns [`None`] if the `APPDATA` environment variable is not set.
fn settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| {
        PathBuf::from(appdata)
            .join("MagicX")
            .join("RAM Cleaner")
            .join("settings.json")
    })
}

/// Load [`GuiSettings`] from disk.
///
/// Returns [`GuiSettings::default`] when:
/// - `APPDATA` is not set,
/// - the settings file does not yet exist, or
/// - the file contains invalid or incompatible JSON.
///
/// Unknown fields are silently ignored thanks to `serde`'s default
/// `deny_unknown_fields = false` behaviour, so old settings files survive
/// schema additions across app versions.
pub fn load_settings() -> GuiSettings {
    let Some(path) = settings_path() else {
        return GuiSettings::default();
    };

    let Ok(content) = std::fs::read_to_string(&path) else {
        // File missing on first launch or unreadable — use defaults.
        return GuiSettings::default();
    };

    serde_json::from_str(&content).unwrap_or_default()
}

/// Persist [`GuiSettings`] to disk.
///
/// Creates all parent directories if they do not already exist. All I/O
/// errors are silently discarded — a failed write must not surface to
/// the user during normal app shutdown.
pub fn save_settings(settings: &GuiSettings) {
    let Some(path) = settings_path() else {
        return;
    };

    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return;
    }

    let Ok(json) = serde_json::to_string_pretty(settings) else {
        return;
    };

    // Intentionally discard write error — a failed save on exit is non-fatal.
    // Named binding (not `_`) defers the drop and avoids let_underscore_drop.
    let _write_result = std::fs::write(&path, json);
}
