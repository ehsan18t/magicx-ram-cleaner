//! Single-instance enforcement for the GUI.

use std::os::windows::io::OwnedHandle;

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows_sys::Win32::System::Threading::CreateMutexW;

use super::handle::owned_or_null;
use super::wide::to_wide;
use super::window::{bring_to_front, find_app_window};

/// Mutex name, scoped to the current user's session.
const MUTEX_NAME: &str = "Local\\MagicXRamCleanerSingleInstance";

/// Proof that this process is the only running GUI instance. Dropping it
/// releases the claim, so keep it alive for the lifetime of the GUI.
#[derive(Debug)]
pub struct SingleInstance {
    /// The named mutex, or `None` if it could not be created (see
    /// [`SingleInstance::acquire`]).
    _mutex: Option<OwnedHandle>,
}

impl SingleInstance {
    /// Claim the single-instance mutex.
    ///
    /// Returns `None` if another instance already holds it; in that case the
    /// other instance's window is restored and brought to the front, and the
    /// caller should exit. If the mutex cannot be created at all, the launch
    /// is allowed so a transient OS error never locks the user out.
    #[must_use]
    pub fn acquire() -> Option<Self> {
        let name = to_wide(MUTEX_NAME);
        // SAFETY: `name` is a valid null-terminated wide string. The returned
        // handle (or null) is owned below and closed exactly once.
        let mutex = unsafe { owned_or_null(CreateMutexW(std::ptr::null(), 0, name.as_ptr())) };
        // SAFETY: Reads the last-error value set by CreateMutexW just above.
        let already_exists = mutex.is_some() && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;

        if already_exists {
            // Our handle to the other instance's mutex is closed on return.
            restore_existing_window();
            return None;
        }
        Some(Self { _mutex: mutex })
    }
}

/// Bring the already running instance's window to the front.
fn restore_existing_window() {
    bring_to_front(find_app_window(crate::ids::WINDOW_TITLE));
}
