//! # `MagicX` RAM Cleaner — Console Utilities
//!
//! Windows console platform utilities: dynamic console attachment for
//! the `SUBSYSTEM:WINDOWS` binary, ANSI virtual terminal processing,
//! standalone detection, pause-before-exit, and balloon notification display.
//!
//! ## Why `SUBSYSTEM:WINDOWS`?
//!
//! This binary sets `#![windows_subsystem = "windows"]` so Windows never
//! auto-creates a console window. When launched from the right-click context
//! menu (`--notify` mode), no console appears at all — the user sees only a
//! brief balloon notification. For normal CLI usage, [`attach_or_create_console`]
//! dynamically attaches to the calling terminal or allocates a fresh console
//! (double-click launch).
//!
//! These are isolated from business logic so that platform-specific console
//! quirks don't leak into the application layer.

use anyhow::{Result, bail};
use colored::Colorize;

// ─── Dynamic console management ─────────────────────────────────────────────

/// How the console session was obtained.
///
/// Returned by [`attach_or_create_console`] so the caller can decide whether
/// the process is "standalone" (allocated its own console → pause on exit)
/// or attached to a parent terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleMode {
    /// Attached to an existing parent console (launched from cmd/PowerShell).
    Attached,
    /// Allocated a brand-new console (launched by double-clicking the `.exe`).
    Allocated,
}

/// Attach to the parent process's console, or allocate a new one.
///
/// Because this binary uses `SUBSYSTEM:WINDOWS`, no console exists at
/// startup. This function creates one so that `println!` and coloured
/// output work normally.
///
/// Returns [`ConsoleMode::Attached`] when a parent console was reused
/// (the user launched from a terminal), or [`ConsoleMode::Allocated`]
/// when a fresh console was created (the user double-clicked the `.exe`).
///
/// # Console handle redirection
///
/// `AttachConsole` does **not** update the process's standard handles for
/// a `SUBSYSTEM:WINDOWS` binary. After attaching, this function opens
/// `CONOUT$` / `CONIN$` and calls `SetStdHandle` so that Rust's
/// `println!` / `eprintln!` write to the correct console.
///
/// `AllocConsole` sets the standard handles automatically — no manual
/// redirection is needed.
#[must_use]
pub fn attach_or_create_console() -> ConsoleMode {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole};

    // SAFETY: AttachConsole is a standard Win32 call.
    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0;

    if attached {
        // Manually redirect stdout/stderr/stdin to the attached console
        // because AttachConsole does not update std handles for GUI apps.
        redirect_std_handles();
        ConsoleMode::Attached
    } else {
        // No parent console (e.g. double-clicked, or launched from GUI).
        // AllocConsole creates a new console AND sets the std handles.
        // SAFETY: AllocConsole is a standard Win32 call.
        unsafe {
            AllocConsole();
        }
        ConsoleMode::Allocated
    }
}

/// Redirect standard I/O handles to the attached console's buffers.
///
/// Opens `CONOUT$` for stdout + stderr and `CONIN$` for stdin using
/// `CreateFileW`, then installs them via `SetStdHandle`. Handles that
/// are already valid (e.g. inherited from a parent via `STARTF_USESTDHANDLES`,
/// as happens with shell I/O redirection like `> file.txt`) are left
/// untouched so piping continues to work.
fn redirect_std_handles() {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
    };

    /// `GENERIC_READ` access right (`0x8000_0000`).
    const GENERIC_READ: u32 = 0x8000_0000;
    /// `GENERIC_WRITE` access right (`0x4000_0000`).
    const GENERIC_WRITE: u32 = 0x4000_0000;

    // ── CONOUT$ (stdout + stderr) ────────────────────────────────────
    // Only redirect if the current handle is null/invalid (i.e. the
    // parent did NOT set up I/O redirection via STARTF_USESTDHANDLES).
    // SAFETY: GetStdHandle with a valid constant is always safe.
    let cur_out = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    let out_needs_redirect = cur_out.is_null() || cur_out == INVALID_HANDLE_VALUE;

    if out_needs_redirect {
        let conout: [u16; 8] = wide_literal::<8>(b"CONOUT$");
        // SAFETY: CreateFileW with the well-known CONOUT$ pseudo-device
        // name, using GENERIC_READ | GENERIC_WRITE for SetConsoleMode
        // compatibility (GetConsoleMode needs read access).
        let handle = unsafe {
            CreateFileW(
                conout.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            // SAFETY: SetStdHandle with a valid handle from CreateFileW.
            unsafe {
                SetStdHandle(STD_OUTPUT_HANDLE, handle);
                SetStdHandle(STD_ERROR_HANDLE, handle);
            }
        }
    }

    // ── CONIN$ (stdin) ───────────────────────────────────────────────
    // SAFETY: GetStdHandle with a valid constant is always safe.
    let cur_in = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    let in_needs_redirect = cur_in.is_null() || cur_in == INVALID_HANDLE_VALUE;

    if in_needs_redirect {
        let conin: [u16; 7] = wide_literal::<7>(b"CONIN$");
        // SAFETY: CreateFileW with the well-known CONIN$ pseudo-device name.
        let handle = unsafe {
            CreateFileW(
                conin.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            // SAFETY: SetStdHandle with a valid handle from CreateFileW.
            unsafe {
                SetStdHandle(STD_INPUT_HANDLE, handle);
            }
        }
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
