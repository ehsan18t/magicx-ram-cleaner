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
    let title = to_wide(strings::gui::persistence::EXPORT_TITLE);
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
    let title = to_wide(strings::gui::persistence::IMPORT_TITLE);
    let mut file_buf = vec![0u16; 512];

    // SAFETY: Same contract as pick_save_path - zero-initialised C struct, all
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

/// `CREATE_NO_WINDOW` process creation flag: keeps `schtasks.exe` from
/// flashing a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Run `schtasks.exe` with `args` without showing a console window.
///
/// Returns `Ok(())` on a zero exit status, or an error string that includes
/// the `schtasks` stderr (falling back to stdout) otherwise.
fn run_schtasks(args: &[&std::ffi::OsStr]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    // Use the absolute System32 path so an elevated process never runs a
    // `schtasks.exe` planted earlier on PATH.
    let exe = std::env::var_os("SystemRoot").map_or_else(
        || PathBuf::from("schtasks.exe"),
        |root| PathBuf::from(root).join("System32").join("schtasks.exe"),
    );

    let output = std::process::Command::new(exe)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Cannot run schtasks.exe: {e}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = if stderr.trim().is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    } else {
        stderr.trim().to_owned()
    };
    Err(format!("schtasks.exe failed ({}): {detail}", output.status))
}

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

    // schtasks expects the XML file in UTF-16 LE with a byte-order mark,
    // matching the encoding declared in the XML prolog.
    let xml = autostart_task_xml(&user, &exe);
    let mut bytes = Vec::with_capacity(2 + xml.len() * 2);
    bytes.extend_from_slice(&[0xFF, 0xFE]);
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }

    // The XML is read by schtasks on behalf of this elevated process, so it
    // must live where a non-elevated process of the same user cannot swap it
    // (which would register an arbitrary elevated logon task). See
    // `PrivateTempDir::create` for why that rules out the user's %TEMP%.
    let dir = PrivateTempDir::create()?;
    let xml_path = dir.path.join("task.xml");
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&xml_path)
            .map_err(|e| format!("Cannot create task XML: {e}"))?;
        file.write_all(&bytes)
            .map_err(|e| format!("Cannot write task XML: {e}"))?;
    }

    // `dir` removes the file and directory when dropped, on every path.
    run_schtasks(&[
        "/Create".as_ref(),
        "/TN".as_ref(),
        AUTOSTART_TASK_NAME.as_ref(),
        "/XML".as_ref(),
        xml_path.as_os_str(),
        "/F".as_ref(),
    ])
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

/// A freshly created temporary directory that only `SYSTEM` and the
/// Administrators group can access. Removed with its contents on drop.
struct PrivateTempDir {
    /// Full path of the directory.
    path: PathBuf,
}

impl PrivateTempDir {
    /// Create a new uniquely named directory under `%SystemRoot%\Temp` with a
    /// protected DACL. Fails if the directory already exists; it is never
    /// reused.
    ///
    /// The user's own `%TEMP%` is not safe even with a protected DACL: the
    /// user owns that folder, so any of their non-elevated processes can
    /// rename our directory away and plant a look-alike. In the Windows temp
    /// folder standard users may create entries but not delete or rename
    /// anyone else's. The Windows directory is read from the API rather than
    /// the environment, which a non-elevated launcher controls.
    fn create() -> Result<Self, String> {
        use std::hash::{BuildHasher, Hasher};

        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
        use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;

        // Protected DACL (no inheritance from %TEMP%): full access for
        // SYSTEM and Administrators only, inherited by the file inside.
        const SDDL: &str = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";

        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u32(std::process::id());
        if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            hasher.write_u128(now.as_nanos());
        }
        let path = windows_temp_dir()?.join(format!("magicx-{:016x}", hasher.finish()));

        let sddl_wide = to_wide(SDDL);
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: `sddl_wide` is a valid null-terminated UTF-16 string and
        // `descriptor` receives a LocalAlloc'd descriptor on success, freed below.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl_wide.as_ptr(),
                SDDL_REVISION_1,
                &raw mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(format!(
                "Cannot build security descriptor: {}",
                std::io::Error::last_os_error()
            ));
        }

        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let path_wide = to_wide(&path.to_string_lossy());
        // SAFETY: `path_wide` is null-terminated and `sa` points at a valid
        // descriptor for the duration of the call. CreateDirectoryW fails
        // (ERROR_ALREADY_EXISTS) rather than reusing an existing directory.
        let created = unsafe { CreateDirectoryW(path_wide.as_ptr(), &raw const sa) };
        let create_err = std::io::Error::last_os_error();
        // SAFETY: `descriptor` was allocated by the conversion call above and
        // is not used after this point.
        unsafe { LocalFree(descriptor) };

        if created == 0 {
            return Err(format!(
                "Cannot create private temp directory: {create_err}"
            ));
        }
        Ok(Self { path })
    }
}

/// `%SystemRoot%\Temp`, resolved through `GetSystemWindowsDirectoryW`.
fn windows_temp_dir() -> Result<PathBuf, String> {
    use std::os::windows::ffi::OsStringExt;

    use windows_sys::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;

    let mut buf = [0u16; 260];
    // SAFETY: `buf` is a writable buffer of the stated length; the call writes
    // at most that many UTF-16 units and returns the length written.
    let len = unsafe { GetSystemWindowsDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if len == 0 || len >= buf.len() {
        return Err(format!(
            "Cannot locate the Windows directory: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(PathBuf::from(std::ffi::OsString::from_wide(&buf[..len])).join("Temp"))
}

impl Drop for PrivateTempDir {
    fn drop(&mut self) {
        // Named binding avoids `let_underscore_drop`; cleanup is best-effort.
        let _cleanup = std::fs::remove_dir_all(&self.path);
    }
}

/// Delete the autostart logon task. A missing task counts as success.
fn delete_autostart_task() -> Result<(), String> {
    let deleted = run_schtasks(&[
        "/Delete".as_ref(),
        "/TN".as_ref(),
        AUTOSTART_TASK_NAME.as_ref(),
        "/F".as_ref(),
    ]);
    // A failed delete is only fine when the task is really gone. Checking
    // with a query (exit status) avoids parsing localised schtasks text, and
    // a delete is always attempted, so a failing query cannot hide a task.
    match deleted {
        Err(_) if !SettingsManager::is_autostart_enabled() => Ok(()),
        other => other,
    }
}

/// Owned registry key handle that is closed on drop.
struct RegKeyGuard(windows_sys::Win32::System::Registry::HKEY);

impl Drop for RegKeyGuard {
    fn drop(&mut self) {
        // SAFETY: the guard is only constructed around a handle returned by a
        // successful `RegOpenKeyExW` call, and it is closed exactly once here.
        unsafe { windows_sys::Win32::System::Registry::RegCloseKey(self.0) };
    }
}

/// Remove the legacy `HKCU\...\Run` autostart value written by older versions.
///
/// A missing key or value counts as success.
fn remove_legacy_run_entry() -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, KEY_SET_VALUE, RegDeleteValueW, RegOpenKeyExW,
    };

    /// `ERROR_FILE_NOT_FOUND`: the key or value does not exist.
    const NOT_FOUND: u32 = 2;
    const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

    let subkey_wide = to_wide(RUN_SUBKEY);
    let value_wide = to_wide(strings::APP_NAME);
    let mut hkey: windows_sys::Win32::System::Registry::HKEY = std::ptr::null_mut();

    // SAFETY: `RegOpenKeyExW` is a standard Win32 registry call.
    // `hkey` is zero-initialised and receives a valid handle on success.
    // All wide-string slices are null-terminated and live for the full call.
    let rc = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey_wide.as_ptr(),
            0,
            KEY_SET_VALUE,
            &raw mut hkey,
        )
    };
    if rc == NOT_FOUND {
        return Ok(());
    }
    if rc != 0 {
        return Err(format!(
            "RegOpenKeyExW failed (code {rc}): cannot access legacy Run registry key"
        ));
    }
    let key = RegKeyGuard(hkey);

    // SAFETY: `key.0` is a valid open handle and `value_wide` is a valid
    // null-terminated UTF-16 string.
    let w = unsafe { RegDeleteValueW(key.0, value_wide.as_ptr()) };
    if w == 0 || w == NOT_FOUND {
        Ok(())
    } else {
        Err(format!(
            "RegDeleteValueW failed (code {w}) on legacy Run value"
        ))
    }
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
        let Some(path) = pick_save_path("magicx-settings.json") else {
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
        let Some(path) = pick_open_path() else {
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
            delete_autostart_task()?;
        }
        // Named binding avoids `let_underscore_drop`; removal is best-effort.
        let _legacy = remove_legacy_run_entry();
        Ok(())
    }

    /// Whether the autostart logon task currently exists.
    pub fn is_autostart_enabled() -> bool {
        run_schtasks(&[
            "/Query".as_ref(),
            "/TN".as_ref(),
            AUTOSTART_TASK_NAME.as_ref(),
        ])
        .is_ok()
    }
}
