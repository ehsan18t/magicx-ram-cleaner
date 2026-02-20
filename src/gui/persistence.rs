//! # Settings Manager
//!
//! Central [`SettingsManager`] for all [`super::app::GuiSettings`] I/O.
//!
//! Handles loading, saving, importing, exporting, and Windows system
//! integration (autostart registry entry).
//!
//! The default persistence path is `settings.json` next to the running executable.
//! Import and export open native Win32 file-picker dialogs (COMDLG32).
//! Autostart writes/removes a value under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` via the registry API.
//!
//! Gracefully falls back to [`Default`] on any read error so a missing or
//! corrupted file never prevents the app from starting.

use std::path::{Path, PathBuf};

use super::app::GuiSettings;

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

// ─── File Dialog Helpers ──────────────────────────────────────────────────────

/// Shorthand re-export of [`crate::stats::to_wide`] for this module.
use crate::stats::to_wide;

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
    /// Load [`GuiSettings`] from the default exe-directory path.
    ///
    /// Returns [`GuiSettings::default`] if the file is absent, unreadable,
    /// or contains incompatible JSON. Unknown fields are silently ignored so
    /// existing files survive schema additions across app versions.
    pub fn load() -> GuiSettings {
        let path = default_settings_path();

        let Ok(content) = std::fs::read_to_string(&path) else {
            return GuiSettings::default();
        };

        serde_json::from_str(&content).unwrap_or_default()
    }

    /// Save `settings` to the default exe-directory path.
    ///
    /// I/O errors are silently discarded — a failed write must not surface
    /// to the user during normal app shutdown.
    pub fn save(settings: &GuiSettings) {
        let path = default_settings_path();

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

    /// Write or remove the Windows autostart registry entry for this executable.
    ///
    /// When `enabled` is `true`, creates (or updates) the value
    /// `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\MagicX RAM Cleaner`
    /// pointing to the running executable's full path.
    ///
    /// When `enabled` is `false`, removes the value if it exists.
    ///
    /// # Errors
    ///
    /// Returns an error string if the registry key cannot be opened or the
    /// value operation fails.
    pub fn set_autostart(enabled: bool) -> Result<(), String> {
        use windows_sys::Win32::System::Registry::{
            HKEY_CURRENT_USER, KEY_WRITE, REG_SZ, RegCloseKey, RegDeleteValueW, RegOpenKeyExW,
            RegSetValueExW,
        };

        const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
        const VALUE_NAME: &str = "MagicX RAM Cleaner";

        let subkey_wide = to_wide(RUN_SUBKEY);
        let value_wide = to_wide(VALUE_NAME);
        let mut hkey: windows_sys::Win32::System::Registry::HKEY = std::ptr::null_mut();

        // SAFETY: `RegOpenKeyExW` is a standard Win32 registry call.
        // `hkey` is zero-initialised and receives a valid handle on success.
        // All wide-string slices are null-terminated and live for the full call.
        let rc = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey_wide.as_ptr(),
                0,
                KEY_WRITE,
                &raw mut hkey,
            )
        };
        if rc != 0 {
            return Err(format!(
                "RegOpenKeyExW failed (code {rc}): cannot access autostart registry key"
            ));
        }

        let result = if enabled {
            let exe = std::env::current_exe()
                .map_err(|e| format!("Cannot resolve executable path: {e}"))?;
            let exe_str = exe
                .to_str()
                .ok_or_else(|| "Executable path contains non-UTF-8 characters".to_owned())?;
            let data = to_wide(exe_str);
            // REG_SZ value length in bytes, including the null terminator.
            let byte_count = (data.len() * 2) as u32;
            // SAFETY: `data` is a valid null-terminated UTF-16 buffer.
            // `byte_count` correctly encodes its byte length for the REG_SZ type.
            let w = unsafe {
                RegSetValueExW(
                    hkey,
                    value_wide.as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr().cast(),
                    byte_count,
                )
            };
            if w == 0 {
                Ok(())
            } else {
                Err(format!("RegSetValueExW failed (code {w})"))
            }
        } else {
            // SAFETY: `value_wide` is a valid null-terminated UTF-16 string.
            let w = unsafe { RegDeleteValueW(hkey, value_wide.as_ptr()) };
            // ERROR_FILE_NOT_FOUND (2) — value was never set; treat as success.
            if w == 0 || w == 2 {
                Ok(())
            } else {
                Err(format!("RegDeleteValueW failed (code {w})"))
            }
        };

        // SAFETY: `hkey` is a valid open registry handle obtained above.
        unsafe { RegCloseKey(hkey) };
        result
    }
}
