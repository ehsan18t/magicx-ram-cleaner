//! # `MagicX` RAM Cleaner — Console Utilities
//!
//! Windows console platform utilities: on-demand console attach/alloc for CLI
//! mode, ANSI virtual terminal processing, pause-before-exit, and balloon
//! notification display.
//!
//! The binary uses `SUBSYSTEM:WINDOWS` so **no** console window is created at
//! startup. For CLI usage, `setup_cli_console()` attaches to the parent
//! terminal (cmd / `PowerShell`) or allocates a fresh console. GUI and
//! notification modes skip this entirely — zero-flash launches.
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
/// Uses `GetConsoleProcessList` to count how many processes share the
/// console. More than one → terminal; exactly one → standalone.
///
/// **Note:** only meaningful after a console is attached. With
/// `SUBSYSTEM:WINDOWS`, prefer [`setup_cli_console`] which returns the
/// mode directly based on whether `AttachConsole` succeeded.
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

/// Attach to the parent terminal or allocate a fresh console for CLI mode.
///
/// With `SUBSYSTEM:WINDOWS`, the process starts with **no** console at all.
/// This function:
/// 1. Tries `AttachConsole(ATTACH_PARENT_PROCESS)` — succeeds when launched
///    from cmd / `PowerShell` / Windows Terminal.
/// 2. Falls back to `AllocConsole()` — creates a brand-new console window
///    (typical for standalone execution with CLI args).
/// 3. Redirects `stdout` / `stderr` to `CONOUT$` so `println!` works.
///
/// Returns [`ConsoleMode::Terminal`] if attached to the parent shell, or
/// [`ConsoleMode::Standalone`] if a new console was allocated.
#[must_use]
pub fn setup_cli_console() -> ConsoleMode {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole};

    // SAFETY: AttachConsole is a standard Win32 call. ATTACH_PARENT_PROCESS
    // tells Windows to attach to the console of the process that launched us.
    // Returns non-zero on success (we were launched from a terminal).
    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0;

    let mode = if attached {
        ConsoleMode::Terminal
    } else {
        // No parent console available (e.g. double-clicked with CLI args).
        // Allocate a brand-new console for output.
        // SAFETY: AllocConsole is a standard Win32 call with no preconditions.
        unsafe {
            AllocConsole();
        }
        ConsoleMode::Standalone
    };

    redirect_std_handles();
    mode
}

/// Redirect `stdout` and `stderr` to the attached/allocated console.
///
/// After `AttachConsole` or `AllocConsole`, the process has a console but
/// Rust's `std::io::stdout()` and `stderr()` may still return null handles
/// (SUBSYSTEM:WINDOWS default). Opening `CONOUT$` and setting it as the
/// standard output/error handle ensures all subsequent writes — including
/// `println!`, `eprintln!`, and `colored` output — work correctly.
fn redirect_std_handles() {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_WRITE, OPEN_EXISTING};
    use windows_sys::Win32::System::Console::{STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle};

    // Standard Win32 access rights (from winnt.h). Not re-exported by
    // the `Win32_Storage_FileSystem` feature in windows-sys 0.61+.
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;

    let conout_name = wide_literal::<8>(b"CONOUT$");

    // SAFETY: CreateFileW with CONOUT$ opens the active console output
    // buffer. The parameters are standard: read/write access, shared write,
    // no security attributes, open existing device, no flags, no template.
    let conout = unsafe {
        CreateFileW(
            conout_name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };

    if conout.is_null() || conout == INVALID_HANDLE_VALUE {
        return;
    }

    // SAFETY: SetStdHandle with the valid CONOUT$ handle redirects all
    // stdout/stderr output to the console. Subsequent GetStdHandle calls
    // in Rust's runtime will return this handle.
    unsafe {
        SetStdHandle(STD_OUTPUT_HANDLE, conout);
        SetStdHandle(STD_ERROR_HANDLE, conout);
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
    eprint!("\n  {}", crate::strings::cli::PAUSE_PROMPT.dimmed());
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

// ─── System Theme Detection ──────────────────────────────────────────────────

/// Detect whether the Windows OS is currently using a dark theme.
///
/// Reads `AppsUseLightTheme` from
/// `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize`.
/// When the value is `0` the OS is dark; when `1` (or absent) the OS is light.
///
/// This is the **OS-level** theme, not the in-app preference. Used primarily
/// for system-tray icon glyph colours — the tray context menu background is
/// drawn by Windows using this theme, so icon colours must match it.
///
/// Returns `true` (dark) when the registry value is `0`, `false` otherwise.
/// Falls back to `false` (light) on any registry error.
#[must_use]
pub fn is_system_dark_mode() -> bool {
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, KEY_READ, RRF_RT_REG_DWORD, RegGetValueW, RegOpenKeyExW,
    };

    const SUBKEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";
    const VALUE_NAME: &str = "AppsUseLightTheme";

    let subkey_wide: Vec<u16> = SUBKEY.encode_utf16().chain(Some(0u16)).collect();
    let value_wide: Vec<u16> = VALUE_NAME.encode_utf16().chain(Some(0u16)).collect();

    let mut hkey: windows_sys::Win32::System::Registry::HKEY = std::ptr::null_mut();

    // SAFETY: `RegOpenKeyExW` is a standard Win32 registry call.
    // `hkey` receives a valid handle on success. The wide-string slices are
    // null-terminated and live for the full call duration.
    let rc = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey_wide.as_ptr(),
            0,
            KEY_READ,
            &raw mut hkey,
        )
    };
    if rc != 0 {
        return false; // Cannot read registry — assume light
    }

    let mut data: u32 = 1; // default = light theme
    let mut data_size: u32 = std::mem::size_of::<u32>() as u32;

    // SAFETY: `RegGetValueW` reads a REG_DWORD value into `data`.
    // `data_size` is correctly set to 4 bytes and is updated by the call.
    let rc = unsafe {
        RegGetValueW(
            hkey,
            std::ptr::null(),
            value_wide.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            std::ptr::from_mut(&mut data).cast(),
            &raw mut data_size,
        )
    };

    // SAFETY: `RegCloseKey` with a valid handle opened above.
    unsafe {
        windows_sys::Win32::System::Registry::RegCloseKey(hkey);
    }

    if rc != 0 {
        return false; // Read failed — assume light
    }

    // AppsUseLightTheme: 0 = dark, 1 = light
    data == 0
}

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

    // Ordinal 135 — SetPreferredAppMode(mode: i32) -> i32
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

    // Ordinal 136 — FlushMenuThemes()
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

// ─── Balloon notification ────────────────────────────────────────────────────

/// Notification icon unique ID (arbitrary, scoped to this process).
const NOTIFY_ICON_UID: u32 = 0xBEEF;

/// Duration the balloon notification stays visible before auto-dismiss (ms).
const BALLOON_TIMEOUT_MS: u32 = 2000;

/// Balloon info icon style (shows an "i" icon).
/// From `shellapi.h` `NIIF_INFO` — not exposed by `windows-sys`.
const NIIF_INFO: u32 = 0x01;

/// Suppress the notification sound.
/// From `shellapi.h` `NIIF_NOSOUND` — not exposed by `windows-sys`.
const NIIF_NOSOUND: u32 = 0x10;

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

/// Attempt to acquire a system-wide named mutex for single-instance enforcement.
///
/// If no other instance holds the mutex, returns `Some(handle)` — the caller
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
        // CreateMutexW failed entirely — let the app launch anyway so
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

/// Restore a hidden window to the foreground.
///
/// Calls `ShowWindow(SW_SHOW)` + `SetForegroundWindow` so the window
/// becomes visible **and** receives input focus.  This is used by the
/// tray-watcher thread when the user selects "Open" or left-clicks the
/// tray icon.
///
/// Must be called **before** `egui::Context::request_repaint()` — the
/// repaint path (`RedrawWindow(RDW_INTERNALPAINT)`) is suppressed by
/// Windows for windows with `WS_VISIBLE` cleared, so the window must
/// be visible first.
///
/// Does nothing when `hwnd` is `0`.
pub fn restore_window(hwnd: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_SHOW, SetForegroundWindow, ShowWindow};

    if hwnd == 0 {
        return;
    }

    // SAFETY: `hwnd` is our own main window, valid for the lifetime of
    // the process.  ShowWindow + SetForegroundWindow are standard Win32
    // calls with no preconditions beyond a valid handle.
    unsafe {
        ShowWindow(hwnd as *mut _, SW_SHOW);
        SetForegroundWindow(hwnd as *mut _);
    }
}

/// Check whether the application window is currently visible (`WS_VISIBLE` set).
///
/// Returns `true` when the window has the `WS_VISIBLE` style flag, which is
/// set by `ShowWindow(SW_SHOW | SW_RESTORE)` and cleared by
/// `ShowWindow(SW_HIDE)` or `SetWindowPos` with `SWP_HIDEWINDOW`.
///
/// Used to detect when an external process (e.g. a second instance) restores
/// a window that the app thinks is still hidden to the tray.
///
/// Returns `false` when `hwnd` is `0`.
#[must_use]
pub fn is_window_visible(hwnd: isize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;

    if hwnd == 0 {
        return false;
    }

    // SAFETY: IsWindowVisible is a read-only state check on a valid owned window.
    unsafe { IsWindowVisible(hwnd as *mut _) != 0 }
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

/// Make a hidden window visible without activating or focusing it.
///
/// Calls `ShowWindow(SW_SHOWNOACTIVATE)` so the `WS_VISIBLE` flag is
/// set (enabling eframe repaints) but the window does **not** steal
/// focus.  Used by the tray-watcher thread for the "Quit" action: the
/// window needs to be visible just long enough for eframe to process
/// the close request.
///
/// Does nothing when `hwnd` is `0`.
pub fn reveal_window(hwnd: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_SHOWNOACTIVATE, ShowWindow};

    if hwnd == 0 {
        return;
    }

    // SAFETY: ShowWindow with a valid hwnd is safe.
    unsafe {
        ShowWindow(hwnd as *mut _, SW_SHOWNOACTIVATE);
    }
}

/// Immediately hide the window by calling `ShowWindow(SW_HIDE)` directly.
///
/// Unlike `egui::ViewportCommand::Visible` with `false`, which is an async
/// request processed by winit on the next event-loop cycle, this call
/// synchronously clears the `WS_VISIBLE` flag so the window disappears on
/// the current frame.
///
/// This is used as a belt-and-braces companion to the egui viewport command
/// in the close intercept.  When the window was previously restored by an
/// external process (e.g. a second instance via `ShowWindow(SW_RESTORE)`),
/// winit's internal visibility state may not yet reflect reality.  In that
/// case the async `Visible(false)` command is silently ignored, leaving the
/// window visible and producing a blank-flash / no-close symptom.  Calling
/// this function guarantees the window is hidden regardless of winit's
/// tracked state.
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
