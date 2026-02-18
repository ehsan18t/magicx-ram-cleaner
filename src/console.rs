//! # `MagicX` RAM Cleaner — Console Utilities
//!
//! Windows console platform utilities: standalone detection, ANSI virtual
//! terminal processing, pause-before-exit, and balloon notification display.
//!
//! The binary uses `SUBSYSTEM:CONSOLE` (the default), so a console window
//! always exists at startup. For context-menu (`--notify`) launches,
//! [`hide_and_free_console`] is called immediately to destroy the
//! auto-created console before it becomes visible. For normal CLI usage
//! the console is used as-is.
//!
//! These are isolated from business logic so that platform-specific console
//! quirks don't leak into the application layer.

use anyhow::{Result, bail};
use colored::Colorize;

// ─── Console mode detection ─────────────────────────────────────────────────

/// How the process was launched.
///
/// Returned by [`detect_console_mode`] so the caller can decide whether
/// to pause before exit (standalone) or return immediately (terminal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleMode {
    /// Sharing a console with a parent shell (launched from cmd / `PowerShell`).
    Terminal,
    /// Own console created by the OS (double-clicked the `.exe` or started
    /// from a GUI launcher). Caller should [`pause_before_exit`].
    Standalone,
}

/// Detect whether this process was launched from a terminal or standalone.
///
/// With `SUBSYSTEM:CONSOLE` Windows always creates a console at startup.
/// When launched **from** a terminal, the child shares the parent's console
/// (multiple processes attached). When double-clicked, Windows creates a
/// fresh console exclusively for this process (single process attached).
///
/// Uses `GetConsoleProcessList` to count how many processes share the
/// console. More than one → terminal; exactly one → standalone.
#[must_use]
pub fn detect_console_mode() -> ConsoleMode {
    use windows_sys::Win32::System::Console::GetConsoleProcessList;

    let mut pids = [0u32; 4];
    // SAFETY: GetConsoleProcessList is a standard Win32 call. We pass a
    // valid buffer and its capacity. Returns the number of processes.
    let count = unsafe { GetConsoleProcessList(pids.as_mut_ptr(), 4) };

    if count > 1 {
        ConsoleMode::Terminal
    } else {
        ConsoleMode::Standalone
    }
}

/// Hide and free the auto-created console for notification-only mode.
///
/// With `SUBSYSTEM:CONSOLE` the OS creates a console window before `main`
/// runs. This function hides it with `ShowWindow(SW_HIDE)` and then
/// detaches with `FreeConsole()` so no console is visible at all.
/// Called as the very first thing in `--notify` mode.
pub fn hide_and_free_console() {
    use windows_sys::Win32::System::Console::{FreeConsole, GetConsoleWindow};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};

    // SAFETY: GetConsoleWindow / ShowWindow / FreeConsole are standard Win32
    // calls with no special preconditions. Hiding before freeing prevents
    // any visible flash on slower machines.
    unsafe {
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_HIDE);
        }
        FreeConsole();
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
pub fn pause_before_exit() {
    use std::io::Write;
    eprint!("\n  {}", "Press Enter to exit...".dimmed());
    drop(std::io::stderr().flush());
    drop(std::io::stdin().read_line(&mut String::new()));
}

/// Enable ANSI virtual terminal processing on Windows consoles.
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

// ─── Balloon notification ────────────────────────────────────────────────────

/// Notification icon unique ID (arbitrary, scoped to this process).
const NOTIFY_ICON_UID: u32 = 0xBEEF;

/// Duration the balloon notification stays visible before auto-dismiss (ms).
const BALLOON_TIMEOUT_MS: u32 = 2000;

/// Encode a Rust `&str` as null-terminated UTF-16 into a fixed-size buffer.
///
/// Silently truncates if `s` is longer than `buf.len() - 1`.
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
/// - Auto-dismisses after ~2 seconds
/// - Is **not** persisted in the Windows Action Center
///
/// A hidden message-only window is created to receive Shell callback
/// messages, and a brief message pump runs so the balloon can render.
///
/// Returns `Ok(())` on success. Errors are non-fatal — the cleaning
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
    // SAFETY: NOTIFYICONDATAW is a POD struct — zeroing is valid initialisation.
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = NOTIFY_ICON_UID;
    nid.uFlags = NIF_ICON | NIF_TIP | NIF_INFO | NIF_MESSAGE | NIF_REALTIME;
    nid.uCallbackMessage = WM_APP;
    nid.hIcon = hicon;

    // Tooltip (shown on hover over the tray icon)
    write_wide_into(&mut nid.szTip, "MagicX RAM Cleaner");

    // Balloon title and body
    write_wide_into(&mut nid.szInfoTitle, title);
    write_wide_into(&mut nid.szInfo, body);

    // NIIF_INFO = 1 (info icon style), NIIF_NOSOUND = 0x10
    // Combined: show info icon, no sound, respect quiet time
    nid.dwInfoFlags = 0x01 | 0x10;

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
    // the hidden message window. DestroyIcon releases the loaded icon.
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &raw const nid);
        DestroyWindow(hwnd);
    }

    if !hicon.is_null() {
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;
        // SAFETY: hicon is a valid icon handle returned by LoadIconW.
        unsafe {
            DestroyIcon(hicon);
        }
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

    // Use the built-in "STATIC" control class — no custom registration needed.
    let class_name = wide_literal::<7>(b"STATIC");

    // HWND_MESSAGE = (HWND)-3 — creates a message-only window that has
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
/// display — the Shell delivers callback messages that drive the
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
