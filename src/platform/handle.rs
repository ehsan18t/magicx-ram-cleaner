//! Taking ownership of Win32 kernel handles as [`OwnedHandle`].
//!
//! Win32 APIs report failure with either a null handle (`OpenProcess`,
//! `CreateMutexW`, ...) or `INVALID_HANDLE_VALUE` (`CreateToolhelp32Snapshot`,
//! `CreateFileW`, ...). These helpers turn both conventions into an
//! `Option<OwnedHandle>`, which closes the handle exactly once when dropped.

use std::os::windows::io::{HandleOrInvalid, HandleOrNull, OwnedHandle};

use windows_sys::Win32::Foundation::HANDLE;

/// Own a handle from an API that returns null on failure.
///
/// # Safety
///
/// `raw` must be null or a valid, open handle that the caller owns and does
/// not close or use as owned elsewhere.
pub unsafe fn owned_or_null(raw: HANDLE) -> Option<OwnedHandle> {
    // SAFETY: Upheld by the caller.
    unsafe { HandleOrNull::from_raw_handle(raw) }
        .try_into()
        .ok()
}

/// Own a handle from an API that returns `INVALID_HANDLE_VALUE` on failure.
///
/// # Safety
///
/// `raw` must be `INVALID_HANDLE_VALUE` or a valid, open handle that the
/// caller owns and does not close or use as owned elsewhere.
pub unsafe fn owned_or_invalid(raw: HANDLE) -> Option<OwnedHandle> {
    // SAFETY: Upheld by the caller.
    unsafe { HandleOrInvalid::from_raw_handle(raw) }
        .try_into()
        .ok()
}
