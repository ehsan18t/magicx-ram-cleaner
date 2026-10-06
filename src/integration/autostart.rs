//! Start the app at sign-in, as an elevated Task Scheduler logon task.
//!
//! There is one task per machine, run for the Administrators group: once
//! any administrator turns autostart on, the copy that did it starts for
//! every administrator account that signs in, including accounts created
//! later. That matches the portable model, where everyone who runs a copy
//! shares its settings. Standard accounts are not included because Windows
//! cannot run this admin-only app at their sign-in.
//!
//! The task is only written when someone flips the switch. Reading it (to
//! show the switch) never changes it, so starting the app cannot take over
//! or delete another copy's autostart, or undo edits made in Task Scheduler.
//!
//! A `HKCU\...\Run` value cannot be used: Windows silently refuses to
//! launch `requireAdministrator` executables from it at logon. Older
//! versions wrote one anyway, so it is removed on every change.

use anyhow::{Context, Result};

use crate::ids::{AUTOSTART_TASK_NAME, LEGACY_RUN_VALUE};
use crate::platform::registry::{self, Hive};
use crate::platform::task_scheduler;
use crate::strings;

/// Registry key of the legacy `HKCU\...\Run` autostart value.
const LEGACY_RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// SID of the built-in Administrators group, the task's principal.
const ADMINISTRATORS_SID: &str = "S-1-5-32-544";

/// Argument the task passes so the app starts hidden in the tray.
pub const TRAY_ARG: &str = "--tray";

/// What the autostart task currently does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutostartState {
    /// There is no autostart task.
    Off,
    /// The task starts this copy of the app.
    ThisCopy,
    /// The task starts a different copy, at this path. Turning autostart on
    /// here moves it to this copy.
    OtherCopy(String),
}

impl AutostartState {
    /// Whether autostart is on for this copy (what the switch shows).
    #[must_use]
    pub const fn is_on(&self) -> bool {
        matches!(self, Self::ThisCopy)
    }
}

/// Read what the autostart task does, without changing it.
///
/// # Errors
///
/// Fails if `schtasks.exe` cannot be run or the executable path cannot be
/// resolved.
pub fn state() -> Result<AutostartState> {
    Ok(state_of(
        task_scheduler::query_xml(AUTOSTART_TASK_NAME)?,
        &canonical_exe_path()?,
    ))
}

/// Read the task's state, upgrading this copy's task if an older version
/// wrote it.
///
/// An older task runs for one user only or lacks the tray argument; it is
/// rewritten in the current form. That one-time upgrade is the only write
/// this makes; any other task is left exactly as it is.
///
/// # Errors
///
/// Fails if the state cannot be read or the upgrade cannot be written.
pub fn state_with_upgrade() -> Result<AutostartState> {
    let exe = canonical_exe_path()?;
    let xml = task_scheduler::query_xml(AUTOSTART_TASK_NAME)?;
    let state = state_of(xml.as_deref().map(str::to_owned), &exe);
    if state == AutostartState::ThisCopy && xml.as_deref().is_some_and(is_outdated) {
        create_task(&exe)?;
    }
    Ok(state)
}

/// Turn autostart on for this copy, or off.
///
/// On creates (or replaces) the task so it starts the running executable
/// for every administrator; off deletes the task, whichever copy it starts.
/// In both cases the legacy `HKCU\...\Run` value written by older versions
/// is removed on a best-effort basis.
///
/// # Errors
///
/// Fails (with the `schtasks` output in the message) if the task cannot be
/// created or deleted.
pub fn set_enabled(enabled: bool) -> Result<()> {
    if enabled {
        create_task(&canonical_exe_path()?)?;
    } else {
        task_scheduler::delete(AUTOSTART_TASK_NAME)?;
    }
    // Named binding avoids `let_underscore_drop`; removal is best-effort.
    let _legacy = registry::delete_value(Hive::CurrentUser, LEGACY_RUN_KEY, LEGACY_RUN_VALUE);
    Ok(())
}

/// Classify the task definition `xml` (`None` = no task) against this
/// copy's executable path `exe`.
fn state_of(xml: Option<String>, exe: &str) -> AutostartState {
    let Some(xml) = xml else {
        return AutostartState::Off;
    };
    match element_text(&xml, "Command") {
        Some(command) if same_path(&command, exe) => AutostartState::ThisCopy,
        Some(command) => AutostartState::OtherCopy(command),
        None => AutostartState::OtherCopy(String::new()),
    }
}

/// Whether a task definition predates the current form: tied to one user
/// instead of the Administrators group, or missing the tray argument.
fn is_outdated(xml: &str) -> bool {
    element_text(xml, "GroupId").as_deref() != Some(ADMINISTRATORS_SID)
        || element_text(xml, "Arguments").as_deref() != Some(TRAY_ARG)
}

/// Whether two executable paths name the same file. Windows paths are
/// case-insensitive; surrounding quotes are ignored.
fn same_path(a: &str, b: &str) -> bool {
    let normal = |path: &str| path.trim().trim_matches('"').to_lowercase();
    normal(a) == normal(b)
}

/// The unescaped text of the first `<name>...</name>` element in `xml`.
fn element_text(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = xml.find(&open)? + open.len();
    let end = start + xml[start..].find(&close)?;
    Some(xml_unescape(&xml[start..end]))
}

/// Create (or replace) the autostart task for executable `exe`.
fn create_task(exe: &str) -> Result<()> {
    task_scheduler::register_from_xml(AUTOSTART_TASK_NAME, &autostart_task_xml(exe))
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

/// Undo [`xml_escape`].
fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Build the Task Scheduler XML for the autostart logon task.
///
/// The logon trigger names no user, so it fires for every sign-in, and the
/// principal is the Administrators group with the highest privileges, so it
/// runs elevated for every administrator. Explicit settings replace plain
/// `schtasks /SC ONLOGON` flags, whose defaults would stop the app after 72
/// hours and refuse to start it on battery power.
fn autostart_task_xml(exe: &str) -> String {
    let app = xml_escape(strings::APP_NAME);
    let exe = xml_escape(exe);
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Starts {app} when an administrator signs in.</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <GroupId>{ADMINISTRATORS_SID}</GroupId>
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
      <Arguments>{TRAY_ARG}</Arguments>
    </Exec>
  </Actions>
</Task>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXE: &str = r"C:\Tools\magicx.exe";

    #[test]
    fn xml_escape_round_trips_all_special_characters() {
        let raw = r#"a&b<c>d"e'f"#;
        assert_eq!(xml_escape(raw), "a&amp;b&lt;c&gt;d&quot;e&apos;f");
        assert_eq!(xml_unescape(&xml_escape(raw)), raw);
    }

    #[test]
    fn task_xml_runs_for_every_administrator_and_keeps_required_settings() {
        let xml = autostart_task_xml(r"C:\A&B\magicx.exe");
        assert!(xml.contains(r"<Command>C:\A&amp;B\magicx.exe</Command>"));
        assert!(xml.contains("<GroupId>S-1-5-32-544</GroupId>"));
        assert!(!xml.contains("<UserId>"), "no single user");
        assert!(xml.contains("<Arguments>--tray</Arguments>"));
        // schtasks defaults would kill the app after 72 h and skip battery power.
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
        assert!(xml.contains("<RunLevel>HighestAvailable</RunLevel>"));
        assert!(xml.starts_with(r#"<?xml version="1.0" encoding="UTF-16"?>"#));
    }

    #[test]
    fn the_state_tells_this_copy_from_another() {
        assert_eq!(state_of(None, EXE), AutostartState::Off);
        let mine = autostart_task_xml(&EXE.to_uppercase());
        assert_eq!(state_of(Some(mine), EXE), AutostartState::ThisCopy);
        let other = autostart_task_xml(r"D:\Other\magicx.exe");
        assert_eq!(
            state_of(Some(other), EXE),
            AutostartState::OtherCopy(r"D:\Other\magicx.exe".to_owned())
        );
    }

    #[test]
    fn tasks_from_older_versions_are_recognised() {
        assert!(!is_outdated(&autostart_task_xml(EXE)));
        let per_user = autostart_task_xml(EXE).replace(
            "<GroupId>S-1-5-32-544</GroupId>",
            r"<UserId>PC\me</UserId><LogonType>InteractiveToken</LogonType>",
        );
        assert!(is_outdated(&per_user));
        let no_tray = autostart_task_xml(EXE).replace("<Arguments>--tray</Arguments>", "");
        assert!(is_outdated(&no_tray));
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

/// Writes the real autostart task, so it only runs on request:
/// `cargo test -- --ignored autostart`.
#[cfg(test)]
mod machine_tests {
    use super::*;

    #[test]
    #[ignore = "creates and deletes the real autostart task"]
    fn autostart_round_trip_on_this_machine() {
        set_enabled(true).expect("creates the task");
        assert_eq!(state().expect("reads"), AutostartState::ThisCopy);
        let xml = task_scheduler::query_xml(AUTOSTART_TASK_NAME)
            .expect("queries")
            .expect("exists");
        assert!(!is_outdated(&xml), "task is in the current form: {xml}");
        assert_eq!(
            state_with_upgrade().expect("reads"),
            AutostartState::ThisCopy
        );

        // A per-user task from an older version is upgraded in place.
        let user = crate::platform::identity::current_account().expect("account");
        let old = autostart_task_xml(&canonical_exe_path().expect("exe"))
            .replace(
                "<GroupId>S-1-5-32-544</GroupId>",
                &format!(
                    "<UserId>{}</UserId><LogonType>InteractiveToken</LogonType>",
                    xml_escape(&user)
                ),
            )
            .replace("<Arguments>--tray</Arguments>", "");
        task_scheduler::register_from_xml(AUTOSTART_TASK_NAME, &old).expect("old task");
        assert_eq!(
            state_with_upgrade().expect("reads"),
            AutostartState::ThisCopy
        );
        let upgraded = task_scheduler::query_xml(AUTOSTART_TASK_NAME)
            .expect("queries")
            .expect("exists");
        assert!(!is_outdated(&upgraded), "upgraded: {upgraded}");

        set_enabled(false).expect("deletes the task");
        assert_eq!(state().expect("reads"), AutostartState::Off);
    }
}
