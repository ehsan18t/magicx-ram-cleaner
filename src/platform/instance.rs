//! Single-instance enforcement for the GUI.

use super::window::find_app_window;

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
