//! # `MagicX` RAM Cleaner — Console Utilities
//!
//! Windows console platform utilities: ANSI virtual terminal processing,
//! standalone console detection, pause-before-exit for double-click launches,
//! console hiding, and balloon notification display.
//!
//! These are isolated from business logic so that platform-specific console
//! quirks don't leak into the application layer.

use anyhow::{Result, bail};
use colored::Colorize;

/// Detect whether this process owns its console (i.e. launched by double-clicking).
///
/// Uses `GetConsoleProcessList` — if only our process is attached, the console
/// was created just for us and will vanish when we exit.
#[must_use]
pub fn is_standalone_console() -> bool {
    use windows_sys::Win32::System::Console::GetConsoleProcessList;
    let mut pids = [0u32; 2];
    // SAFETY: `GetConsoleProcessList` is a standard Win32 console API.
    // We pass a valid stack-allocated buffer and its length.
    let count = unsafe { GetConsoleProcessList(pids.as_mut_ptr(), 2) };
    count <= 1
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

/// Detach from the console window so it closes immediately.
///
/// Called in `--notify` mode to prevent a visible terminal window.
/// After this call, stdout/stderr are no longer usable.
pub fn hide_console_window() {
    use windows_sys::Win32::System::Console::FreeConsole;
    // SAFETY: FreeConsole is a standard Win32 call with no preconditions.
    // After this call the console window is destroyed and stdout/stderr
    // become invalid, which is intentional in notification-only mode.
    unsafe {
        FreeConsole();
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
/// The notification icon is loaded from the current executable's embedded
/// resources (resource ID 1 = `app.ico`).
///
/// Returns `Ok(())` on success. Errors are non-fatal — the cleaning
/// operation has already completed, so a notification failure is harmless.
pub fn show_balloon_notification(title: &str, body: &str) -> Result<()> {
    use windows_sys::Win32::UI::Shell::{
        NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_REALTIME, NIF_TIP, NIM_ADD, NIM_DELETE,
        NIM_SETVERSION, NOTIFYICON_VERSION, NOTIFYICONDATAW, Shell_NotifyIconW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

    let hicon = load_app_icon();

    // Zero-init the struct, then fill in the fields we need.
    // SAFETY: NOTIFYICONDATAW is a POD struct — zeroing is valid initialisation.
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    // Use a null HWND — we don't need callback messages, we just show and remove.
    nid.hWnd = std::ptr::null_mut();
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
        bail!("Shell_NotifyIconW(NIM_ADD) failed");
    }

    // Set the icon version to NOTIFYICON_VERSION (v3) so the balloon uses
    // the classic style and is NOT forwarded to the Action Center.
    nid.Anonymous.uVersion = NOTIFYICON_VERSION;
    // SAFETY: NIM_SETVERSION is a standard call; nid.uID identifies our icon.
    unsafe {
        Shell_NotifyIconW(NIM_SETVERSION, &raw const nid);
    }

    // Keep the balloon visible for the desired duration, then remove the icon.
    // Removing the icon also dismisses the balloon and prevents Action Center
    // persistence.
    std::thread::sleep(std::time::Duration::from_millis(u64::from(
        BALLOON_TIMEOUT_MS,
    )));

    // SAFETY: NIM_DELETE removes the tray icon. nid identifies it by hWnd + uID.
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &raw const nid);
    }

    // Release the loaded icon handle
    if !hicon.is_null() {
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;
        // SAFETY: hicon is a valid icon handle returned by LoadIconW.
        unsafe {
            DestroyIcon(hicon);
        }
    }

    Ok(())
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
