//! # Settings Manager
//!
//! Central [`SettingsManager`] for all [`super::app::GuiSettings`] I/O.
//!
//! Handles loading, saving, importing, and exporting settings.
//! The default persistence path is `%APPDATA%\MagicX\RAM Cleaner\settings.json`.
//! Import and export open native Win32 file-picker dialogs (COMDLG32).
//!
//! Gracefully falls back to [`Default`] on any read error so a missing or
//! corrupted file never prevents the app from starting.

use std::path::{Path, PathBuf};

use super::app::GuiSettings;

// ─── Default Path ─────────────────────────────────────────────────────────────

/// Returns the default settings JSON path: `%APPDATA%\MagicX\RAM Cleaner\settings.json`.
///
/// Returns [`None`] if the `APPDATA` environment variable is absent.
fn default_settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| {
        PathBuf::from(appdata)
            .join("MagicX")
            .join("RAM Cleaner")
            .join("settings.json")
    })
}

// ─── File Dialog Helpers ──────────────────────────────────────────────────────

/// Encode a `&str` as a null-terminated UTF-16 `Vec<u16>`.
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Open a native Win32 **Save File** dialog pre-filtered to `*.json`.
///
/// Returns the chosen path, or [`None`] if the user cancels.
fn pick_save_path(default_name: &str) -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStringExt;

    use windows_sys::Win32::UI::Controls::Dialogs::{
        GetSaveFileNameW, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OPENFILENAMEW,
    };

    // Pairs separated by single NUL; to_wide appends a final NUL → double-NUL terminator.
    let filter = to_wide("JSON settings (*.json)\0*.json\0All files (*.*)\0*.*\0");
    let title = to_wide("Export Settings \u{2014} MagicX RAM Cleaner");
    let ext = to_wide("json");

    // Pre-populate the filename buffer with the suggested default.
    let mut file_buf = vec![0u16; 512];
    for (i, c) in default_name.encode_utf16().take(260).enumerate() {
        file_buf[i] = c;
    }

    // SAFETY: OPENFILENAMEW is a plain C struct. Zero-initialisation sets all
    // pointers to null (unused fields) and integers to 0, which is the
    // MSDN-recommended initialisation pattern. All pointer fields assigned below
    // reference local data that remains valid for the entire duration of the call.
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file_buf.as_mut_ptr();
    ofn.nMaxFile = file_buf.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.lpstrDefExt = ext.as_ptr();
    ofn.Flags = OFN_OVERWRITEPROMPT | OFN_HIDEREADONLY | OFN_NOCHANGEDIR;

    // SAFETY: `ofn` fields satisfy the GetSaveFileNameW contract; `file_buf`
    // is a mutable, correctly sized buffer that lives for the duration of the call.
    let ok = unsafe { GetSaveFileNameW(&raw mut ofn) };
    if ok == 0 {
        return None;
    }

    let end = file_buf
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(file_buf.len());
    Some(PathBuf::from(OsString::from_wide(&file_buf[..end])))
}

/// Open a native Win32 **Open File** dialog pre-filtered to `*.json`.
///
/// Returns the chosen path, or [`None`] if the user cancels.
fn pick_open_path() -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStringExt;

    use windows_sys::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST,
        OPENFILENAMEW,
    };

    let filter = to_wide("JSON settings (*.json)\0*.json\0All files (*.*)\0*.*\0");
    let title = to_wide("Import Settings \u{2014} MagicX RAM Cleaner");
    let mut file_buf = vec![0u16; 512];

    // SAFETY: Same contract as pick_save_path — zero-initialised C struct, all
    // pointer fields reference local buffers live for the duration of the call.
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = file_buf.as_mut_ptr();
    ofn.nMaxFile = file_buf.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY | OFN_NOCHANGEDIR;

    // SAFETY: `ofn` fields satisfy the GetOpenFileNameW contract; `file_buf`
    // is a mutable, correctly sized buffer that lives for the duration of the call.
    let ok = unsafe { GetOpenFileNameW(&raw mut ofn) };
    if ok == 0 {
        return None;
    }

    let end = file_buf
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(file_buf.len());
    Some(PathBuf::from(OsString::from_wide(&file_buf[..end])))
}

// ─── Low-Level I/O ────────────────────────────────────────────────────────────

/// Deserialise [`GuiSettings`] from a JSON file.
fn read_settings_file(path: &Path) -> Result<GuiSettings, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("Cannot read file: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("Invalid settings file: {e}"))
}

/// Serialise `settings` as pretty JSON to `path`, creating parent directories.
fn write_settings_file(path: &Path, settings: &GuiSettings) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return Err(format!("Cannot create directory: {}", path.display()));
    }

    let json =
        serde_json::to_string_pretty(settings).map_err(|e| format!("Serialisation error: {e}"))?;

    std::fs::write(path, json).map_err(|e| format!("Cannot write file: {e}"))
}

// ─── Settings Manager ─────────────────────────────────────────────────────────

/// Central manager for all settings persistence operations.
///
/// A stateless unit struct — every method takes settings by reference or
/// returns new values. This is the single place to add versioning,
/// migration, or multi-profile logic in the future.
pub struct SettingsManager;

impl SettingsManager {
    /// Load [`GuiSettings`] from the default `%APPDATA%` path.
    ///
    /// Returns [`GuiSettings::default`] if the file is absent, unreadable,
    /// or contains incompatible JSON. Unknown fields are silently ignored so
    /// existing files survive schema additions across app versions.
    pub fn load() -> GuiSettings {
        let Some(path) = default_settings_path() else {
            return GuiSettings::default();
        };

        let Ok(content) = std::fs::read_to_string(&path) else {
            return GuiSettings::default();
        };

        serde_json::from_str(&content).unwrap_or_default()
    }

    /// Save `settings` to the default `%APPDATA%` path.
    ///
    /// I/O errors are silently discarded — a failed write must not surface
    /// to the user during normal app shutdown.
    pub fn save(settings: &GuiSettings) {
        let Some(path) = default_settings_path() else {
            return;
        };

        // Named binding avoids `let_underscore_drop`; error is intentionally ignored.
        let _write_result = write_settings_file(&path, settings);
    }

    /// Export `settings` to a user-chosen file via a native Save dialog.
    ///
    /// - `Ok(Some(path))` — exported successfully; `path` is where the file was written.
    /// - `Ok(None)` — user cancelled the dialog.
    /// - `Err(msg)` — the user confirmed a path but the write failed.
    pub fn export(settings: &GuiSettings) -> Result<Option<PathBuf>, String> {
        let Some(path) = pick_save_path("magicx-settings.json") else {
            return Ok(None);
        };
        write_settings_file(&path, settings)?;
        Ok(Some(path))
    }

    /// Import settings from a user-chosen file via a native Open dialog.
    ///
    /// - `Ok(Some(settings))` — loaded successfully from the chosen file.
    /// - `Ok(None)` — user cancelled the dialog.
    /// - `Err(msg)` — file was chosen but could not be read or parsed.
    pub fn import() -> Result<Option<GuiSettings>, String> {
        let Some(path) = pick_open_path() else {
            return Ok(None);
        };
        read_settings_file(&path).map(Some)
    }
}
