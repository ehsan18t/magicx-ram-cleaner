//! Ownership of Win32 kernel handles.

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};

/// RAII wrapper for Win32 `HANDLE` values.
///
/// Automatically calls `CloseHandle` on drop, preventing handle leaks if a
/// panic occurs between the `Open*` / `CreateToolhelp32Snapshot` call and the
/// explicit `CloseHandle`. Null and `INVALID_HANDLE_VALUE` handles are not
/// closed (they are never valid).
pub struct HandleGuard {
    handle: HANDLE,
}

impl HandleGuard {
    /// Wrap a raw `HANDLE`. The caller must ensure the handle is valid
    /// and needs closing, or is null / `INVALID_HANDLE_VALUE`.
    pub const fn new(handle: HANDLE) -> Self {
        Self { handle }
    }

    /// Borrow the underlying handle for FFI calls.
    #[must_use]
    pub const fn raw(&self) -> HANDLE {
        self.handle
    }
}

impl Drop for HandleGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() && self.handle != INVALID_HANDLE_VALUE {
            // SAFETY: handle is a valid, open Win32 handle that must be closed.
            // CloseHandle is safe for any valid handle and idempotent for closed ones.
            unsafe { CloseHandle(self.handle) };
        }
    }
}
