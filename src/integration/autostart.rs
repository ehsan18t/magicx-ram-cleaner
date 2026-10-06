//! Start the app when the user signs in, as an elevated Task Scheduler
//! logon task.
//!
//! A `HKCU\...\Run` value cannot be used: Windows silently refuses to
//! launch `requireAdministrator` executables from it at logon. Older
//! versions wrote one anyway, so it is removed on every change.

use anyhow::{Context, Result};

use crate::platform::registry::{self, Hive};
use crate::platform::task_scheduler;
use crate::strings;

/// Task Scheduler task name used for the autostart logon task.
const AUTOSTART_TASK_NAME: &str = strings::APP_NAME;

/// Registry key of the legacy `HKCU\...\Run` autostart value.
const LEGACY_RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Create or remove the autostart logon task for this executable.
///
/// When `enabled`, registers (or replaces) a task named after the app that
/// launches the running executable with highest privileges when the current
/// user signs in; otherwise deletes that task if it exists.
///
/// In both cases the legacy `HKCU\...\Run` value written by older versions
/// is removed on a best-effort basis. Windows never honours it for this
/// elevated app, so failing to remove it must not make the call fail (the
/// caller would then show a state that contradicts the real task).
///
/// # Errors
///
/// Fails (with the `schtasks` output in the message) if the task cannot be
/// created or deleted.
pub fn set_enabled(enabled: bool) -> Result<()> {
    if enabled {
        create_task()?;
    } else {
        task_scheduler::delete(AUTOSTART_TASK_NAME)?;
    }
    // Named binding avoids `let_underscore_drop`; removal is best-effort.
    let _legacy = registry::delete_value(Hive::CurrentUser, LEGACY_RUN_KEY, strings::APP_NAME);
    Ok(())
}

/// Whether the autostart logon task currently exists.
#[must_use]
pub fn is_enabled() -> bool {
    task_scheduler::exists(AUTOSTART_TASK_NAME)
}

/// Create (or replace) the autostart logon task for the running executable.
fn create_task() -> Result<()> {
    let exe = canonical_exe_path()?;

    let user_name = std::env::var("USERNAME").context("USERNAME is not set")?;
    let user = match std::env::var("USERDOMAIN") {
        Ok(domain) if !domain.is_empty() => format!("{domain}\\{user_name}"),
        _ => user_name,
    };

    task_scheduler::register_from_xml(AUTOSTART_TASK_NAME, &autostart_task_xml(&user, &exe))
}

/// Canonical path of the running executable, as Task Scheduler expects it.
fn canonical_exe_path() -> Result<String> {
    let exe = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .context("Cannot resolve executable path")?;
    let exe = exe
        .to_str()
        .context("Executable path contains non-UTF-8 characters")?;
    Ok(plain_path(exe).to_owned())
}

/// Strip the `\\?\` prefix that `canonicalize` adds, for drive-letter paths
/// only (UNC and other verbatim paths keep it).
fn plain_path(path: &str) -> &str {
    path.strip_prefix(r"\\?\")
        .filter(|rest| rest.as_bytes().get(1) == Some(&b':'))
        .unwrap_or(path)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_escape_escapes_all_special_characters() {
        assert_eq!(
            xml_escape(r#"a&b<c>d"e'f"#),
            "a&amp;b&lt;c&gt;d&quot;e&apos;f"
        );
    }

    #[test]
    fn task_xml_escapes_values_and_keeps_required_settings() {
        let xml = autostart_task_xml(r"PC\o'brien", r"C:\A&B\magicx.exe");
        assert!(xml.contains(r"<UserId>PC\o&apos;brien</UserId>"));
        assert!(xml.contains(r"<Command>C:\A&amp;B\magicx.exe</Command>"));
        // schtasks defaults would kill the app after 72 h and skip battery power.
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
        assert!(xml.contains("<RunLevel>HighestAvailable</RunLevel>"));
        assert!(xml.starts_with(r#"<?xml version="1.0" encoding="UTF-16"?>"#));
    }

    #[test]
    fn plain_path_strips_verbatim_prefix_for_drive_paths_only() {
        assert_eq!(plain_path(r"\\?\C:\Tools\app.exe"), r"C:\Tools\app.exe");
        assert_eq!(
            plain_path(r"\\?\UNC\server\share\app.exe"),
            r"\\?\UNC\server\share\app.exe"
        );
        assert_eq!(plain_path(r"C:\app.exe"), r"C:\app.exe");
    }
}
