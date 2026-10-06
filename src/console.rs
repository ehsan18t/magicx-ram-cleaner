//! # `MagicX` RAM Cleaner - Console Utilities
//!
//! Windows console platform utilities: on-demand console attach/alloc for CLI
//! mode, ANSI virtual terminal processing, pause-before-exit, and balloon
//! notification display.
//!
//! The binary is a console program (`SUBSYSTEM:CONSOLE`), so cmd and
//! `PowerShell` wait for it and see its exit code. Its manifest sets
//! `consoleAllocationPolicy` to `detached`, so on Windows 11 24H2 and later a
//! launch without a parent console (Explorer, context menu, Task Scheduler)
//! gets no console window at all. Older Windows ignores that setting and
//! creates one; [`release_private_console`] frees it immediately so those
//! launches behave the same apart from a brief flash. For CLI usage,
//! `setup_cli_console()` then attaches to the parent terminal or allocates a
//! fresh console on demand.
//!
//! These are isolated from business logic so that platform-specific console
//! quirks don't leak into the application layer.

use anyhow::{Result, bail};
use colored::Colorize;

// ─── Console mode detection ─────────────────────────────────────────────────

/// How the process was launched.
///
/// Returned by [`setup_cli_console`] so the caller can decide whether
/// to pause before exit (standalone) or return immediately (terminal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleMode {
    /// Sharing a console with a parent shell (launched from cmd / `PowerShell`).
    Terminal,
    /// No console, but the parent handed us redirected standard handles
    /// (pipes or files), e.g. a scheduled task, a service wrapper or a script
    /// host capturing output. Output goes to those handles and nothing waits
    /// for the user.
    Redirected,
    /// Own console allocated for a launch from Explorer (double-clicked the
    /// `.exe`, a shortcut or the Run dialog). The window would vanish on
    /// exit, so the caller should [`pause_before_exit`].
    Standalone,
    /// Own console allocated for any other launcher without a console, such
    /// as Task Scheduler. Nobody may be there to press Enter (or the session
    /// may not even be interactive), so the caller must not pause.
    Allocated,
}

/// Attach to the parent terminal or allocate a fresh console for CLI mode.
///
/// This function:
/// 0. Returns [`ConsoleMode::Terminal`] straight away when the process already
///    shares its parent shell's console; Windows has then set up the standard
///    handles (including any redirection) itself. A private console created
///    by older Windows is released first (see [`release_private_console`]).
/// 1. Records which standard handles the parent passed in (redirected to a
///    file or pipe). Those are always kept, so `status --json > out.json`
///    and pipelines work.
/// 2. Tries `AttachConsole(ATTACH_PARENT_PROCESS)` - succeeds when launched
///    from cmd / `PowerShell` / Windows Terminal.
/// 3. Without a parent console, keeps the inherited handles if there are any
///    ([`ConsoleMode::Redirected`]); otherwise allocates a brand-new console,
///    reported as [`ConsoleMode::Standalone`] only when Explorer launched us.
/// 4. Points every standard handle that was not inherited at the console.
#[must_use]
pub fn setup_cli_console() -> ConsoleMode {
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE,
    };

    release_private_console();
    if shares_parent_console() {
        return ConsoleMode::Terminal;
    }

    let inherited_in = inherited_std_handle(STD_INPUT_HANDLE);
    let inherited_out = inherited_std_handle(STD_OUTPUT_HANDLE);
    let inherited_err = inherited_std_handle(STD_ERROR_HANDLE);

    // SAFETY: AttachConsole is a standard Win32 call. ATTACH_PARENT_PROCESS
    // tells Windows to attach to the console of the process that launched us.
    // Returns non-zero on success (we were launched from a terminal).
    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0;

    let mode = if attached {
        ConsoleMode::Terminal
    } else if inherited_out.is_some() || inherited_err.is_some() {
        // Launched without a console but with captured output: stay
        // console-less so no window pops up and nothing blocks on input.
        ConsoleMode::Redirected
    } else {
        // SAFETY: AllocConsole is a standard Win32 call with no preconditions.
        unsafe {
            AllocConsole();
        }
        if launched_by_explorer() {
            ConsoleMode::Standalone
        } else {
            ConsoleMode::Allocated
        }
    };

    if mode != ConsoleMode::Redirected {
        bind_std_handle(STD_INPUT_HANDLE, inherited_in, b"CONIN$\0");
        bind_std_handle(STD_OUTPUT_HANDLE, inherited_out, b"CONOUT$\0");
        bind_std_handle(STD_ERROR_HANDLE, inherited_err, b"CONOUT$\0");
    }
    mode
}

/// Number of processes attached to this process's console (0 = no console).
fn console_process_count() -> u32 {
    use windows_sys::Win32::System::Console::GetConsoleProcessList;

    let mut pids = [0u32; 2];
    // SAFETY: `pids` is a writable buffer of the stated length. The call
    // returns the total count even when the buffer is too small, and 0 when
    // the process has no console.
    unsafe { GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32) }
}

/// Whether this process shares its console with a parent (started from a
/// terminal). The shell is then waiting for us to exit.
#[must_use]
pub fn shares_parent_console() -> bool {
    console_process_count() >= 2
}

/// Free a console that Windows created just for this process.
///
/// Windows before 11 24H2 ignores the manifest's `detached` console policy
/// and gives every launch without a parent console (Explorer, context menu,
/// Task Scheduler) its own console window. Releasing it right away makes
/// those launches behave like 24H2 apart from a brief flash. A console shared
/// with a parent shell is left alone.
pub fn release_private_console() {
    use windows_sys::Win32::System::Console::{
        FreeConsole, GetConsoleMode, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE, SetStdHandle,
    };

    if console_process_count() != 1 {
        return;
    }
    // Forget standard handles that belong to the console being freed, so
    // later code does not mistake them for handles inherited from a parent.
    for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        let mut mode = 0u32;
        // SAFETY: GetStdHandle/GetConsoleMode only inspect the handle;
        // SetStdHandle stores a null handle.
        unsafe {
            if GetConsoleMode(GetStdHandle(which), &raw mut mode) != 0 {
                SetStdHandle(which, std::ptr::null_mut());
            }
        }
    }
    // SAFETY: FreeConsole detaches this process from its console.
    unsafe {
        FreeConsole();
    }
}

/// Relaunch this executable with the same arguments as a detached process
/// (no console) and return whether the new process started.
///
/// Used when the GUI is started from a terminal: as a console program, the
/// shell would otherwise stay blocked until the window is closed.
#[must_use]
pub fn relaunch_detached() -> bool {
    use std::os::windows::process::CommandExt;

    /// `DETACHED_PROCESS`: the new process gets no console.
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .creation_flags(DETACHED_PROCESS)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// Whether the parent process is `explorer.exe`, i.e. the user started us
/// from the shell (double-click, shortcut, Run dialog) and is watching.
fn launched_by_explorer() -> bool {
    parent_process_name().is_some_and(|name| name.eq_ignore_ascii_case("explorer.exe"))
}

/// Executable name of this process's parent, via a Toolhelp snapshot.
///
/// Returns `None` if the snapshot fails or the parent has already exited.
fn parent_process_name() -> Option<String> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    // SAFETY: Standard documented call; the handle is owned by the guard.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return None;
    }
    let snapshot = crate::stats::HandleGuard::new(raw);

    let mut entries: Vec<(u32, u32, String)> = Vec::new();
    // SAFETY: PROCESSENTRY32W is plain data; zeroing it and setting dwSize is
    // the documented initialisation.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: `snapshot` is a valid Toolhelp snapshot and `entry` is sized.
    let mut more = unsafe { Process32FirstW(snapshot.raw(), &raw mut entry) } != 0;
    while more {
        entries.push((
            entry.th32ProcessID,
            entry.th32ParentProcessID,
            crate::stats::extract_exe_name(&entry.szExeFile),
        ));
        // SAFETY: As above.
        more = unsafe { Process32NextW(snapshot.raw(), &raw mut entry) } != 0;
    }

    let own_pid = std::process::id();
    let parent_pid = entries.iter().find(|e| e.0 == own_pid)?.1;
    entries.into_iter().find(|e| e.0 == parent_pid).map(|e| e.2)
}

/// Return the standard handle `which` if the parent passed in a usable one
/// (a file, pipe or console), or `None` if it is missing.
fn inherited_std_handle(
    which: windows_sys::Win32::System::Console::STD_HANDLE,
) -> Option<windows_sys::Win32::Foundation::HANDLE> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{FILE_TYPE_UNKNOWN, GetFileType};
    use windows_sys::Win32::System::Console::GetStdHandle;

    // SAFETY: GetStdHandle has no preconditions and returns null or
    // INVALID_HANDLE_VALUE when there is no handle.
    let handle = unsafe { GetStdHandle(which) };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return None;
    }
    // SAFETY: GetFileType only inspects the handle; an invalid handle yields
    // FILE_TYPE_UNKNOWN.
    (unsafe { GetFileType(handle) } != FILE_TYPE_UNKNOWN).then_some(handle)
}

/// Set standard handle `which` to `inherited` if the parent provided one,
/// otherwise to the console device `device` (`CONIN$` or `CONOUT$`).
///
/// Re-applying the inherited handle matters because attaching to a console
/// may replace the standard handles with console ones.
fn bind_std_handle(
    which: windows_sys::Win32::System::Console::STD_HANDLE,
    inherited: Option<windows_sys::Win32::Foundation::HANDLE>,
    device: &[u8],
) {
    use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileA, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::SetStdHandle;

    let handle = if let Some(handle) = inherited {
        handle
    } else {
        // SAFETY: `device` is a null-terminated ASCII device name. CreateFileA
        // opens the console input/output buffer of the attached console.
        let console = unsafe {
            CreateFileA(
                device.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if console.is_null() || console == INVALID_HANDLE_VALUE {
            return;
        }
        console
    };

    // SAFETY: `handle` is a valid open handle; SetStdHandle only stores it.
    unsafe {
        SetStdHandle(which, handle);
    }
}

/// Compile-time conversion of an ASCII byte literal to a null-terminated
/// UTF-16 array. `N` must equal `src.len() + 1` (for the null terminator).
const fn wide_literal<const N: usize>(src: &[u8]) -> [u16; N] {
    assert!(src.len() + 1 == N, "N must be src.len() + 1");
    let mut buf = [0u16; N];
    let mut i = 0;
    while i < src.len() {
        buf[i] = src[i] as u16;
        i += 1;
    }
    buf
}

/// Wait for the user to press Enter before the console window closes.
///
/// Prints a dimmed prompt to `stderr` and blocks on `stdin`.  Intended for
/// [`ConsoleMode::Standalone`] sessions where the console would vanish
/// immediately after the program exits.
pub fn pause_before_exit() {
    use std::io::Write;
    eprint!("\n  {}", crate::strings::cli::PAUSE_PROMPT.dimmed());
    drop(std::io::stderr().flush());
    drop(std::io::stdin().read_line(&mut String::new()));
}

/// Enable ANSI virtual terminal processing on the standard output console.
///
/// Sets `ENABLE_VIRTUAL_TERMINAL_PROCESSING` on the `STD_OUTPUT_HANDLE` so
/// that ANSI escape sequences (used by the `colored` crate) render correctly
/// in legacy `conhost.exe` terminals.  Does nothing if the handle is invalid
/// (e.g. output is fully redirected with no console attached).
pub fn enable_ansi_colors() {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE,
        SetConsoleMode,
    };
    // SAFETY: GetStdHandle/GetConsoleMode/SetConsoleMode are standard Win32 calls
    // with no preconditions beyond a valid handle. STD_OUTPUT_HANDLE is always valid.
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        // Guard against INVALID_HANDLE_VALUE (e.g. output fully redirected with no console)
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return;
        }
        let mut mode: u32 = 0;
        if GetConsoleMode(handle, &raw mut mode) != 0 {
            let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}

// ─── System Theme Control ────────────────────────────────────────────────────

/// Force the process's native Win32 menus to render in dark or light theme.
///
/// Uses the undocumented but stable `SetPreferredAppMode` (ordinal 135) and
/// `FlushMenuThemes` (ordinal 136) exports from `uxtheme.dll`.  These ordinals
/// have been stable since Windows 10 1903 and are relied on by production
/// apps such as Firefox, VS Code, and Notepad++.
///
/// Does nothing silently if the DLL cannot be loaded or the ordinals are
/// missing (e.g. on older Windows builds).
#[expect(
    clippy::as_conversions,
    reason = "MAKEINTRESOURCE pattern: ordinal as *const u8 is the Win32 convention"
)]
pub fn set_process_dark_mode(dark: bool) {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    /// `SetPreferredAppMode` argument: force dark context menus.
    const FORCE_DARK: i32 = 2;
    /// `SetPreferredAppMode` argument: force light context menus.
    const FORCE_LIGHT: i32 = 3;

    let lib_name: Vec<u16> = "uxtheme.dll".encode_utf16().chain(Some(0)).collect();

    // SAFETY: LoadLibraryW is a standard Win32 call with a null-terminated
    // wide string.  Returns null on failure.
    let hmodule = unsafe { LoadLibraryW(lib_name.as_ptr()) };
    if hmodule.is_null() {
        return;
    }

    // Ordinal 135 - SetPreferredAppMode(mode: i32) -> i32
    // SAFETY: GetProcAddress with a MAKEINTRESOURCE-style ordinal (low 16
    // bits = ordinal, high bits zero) is the documented Win32 pattern for
    // looking up exports by ordinal number.
    let set_mode_addr = unsafe { GetProcAddress(hmodule, 135_usize as *const u8) };
    if let Some(f) = set_mode_addr {
        // SAFETY: Ordinal 135 is `fn(i32) -> i32` (stdcall).  This
        // signature has been stable across all Windows 10/11 builds since
        // 1903.  Transmute between equal-sized function pointer types is sound.
        let set_mode: unsafe extern "system" fn(i32) -> i32 = unsafe { std::mem::transmute(f) };
        unsafe {
            set_mode(if dark { FORCE_DARK } else { FORCE_LIGHT });
        }
    }

    // Ordinal 136 - FlushMenuThemes()
    // Forces all menus in the process to re-evaluate their theme on next show.
    let flush_addr = unsafe { GetProcAddress(hmodule, 136_usize as *const u8) };
    if let Some(f) = flush_addr {
        // SAFETY: Ordinal 136 is `fn()`.  Transmute is sound (same size).
        let flush: unsafe extern "system" fn() = unsafe { std::mem::transmute(f) };
        unsafe {
            flush();
        }
    }
}

/// Set the window title bar to dark or light mode independently of the OS theme.
///
/// Uses [`DwmSetWindowAttribute`] with `DWMWA_USE_IMMERSIVE_DARK_MODE`
/// (value 20, stable since Windows 10 build 18985 / 19H2).  This ensures the
/// title bar matches the in-app theme rather than inheriting the OS-level
/// dark/light preference.
///
/// Does nothing when `hwnd` is `0` (no window found) or on failure.
///
/// [`DwmSetWindowAttribute`]: https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmsetwindowattribute
pub fn set_title_bar_dark_mode(hwnd: isize, dark: bool) {
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;

    /// `DWMWA_USE_IMMERSIVE_DARK_MODE` attribute constant.
    /// Stable since Windows 10 Build 18985 (19H2+).
    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;

    if hwnd == 0 {
        return;
    }

    let value: i32 = i32::from(dark);

    // SAFETY: DwmSetWindowAttribute is a documented Win32 DWM call.
    // We pass a valid HWND, the attribute constant, a pointer to a
    // BOOL-valued i32, and its byte size (4).  The isize→HWND cast
    // is the reverse of find_app_window's HWND→isize return convention.
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd as windows_sys::Win32::Foundation::HWND,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            std::ptr::from_ref(&value).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    };
}

// ─── Unelevated URL launch ───────────────────────────────────────────────────

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
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
    use windows_sys::Win32::System::Threading::{
        CreateProcessWithTokenW, OpenProcess, OpenProcessToken, PROCESS_INFORMATION,
        PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId};

    use crate::stats::{HandleGuard, to_wide};

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

    let mut system_dir = [0u16; 260];
    // SAFETY: `system_dir` is a writable buffer of the stated length.
    let len =
        unsafe { GetSystemDirectoryW(system_dir.as_mut_ptr(), system_dir.len() as u32) } as usize;
    if len == 0 || len >= system_dir.len() {
        bail!("cannot locate the system directory");
    }
    let rundll32 = format!(
        "{}\\rundll32.exe",
        String::from_utf16_lossy(&system_dir[..len])
    );
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

// ─── Balloon notification ────────────────────────────────────────────────────

/// Notification icon unique ID (arbitrary, scoped to this process).
const NOTIFY_ICON_UID: u32 = 0xBEEF;

/// Duration the balloon notification stays visible before auto-dismiss (ms).
const BALLOON_TIMEOUT_MS: u32 = 2000;

/// Balloon info icon style (shows an "i" icon).
/// From `shellapi.h` `NIIF_INFO` - not exposed by `windows-sys`.
const NIIF_INFO: u32 = 0x01;

/// Suppress the notification sound.
/// From `shellapi.h` `NIIF_NOSOUND` - not exposed by `windows-sys`.
const NIIF_NOSOUND: u32 = 0x10;

/// Encode a Rust `&str` as null-terminated UTF-16 into a fixed-size buffer.
///
/// Silently truncates if `s` is longer than `buf.len() -1`.
fn write_wide_into(buf: &mut [u16], s: &str) {
    let mut i = 0;
    for c in s.encode_utf16() {
        if i >= buf.len() - 1 {
            break;
        }
        buf[i] = c;
        i += 1;
    }
    buf[i] = 0;
}

/// Show a transient Windows balloon notification that auto-dismisses.
///
/// Uses the classic `Shell_NotifyIconW` balloon API so the notification:
/// - Appears near the system tray
/// - Auto-dismisses after ~2 seconds (Windows 10/11 may show it as a toast)
///
/// A hidden message-only window is created to receive Shell callback
/// messages, and a brief message pump runs so the balloon can render.
///
/// Returns `Ok(())` on success. Errors are non-fatal - the cleaning
/// operation has already completed, so a notification failure is harmless.
pub fn show_balloon_notification(title: &str, body: &str) -> Result<()> {
    use windows_sys::Win32::UI::Shell::{
        NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_REALTIME, NIF_TIP, NIM_ADD, NIM_DELETE,
        NIM_SETVERSION, NOTIFYICON_VERSION, NOTIFYICONDATAW, Shell_NotifyIconW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyWindow, WM_APP};

    let hwnd = create_notification_window()?;
    let hicon = load_app_icon();

    // Zero-init the struct, then fill in the fields we need.
    // SAFETY: NOTIFYICONDATAW is a POD struct - zeroing is valid initialisation.
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = NOTIFY_ICON_UID;
    nid.uFlags = NIF_ICON | NIF_TIP | NIF_INFO | NIF_MESSAGE | NIF_REALTIME;
    nid.uCallbackMessage = WM_APP;
    nid.hIcon = hicon;

    // Tooltip (shown on hover over the tray icon)
    write_wide_into(&mut nid.szTip, crate::strings::APP_NAME);

    // Balloon title and body
    write_wide_into(&mut nid.szInfoTitle, title);
    write_wide_into(&mut nid.szInfo, body);

    // Balloon icon style and behaviour flags.
    nid.dwInfoFlags = NIIF_INFO | NIIF_NOSOUND;

    // Anonymous union: `uTimeout` (deprecated field, but still used on legacy
    // paths to hint display duration). The union shares space with `uVersion`.
    // Set timeout before NIM_ADD, then set version with NIM_SETVERSION.
    nid.Anonymous.uTimeout = BALLOON_TIMEOUT_MS;

    // SAFETY: Shell_NotifyIconW is a standard Shell32 call. nid is fully
    // initialised above with valid field values. NIM_ADD adds a tray icon.
    let added = unsafe { Shell_NotifyIconW(NIM_ADD, &raw const nid) };
    if added == 0 {
        // SAFETY: DestroyWindow with a valid hwnd from CreateWindowExW.
        unsafe {
            DestroyWindow(hwnd);
        }
        bail!("Shell_NotifyIconW(NIM_ADD) failed");
    }

    // Set the icon version to NOTIFYICON_VERSION (v3) so the balloon uses
    // the classic style and is NOT forwarded to the Action Center.
    nid.Anonymous.uVersion = NOTIFYICON_VERSION;
    // SAFETY: NIM_SETVERSION is a standard call; nid.uID identifies our icon.
    unsafe {
        Shell_NotifyIconW(NIM_SETVERSION, &raw const nid);
    }

    // Run a message pump so Windows can deliver the Shell callback
    // messages that trigger the actual balloon display.
    pump_messages(BALLOON_TIMEOUT_MS);

    // ── Cleanup ──────────────────────────────────────────────────────
    // SAFETY: NIM_DELETE removes the tray icon. DestroyWindow destroys
    // the hidden message window. The icon from LoadIconW is a shared resource
    // owned by the system and must not be destroyed.
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &raw const nid);
        DestroyWindow(hwnd);
    }

    Ok(())
}

/// Create a hidden message-only window for `Shell_NotifyIconW`.
///
/// The Shell notification system requires a valid `hWnd` to deliver
/// callback messages. A message-only window (`HWND_MESSAGE` parent)
/// is invisible and never shown on screen or in the taskbar.
fn create_notification_window() -> Result<windows_sys::Win32::Foundation::HWND> {
    use windows_sys::Win32::UI::WindowsAndMessaging::CreateWindowExW;

    let hinstance = get_exe_hinstance();

    // Use the built-in "STATIC" control class - no custom registration needed.
    let class_name = wide_literal::<7>(b"STATIC");

    // HWND_MESSAGE = (HWND)-3 - creates a message-only window that has
    // no visible representation and only receives posted/sent messages.
    let hwnd_message = std::ptr::without_provenance_mut::<std::ffi::c_void>(!2_usize);

    // SAFETY: CreateWindowExW with a built-in class name (STATIC),
    // zero dimensions, and HWND_MESSAGE parent. Creates an invisible
    // message-only window suitable for Shell_NotifyIconW callbacks.
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            hwnd_message,
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        )
    };

    if hwnd.is_null() {
        bail!("Failed to create notification window");
    }
    Ok(hwnd)
}

/// Run a Win32 message pump for `duration_ms` milliseconds.
///
/// Processes pending messages each iteration, then sleeps briefly to
/// avoid busy-spinning. Required for `Shell_NotifyIconW` balloons to
/// display - the Shell delivers callback messages that drive the
/// balloon lifecycle.
fn pump_messages(duration_ms: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
    };

    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(u64::from(duration_ms));

    loop {
        if std::time::Instant::now() >= deadline {
            break;
        }

        // SAFETY: PeekMessageW / TranslateMessage / DispatchMessageW are
        // standard Win32 message loop calls. msg is stack-allocated and
        // valid. Null hWnd processes all thread messages.
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&raw mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&raw const msg);
                DispatchMessageW(&raw const msg);
            }
        }

        // Brief sleep to avoid busy-spinning
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Attempt to acquire a system-wide named mutex for single-instance enforcement.
///
/// If no other instance holds the mutex, returns `Some(handle)` - the caller
/// **must** keep this value alive for the lifetime of the process (dropping or
/// closing it releases the mutex, allowing a second launch).
///
/// If another instance already holds the mutex, finds and restores its window
/// and returns `None`, signalling the caller to exit cleanly.
#[must_use]
pub fn try_acquire_single_instance() -> Option<isize> {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SW_RESTORE, SetForegroundWindow, ShowWindow,
    };

    // Unique mutex name scoped to the current user's session.
    let name: Vec<u16> = "Local\\MagicXRamCleanerSingleInstance"
        .encode_utf16()
        .chain(Some(0u16))
        .collect();

    // SAFETY: CreateMutexW with a valid null-terminated wide string and
    // null security attributes. Returns a valid handle or null on failure.
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };

    if handle.is_null() {
        // CreateMutexW failed entirely - let the app launch anyway so
        // the user isn't blocked by a transient OS error.
        return Some(0);
    }

    // SAFETY: GetLastError is safe to call immediately after CreateMutexW.
    let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;

    if already_exists {
        // Another instance owns the mutex. Find its window and bring it
        // to the foreground, then signal the caller to exit.
        let hwnd = find_app_window(crate::strings::APP_NAME);
        if hwnd != 0 {
            // SAFETY: hwnd is the existing instance's main window.
            // ShowWindow(SW_RESTORE) un-minimizes if iconic, and
            // SetForegroundWindow brings it in front.  PostMessageW
            // wakes the event loop immediately so the first instance
            // detects the visibility change without waiting for its
            // next scheduled repaint.
            unsafe {
                ShowWindow(hwnd as *mut _, SW_RESTORE);
                SetForegroundWindow(hwnd as *mut _);
                windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                    hwnd as *mut _,
                    windows_sys::Win32::UI::WindowsAndMessaging::WM_PAINT,
                    0,
                    0,
                );
            }
        }

        // Close our duplicate handle before returning.
        // SAFETY: handle is a valid mutex handle returned by CreateMutexW.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(handle);
        }

        return None;
    }

    Some(handle as isize)
}

/// Return the `HWND` of the first top-level window whose title equals `title`.
///
/// Returns `0` when no matching window is found.  Because we look up our
/// **own** window we do not have to worry about the inherent race between
/// `FindWindowW` and the window closing.
#[must_use]
pub fn find_app_window(title: &str) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;

    let wide: Vec<u16> = title.encode_utf16().chain(Some(0u16)).collect();

    // SAFETY: `FindWindowW` is called with a valid null-terminated wide
    // string allocated on this stack frame.  The return value is an `HWND`
    // pointer valid for the lifetime of the target window; we immediately
    // cast it to `isize` for `Send`-safe storage.
    unsafe { FindWindowW(std::ptr::null(), wide.as_ptr()) as isize }
}

/// Check whether the application window is currently minimized (iconic).
///
/// Uses the Win32 `IsIconic` API for reliable minimized-state detection.
/// Unlike `egui::ViewportInfo::minimized` (which may return `None` when
/// the windowing back-end does not report it), this always returns a
/// definitive answer on Windows.
///
/// Returns `false` when `hwnd` is `0`.
#[must_use]
pub fn is_window_minimized(hwnd: isize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsIconic;

    if hwnd == 0 {
        return false;
    }

    // SAFETY: IsIconic is a read-only state check on a valid owned window.
    unsafe { IsIconic(hwnd as *mut _) != 0 }
}

/// Immediately hide the window by calling `ShowWindow(SW_HIDE)` directly.
///
/// Synchronously clears the `WS_VISIBLE` flag so the window disappears on
/// the current frame.  Prefer [`cloak_window`] for the minimize-to-tray
/// flow; this function is used internally by `cloak_window`.
///
/// Does nothing when `hwnd` is `0`.
pub fn hide_window(hwnd: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};

    if hwnd == 0 {
        return;
    }

    // SAFETY: `hwnd` is our own main window, valid for the lifetime of the
    // process.  `SW_HIDE` is a standard, non-destructive window-state change.
    unsafe { ShowWindow(hwnd as *mut _, SW_HIDE) };
}

/// Cloak the window: make it invisible to the user while keeping
/// `WS_VISIBLE` set so eframe's event loop stays in `ControlFlow::Wait`.
///
/// The approach avoids the eframe/winit CPU bug (`emilk/egui#7776`) where
/// `ControlFlow::Poll` is used for invisible windows.  Steps:
///
/// 1. `SW_HIDE` - instantly clear `WS_VISIBLE` so no animations play.
/// 2. Add `WS_EX_TOOLWINDOW` / remove `WS_EX_APPWINDOW` - hides the
///    window from the taskbar and Alt-Tab.
/// 3. `SW_SHOWMINNOACTIVE` - restores `WS_VISIBLE` in the iconic
///    (minimized) state without stealing focus.  On modern Windows,
///    a minimized tool window has no on-screen representation.
///
/// After this call: `IsWindowVisible` → `true`, `IsIconic` → `true`,
/// no taskbar button, no Alt-Tab entry, zero visual presence.
///
/// Does nothing when `hwnd` is `0`.
pub fn cloak_window(hwnd: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, SW_SHOWMINNOACTIVE, SetWindowLongPtrW, ShowWindow,
        WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    if hwnd == 0 {
        return;
    }

    // SAFETY: All calls operate on our own main-window handle, which is
    // valid for the process lifetime.  The sequence is
    // hide → style change → show-minimized, each a standard Win32 call.
    unsafe {
        hide_window(hwnd);
        let ex = GetWindowLongPtrW(hwnd as *mut _, GWL_EXSTYLE);
        #[allow(clippy::cast_possible_wrap)]
        let new_ex = (ex | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize);
        SetWindowLongPtrW(hwnd as *mut _, GWL_EXSTYLE, new_ex);
        ShowWindow(hwnd as *mut _, SW_SHOWMINNOACTIVE);
    }
}

/// Reverse [`cloak_window`]: restore the window to its pre-minimize state
/// with a normal taskbar button and Alt-Tab entry.
///
/// Steps:
///
/// 1. Remove `WS_EX_TOOLWINDOW` / add `WS_EX_APPWINDOW` - taskbar and
///    Alt-Tab presence restored.
/// 2. `SW_RESTORE` - un-minimizes to the previous size and position.
/// 3. `SetForegroundWindow` - brings the window to the front.
///
/// Does nothing when `hwnd` is `0`.
pub fn uncloak_window(hwnd: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, SW_RESTORE, SetForegroundWindow, SetWindowLongPtrW,
        ShowWindow, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };

    if hwnd == 0 {
        return;
    }

    // SAFETY: All calls operate on our own main-window handle, which is
    // valid for the process lifetime.  The sequence is
    // style-restore → un-minimize → foreground, each a standard Win32 call.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd as *mut _, GWL_EXSTYLE);
        #[allow(clippy::cast_possible_wrap)]
        let new_ex = (ex & !(WS_EX_TOOLWINDOW as isize)) | WS_EX_APPWINDOW as isize;
        SetWindowLongPtrW(hwnd as *mut _, GWL_EXSTYLE, new_ex);
        ShowWindow(hwnd as *mut _, SW_RESTORE);
        SetForegroundWindow(hwnd as *mut _);
    }
}

/// Load the application icon (resource ID 1) from the running executable.
///
/// Falls back to a null handle if loading fails (the balloon will show
/// without a custom icon, using the default info icon instead).
fn load_app_icon() -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    use windows_sys::Win32::UI::WindowsAndMessaging::LoadIconW;

    // Get the module handle for the current exe (HINSTANCE == HMODULE for exe)
    let hinstance = get_exe_hinstance();

    // MAKEINTRESOURCEW(1) = resource ID 1 = app.ico
    // This is the standard Win32 MAKEINTRESOURCE pattern: an integer packed
    // into the low 16 bits of a pointer. Not a real dereferenceable address.
    let resource_id = std::ptr::without_provenance::<u16>(1);

    // SAFETY: LoadIconW with a valid hinstance and numeric resource ID is safe.
    // Returns null on failure (we handle that gracefully).
    unsafe { LoadIconW(hinstance, resource_id) }
}

/// Get the `HINSTANCE` of the running executable.
fn get_exe_hinstance() -> windows_sys::Win32::Foundation::HINSTANCE {
    // For an .exe, HINSTANCE == the base address of the module.
    // GetModuleHandleW(null) returns the handle of the exe itself.
    // SAFETY: GetModuleHandleW(null) is a standard Win32 call.
    unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()) }
}
