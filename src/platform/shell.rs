//! Launching things through the desktop shell on the user's behalf.

use anyhow::{Result, bail};

/// Open `url` in the default browser as the signed-in user, without this
/// process's administrator rights.
///
/// The app always runs elevated, and opening a link the usual way would
/// start the browser elevated as well. Instead this borrows the token of the
/// desktop shell (Explorer), which runs unelevated, and starts the URL handler
/// (`rundll32 url.dll,FileProtocolHandler`) with it.
///
/// Only plain `https://` URLs are accepted, so the URL cannot break out of
/// the handler's command line.
///
/// # Errors
///
/// Fails if the URL is rejected, no shell is running, or the process cannot
/// be created. Callers may fall back to a normal (elevated) launch.
pub fn open_url_unelevated(url: &str) -> Result<()> {
    use windows_sys::Win32::Security::{
        DuplicateTokenEx, SecurityImpersonation, TOKEN_ADJUST_DEFAULT, TOKEN_ADJUST_SESSIONID,
        TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_QUERY, TokenPrimary,
    };
    use windows_sys::Win32::System::Threading::{
        CreateProcessWithTokenW, OpenProcess, OpenProcessToken, PROCESS_INFORMATION,
        PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId};

    use crate::platform::{handle::HandleGuard, wide::to_wide};

    if !url.starts_with("https://")
        || url
            .chars()
            .any(|c| c == '"' || c.is_whitespace() || c.is_control())
    {
        bail!("refusing to open unexpected URL '{url}'");
    }

    // SAFETY: GetShellWindow has no preconditions; null means no shell.
    let shell = unsafe { GetShellWindow() };
    if shell.is_null() {
        bail!("no desktop shell is running");
    }
    let mut shell_pid = 0u32;
    // SAFETY: `shell` is a window handle and `shell_pid` a valid out pointer.
    unsafe { GetWindowThreadProcessId(shell, &raw mut shell_pid) };
    if shell_pid == 0 {
        bail!("cannot identify the desktop shell process");
    }

    // SAFETY: OpenProcess has no memory-safety preconditions; the handle (or
    // null) is owned by the guard.
    let shell_process =
        HandleGuard::new(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, shell_pid) });
    if shell_process.raw().is_null() {
        bail!("cannot open the desktop shell process");
    }

    let mut shell_token = std::ptr::null_mut();
    // SAFETY: `shell_process` is a valid process handle; the token handle
    // is written to a valid out pointer and owned by a guard right after.
    if unsafe { OpenProcessToken(shell_process.raw(), TOKEN_DUPLICATE, &raw mut shell_token) } == 0
    {
        bail!("cannot open the desktop shell token");
    }
    let shell_token = HandleGuard::new(shell_token);

    let mut primary = std::ptr::null_mut();
    // SAFETY: Duplicates a valid token handle into a new primary token,
    // written to a valid out pointer and owned by a guard right after.
    let duplicated = unsafe {
        DuplicateTokenEx(
            shell_token.raw(),
            TOKEN_QUERY
                | TOKEN_DUPLICATE
                | TOKEN_ASSIGN_PRIMARY
                | TOKEN_ADJUST_DEFAULT
                | TOKEN_ADJUST_SESSIONID,
            std::ptr::null(),
            SecurityImpersonation,
            TokenPrimary,
            &raw mut primary,
        )
    };
    if duplicated == 0 {
        bail!("cannot duplicate the desktop shell token");
    }
    let primary = HandleGuard::new(primary);

    let rundll32 = super::paths::system_directory()?
        .join("rundll32.exe")
        .to_string_lossy()
        .into_owned();
    let application = to_wide(&rundll32);
    let mut command_line = to_wide(&format!("\"{rundll32}\" url.dll,FileProtocolHandler {url}"));

    // SAFETY: STARTUPINFOW / PROCESS_INFORMATION are plain data; zeroing
    // them and setting `cb` is the documented initialisation.
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    // SAFETY: All pointers reference live, null-terminated buffers or
    // initialised structs; `command_line` is mutable as the API requires.
    // Requires SeImpersonatePrivilege, which Administrators hold.
    let created = unsafe {
        CreateProcessWithTokenW(
            primary.raw(),
            0,
            application.as_ptr(),
            command_line.as_mut_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
            &raw const startup,
            &raw mut info,
        )
    };
    if created == 0 {
        bail!(
            "cannot start the URL handler: {}",
            std::io::Error::last_os_error()
        );
    }
    // The new process runs on its own; only our handles to it are closed.
    drop(HandleGuard::new(info.hThread));
    drop(HandleGuard::new(info.hProcess));
    Ok(())
}
