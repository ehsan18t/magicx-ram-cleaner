//! # `MagicX` RAM Cleaner - Privilege Management
//!
//! Handles Windows security privilege elevation required for memory operations.
//! Most memory cleaning operations require `SeProfileSingleProcessPrivilege`,
//! and file cache operations require `SeIncreaseQuotaPrivilege`.

use anyhow::{Context, Result, bail};
use windows_sys::Win32::Foundation::{ERROR_NOT_ALL_ASSIGNED, HANDLE, LUID};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES,
    TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use std::os::windows::io::AsRawHandle;

use crate::platform::{handle::owned_or_null, wide::to_wide};

/// Enable a named Windows privilege on the current process token.
///
/// # Privileges used by `MagicX`
///
/// | Privilege | Required For |
/// |---|---|
/// | `SeProfileSingleProcessPrivilege` | `NtSetSystemInformation` memory commands |
/// | `SeIncreaseQuotaPrivilege` | `SetSystemFileCacheSize` |
/// | `SeDebugPrivilege` | Opening system/protected process handles |
///
/// # Errors
///
/// Returns an error if the privilege cannot be looked up or adjusted.
pub fn enable_privilege(privilege_name: &str) -> Result<()> {
    // SAFETY: All pointers point to valid stack-allocated variables with correct
    // sizes. The token handle is owned (and closed) by an OwnedHandle.
    unsafe {
        let mut raw_token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &raw mut raw_token,
        ) == 0
        {
            bail!(
                "OpenProcessToken failed ({}). Are you running as Administrator?",
                std::io::Error::last_os_error()
            );
        }
        // Closed automatically on all exit paths.
        let Some(token) = owned_or_null(raw_token) else {
            bail!("OpenProcessToken returned no token");
        };

        let wide_name = to_wide(privilege_name);
        let mut luid = LUID {
            LowPart: 0,
            HighPart: 0,
        };

        if LookupPrivilegeValueW(std::ptr::null(), wide_name.as_ptr(), &raw mut luid) == 0 {
            bail!(
                "LookupPrivilegeValueW failed for '{privilege_name}' ({})",
                std::io::Error::last_os_error()
            );
        }

        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [windows_sys::Win32::Security::LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };

        if AdjustTokenPrivileges(
            token.as_raw_handle(),
            0, // do not disable all
            &raw const tp,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ) == 0
        {
            let err = std::io::Error::last_os_error();
            bail!("AdjustTokenPrivileges failed for '{privilege_name}' ({err})");
        }

        // AdjustTokenPrivileges can succeed but still fail to set:
        let err = std::io::Error::last_os_error().raw_os_error();
        // token guard dropped here - CloseHandle called automatically

        if err == Some(ERROR_NOT_ALL_ASSIGNED as i32) {
            bail!("Privilege '{privilege_name}' not held by this account. Run as Administrator.");
        }

        Ok(())
    }
}

/// Enable all privileges needed for full RAM cleaning.
///
/// Enables `SeProfileSingleProcessPrivilege` (memory list commands) and
/// `SeIncreaseQuotaPrivilege` (file cache management).  `SeDebugPrivilege`
/// is attempted but its failure is silently ignored since it is optional
/// (only needed for trimming protected processes).
///
/// # Errors
///
/// Returns an error if a mandatory privilege cannot be enabled.
pub fn enable_all_privileges() -> Result<()> {
    enable_privilege("SeProfileSingleProcessPrivilege")
        .context("Required for memory list operations")?;
    enable_privilege("SeIncreaseQuotaPrivilege").context("Required for file cache management")?;
    // SeDebugPrivilege is optional - allows trimming protected processes
    drop(enable_privilege("SeDebugPrivilege"));
    Ok(())
}

/// Verify that the process is running with Administrator elevation.
///
/// Uses `CheckTokenMembership` with the built-in Administrators group SID
/// (`S-1-5-32-544`) to confirm true elevation, not just token access.
///
/// # Errors
///
/// Returns an error with a user-friendly message if the process is not
/// elevated or if the check itself fails.
pub fn check_admin() -> Result<()> {
    use windows_sys::Win32::Security::{
        CheckTokenMembership, CreateWellKnownSid, SECURITY_MAX_SID_SIZE,
        WinBuiltinAdministratorsSid,
    };

    // A SID buffer of the maximum size, aligned for the SID structure.
    let mut sid = [0usize; (SECURITY_MAX_SID_SIZE as usize).div_ceil(size_of::<usize>())];
    let mut sid_len = size_of_val(&sid) as u32;
    // SAFETY: `sid` is a writable buffer of `sid_len` bytes, enough for any
    // SID; the domain SID is null, as the BUILTIN group needs none.
    let created = unsafe {
        CreateWellKnownSid(
            WinBuiltinAdministratorsSid,
            std::ptr::null_mut(),
            sid.as_mut_ptr().cast(),
            &raw mut sid_len,
        )
    };
    if created == 0 {
        bail!(
            "Cannot build the Administrators group SID to check elevation: {}",
            std::io::Error::last_os_error()
        );
    }

    let mut is_member: i32 = 0;
    // SAFETY: A null token means "the calling thread's token"; `sid` holds a
    // valid SID built above and `is_member` is a valid out pointer.
    let checked = unsafe {
        CheckTokenMembership(
            std::ptr::null_mut(),
            sid.as_mut_ptr().cast(),
            &raw mut is_member,
        )
    };
    if checked == 0 {
        bail!(
            "Cannot check administrator membership: {}",
            std::io::Error::last_os_error()
        );
    }
    if is_member == 0 {
        bail!(
            "Not running as Administrator.
             Right-click Command Prompt or PowerShell → 'Run as administrator'"
        );
    }

    Ok(())
}
