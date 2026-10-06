//! # Settings Manager
//!
//! Central [`SettingsManager`] for all [`super::settings::GuiSettings`] I/O.
//!
//! Handles loading, saving, importing and exporting. (Autostart is Windows
//! integration, see [`crate::integration::autostart`].)
//!
//! The default persistence path is `settings.json` next to the running executable.
//! Import and export open native Win32 file-picker dialogs (COMDLG32).
//!
//! Gracefully falls back to [`Default`] on any read error so a missing or
//! corrupted file never prevents the app from starting.

use std::path::{Path, PathBuf};

use super::settings::GuiSettings;
use crate::platform::dialog;

use crate::strings;

// ─── Default Path ─────────────────────────────────────────────────────────────

/// Returns the default settings JSON path: `<exe directory>\settings.json`.
///
/// Falls back to `settings.json` in the current working directory if the
/// executable path cannot be resolved.
fn default_settings_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("settings.json")
}

// ─── Low-Level I/O ────────────────────────────────────────────────────────────

/// Deserialise [`GuiSettings`] from a JSON file and clamp it to valid ranges.
fn read_settings_file(path: &Path) -> Result<GuiSettings, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("Cannot read file: {e}"))?;
    let mut settings: GuiSettings =
        serde_json::from_str(&content).map_err(|e| format!("Invalid settings file: {e}"))?;
    settings.sanitize();
    Ok(settings)
}

/// Serialise `settings` as pretty JSON to `path`, creating parent directories.
///
/// The JSON is first written to a sibling `<name>.tmp` file which is then
/// renamed over `path`, so a crash or power loss mid-write never leaves a
/// truncated settings file behind.
fn write_settings_file(path: &Path, settings: &GuiSettings) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return Err(format!("Cannot create directory: {}", path.display()));
    }

    let json =
        serde_json::to_string_pretty(settings).map_err(|e| format!("Serialisation error: {e}"))?;

    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp_path = path.with_file_name(tmp_name);

    std::fs::write(&tmp_path, json).map_err(|e| format!("Cannot write file: {e}"))?;
    std::fs::rename(&tmp_path, path).map_err(|e| {
        // Named binding avoids `let_underscore_drop`; cleanup is best-effort.
        let _cleanup = std::fs::remove_file(&tmp_path);
        format!("Cannot replace file: {e}")
    })
}

// ─── Settings Manager ─────────────────────────────────────────────────────────

/// Central manager for all settings persistence operations.
///
/// A stateless unit struct - every method takes settings by reference or
/// returns new values. This is the single place to add versioning,
/// migration, or multi-profile logic in the future.
pub struct SettingsManager;

impl SettingsManager {
    /// Load [`GuiSettings`] from the default exe-directory path.
    ///
    /// - `Ok(Some(settings))` - loaded and sanitised.
    /// - `Ok(None)` - no settings file exists yet (first run).
    /// - `Err(msg)` - the file exists but is unreadable or not valid JSON.
    ///
    /// Unknown fields are silently ignored and missing fields take their
    /// default values, so existing files survive schema changes across app
    /// versions.
    pub fn load() -> Result<Option<GuiSettings>, String> {
        let path = default_settings_path();
        if !path.exists() {
            return Ok(None);
        }
        read_settings_file(&path).map(Some)
    }

    /// Save `settings` to the default exe-directory path.
    ///
    /// I/O errors are silently discarded - a failed write must not surface
    /// to the user during normal app shutdown.
    pub fn save(settings: &GuiSettings) {
        let path = default_settings_path();

        // Named binding avoids `let_underscore_drop`; error is intentionally ignored.
        let _write_result = write_settings_file(&path, settings);
    }

    /// Export `settings` to a user-chosen file via a native Save dialog.
    ///
    /// - `Ok(Some(path))` - exported successfully; `path` is where the file was written.
    /// - `Ok(None)` - user cancelled the dialog.
    /// - `Err(msg)` - the user confirmed a path but the write failed.
    pub fn export(settings: &GuiSettings) -> Result<Option<PathBuf>, String> {
        let Some(path) = dialog::pick_save_json(
            strings::gui::persistence::EXPORT_TITLE,
            "magicx-settings.json",
        ) else {
            return Ok(None);
        };
        write_settings_file(&path, settings)?;
        Ok(Some(path))
    }

    /// Import settings from a user-chosen file via a native Open dialog.
    ///
    /// - `Ok(Some(settings))` - loaded (and sanitised) from the chosen file.
    /// - `Ok(None)` - user cancelled the dialog.
    /// - `Err(msg)` - file was chosen but could not be read or parsed.
    pub fn import() -> Result<Option<GuiSettings>, String> {
        let Some(path) = dialog::pick_open_json(strings::gui::persistence::IMPORT_TITLE) else {
            return Ok(None);
        };
        read_settings_file(&path).map(Some)
    }
}
