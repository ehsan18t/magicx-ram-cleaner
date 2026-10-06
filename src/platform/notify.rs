//! Transient balloon notifications near the system tray, used by the
//! context-menu (`--notify`) launches.

use super::wide::{wide_literal, write_wide_into};

use anyhow::{Result, bail};

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
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, DestroyWindow, WM_APP};

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
        // SAFETY: DestroyWindow with a valid hwnd from CreateWindowExW, and
        // DestroyIcon with the icon LoadImageW created (null is ignored).
        unsafe {
            DestroyWindow(hwnd);
            DestroyIcon(hicon);
        }
        bail!("Shell_NotifyIconW(NIM_ADD) failed");
    }

    // NOTIFYICON_VERSION (v3) gives the icon the balloon behaviour this
    // module relies on (the uTimeout hint, the callback messages). Windows
    // 10 and 11 may still present the balloon as a toast.
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
    // the hidden message window. The icon came from LoadImageW at an
    // explicit size, so it is not shared and is ours to destroy, after the
    // shell no longer uses it.
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &raw const nid);
        DestroyWindow(hwnd);
        DestroyIcon(hicon);
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

/// Load the application icon (resource ID 1) from the running executable at
/// the small-icon size the notification area shows, so Windows picks the
/// matching frame instead of shrinking the 32 px one. The caller destroys it.
///
/// Falls back to a null handle if loading fails (the balloon will show
/// without a custom icon, using the default info icon instead).
fn load_app_icon() -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW, SM_CXSMICON, SM_CYSMICON,
    };

    // Get the module handle for the current exe (HINSTANCE == HMODULE for exe)
    let hinstance = get_exe_hinstance();

    // MAKEINTRESOURCEW(1) = resource ID 1 = app.ico
    // This is the standard Win32 MAKEINTRESOURCE pattern: an integer packed
    // into the low 16 bits of a pointer. Not a real dereferenceable address.
    let resource_id = std::ptr::without_provenance::<u16>(1);

    // SAFETY: GetSystemMetrics only reads system settings. LoadImageW gets a
    // valid hinstance and numeric resource ID and returns null on failure
    // (handled by the caller).
    unsafe {
        LoadImageW(
            hinstance,
            resource_id,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            LR_DEFAULTCOLOR,
        )
    }
}

/// Get the `HINSTANCE` of the running executable.
fn get_exe_hinstance() -> windows_sys::Win32::Foundation::HINSTANCE {
    // For an .exe, HINSTANCE == the base address of the module.
    // GetModuleHandleW(null) returns the handle of the exe itself.
    // SAFETY: GetModuleHandleW(null) is a standard Win32 call.
    unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()) }
}
