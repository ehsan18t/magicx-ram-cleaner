//! # `MagicX` RAM Cleaner — Privilege Management
//!
//! Handles Windows security privilege elevation required for memory operations.
//! Most memory cleaning operations require `SeProfileSingleProcessPrivilege`,
//! and file cache operations require `SeIncreaseQuotaPrivilege`.

use anyhow::{Context, Result, bail};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LUID};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES,
    TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Encode a UTF-16 null-terminated string from a Rust &str.
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

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
    // sizes. The token handle is closed on every code path (success and error).
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &raw mut token,
        ) == 0
        {
            bail!(
                "OpenProcessToken failed (error {}). Are you running as Administrator?",
                get_last_error()
            );
        }

        let wide_name = to_wide(privilege_name);
        let mut luid = LUID {
            LowPart: 0,
            HighPart: 0,
        };

        if LookupPrivilegeValueW(std::ptr::null(), wide_name.as_ptr(), &raw mut luid) == 0 {
            CloseHandle(token);
            bail!(
                "LookupPrivilegeValueW failed for '{}' (error {})",
                privilege_name,
                get_last_error()
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
            token,
            0, // do not disable all
            &raw const tp,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ) == 0
        {
            let err = get_last_error();
            CloseHandle(token);
            bail!("AdjustTokenPrivileges failed for '{privilege_name}' (error {err})");
        }

        // AdjustTokenPrivileges can succeed but still fail to set:
        let err = get_last_error();
        CloseHandle(token);

        if err == 1300 {
            // ERROR_NOT_ALL_ASSIGNED
            bail!("Privilege '{privilege_name}' not held by this account. Run as Administrator.");
        }

        Ok(())
    }
}

/// Enable all privileges needed for full RAM cleaning.
pub fn enable_all_privileges() -> Result<()> {
    enable_privilege("SeProfileSingleProcessPrivilege")
        .context("Required for memory list operations")?;
    enable_privilege("SeIncreaseQuotaPrivilege").context("Required for file cache management")?;
    // SeDebugPrivilege is optional — allows trimming protected processes
    drop(enable_privilege("SeDebugPrivilege"));
    Ok(())
}

/// Get the last Win32 error code.
fn get_last_error() -> u32 {
    unsafe { windows_sys::Win32::Foundation::GetLastError() }
}
