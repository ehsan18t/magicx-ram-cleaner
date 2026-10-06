//! # Settings Manager
//!
//! Central [`SettingsManager`] for all [`super::app::GuiSettings`] I/O.
//!
//! Handles loading, saving, importing, exporting, and Windows system
//! integration (autostart logon task).
//!
//! The default persistence path is `settings.json` next to the running executable.
//! Import and export open native Win32 file-picker dialogs (COMDLG32).
//! Autostart creates/removes a Task Scheduler logon task via `schtasks.exe`.
//! A `HKCU\...\Run` value cannot be used: Windows silently refuses to launch
//! `requireAdministrator` executables from it at logon.
//!
//! Gracefully falls back to [`Default`] on any read error so a missing or
//! corrupted file never prevents the app from starting.

use std::path::{Path, PathBuf};

use super::app::GuiSettings;
use crate::platform::registry::{self, Hive};
use crate::platform::{dialog, task_scheduler};
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

// ─── Autostart Helpers ────────────────────────────────────────────────────────

/// Task Scheduler task name used for the autostart logon task.
const AUTOSTART_TASK_NAME: &str = strings::APP_NAME;

/// Registry key of the legacy `HKCU\...\Run` autostart value.
const LEGACY_RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Escape the five XML special characters in `s`.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Build the Task Scheduler XML for the autostart logon task.
///
/// Uses explicit settings instead of plain `schtasks /SC ONLOGON` flags,
/// whose defaults would stop the app after 72 hours and refuse to start it
/// on battery power.
fn autostart_task_xml(user: &str, exe: &str) -> String {
    let app = xml_escape(strings::APP_NAME);
    let user = xml_escape(user);
    let exe = xml_escape(exe);
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Starts {app} when you sign in.</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
    <Enabled>true</Enabled>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
    </Exec>
  </Actions>
</Task>
"#
    )
}

/// Create (or replace) the autostart logon task for the running executable.
fn create_autostart_task() -> Result<(), String> {
    let exe = canonical_exe_path()?;

    let user_name = std::env::var("USERNAME").map_err(|_| "USERNAME is not set".to_owned())?;
    let user = match std::env::var("USERDOMAIN") {
        Ok(domain) if !domain.is_empty() => format!("{domain}\\{user_name}"),
        _ => user_name,
    };

    task_scheduler::register_from_xml(AUTOSTART_TASK_NAME, &autostart_task_xml(&user, &exe))
        .map_err(|e| format!("{e:#}"))
}

/// Canonical path of the running executable, without the `\\?\` prefix for
/// ordinary drive-letter paths (Task Scheduler expects a plain path).
fn canonical_exe_path() -> Result<String, String> {
    let exe = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|e| format!("Cannot resolve executable path: {e}"))?;
    let exe_str = exe
        .to_str()
        .ok_or_else(|| "Executable path contains non-UTF-8 characters".to_owned())?;
    let plain = exe_str
        .strip_prefix(r"\\?\")
        .filter(|rest| rest.as_bytes().get(1) == Some(&b':'))
        .unwrap_or(exe_str);
    Ok(plain.to_owned())
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

    /// Create or remove the Windows autostart logon task for this executable.
    ///
    /// When `enabled` is `true`, registers (or replaces) a Task Scheduler task
    /// named after the app that launches the running executable with highest
    /// privileges when the current user signs in.
    ///
    /// When `enabled` is `false`, deletes that task if it exists.
    ///
    /// In both cases the legacy `HKCU\...\Run` value written by older versions
    /// is removed on a best-effort basis. Windows never honours it for this
    /// elevated app, so failing to remove it must not make the call fail
    /// (the caller would then show a state that contradicts the real task).
    ///
    /// # Errors
    ///
    /// Returns an error string (including `schtasks` output) if the task
    /// cannot be created or deleted.
    pub fn set_autostart(enabled: bool) -> Result<(), String> {
        if enabled {
            create_autostart_task()?;
        } else {
            task_scheduler::delete(AUTOSTART_TASK_NAME).map_err(|e| format!("{e:#}"))?;
        }
        // Named binding avoids `let_underscore_drop`; removal is best-effort.
        let _legacy = registry::delete_value(Hive::CurrentUser, LEGACY_RUN_KEY, strings::APP_NAME);
        Ok(())
    }

    /// Whether the autostart logon task currently exists.
    pub fn is_autostart_enabled() -> bool {
        task_scheduler::exists(AUTOSTART_TASK_NAME)
    }
}
