//! The identity this process runs as.

use std::os::windows::io::AsRawHandle;

use anyhow::{Result, bail};
use windows_sys::Win32::Security::{
    GetTokenInformation, LookupAccountSidW, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use super::handle::owned_or_null;

/// The account this process runs as, as `DOMAIN\user`.
///
/// Read from the process token rather than the `USERNAME` / `USERDOMAIN`
/// environment variables, which an elevated process inherits from a
/// possibly non-elevated launcher that could set them to anything.
pub fn current_account() -> Result<String> {
    let mut raw_token = std::ptr::null_mut();
    // SAFETY: GetCurrentProcess returns a pseudo-handle; `raw_token` is a
    // valid out pointer whose handle is owned right after.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut raw_token) } == 0 {
        bail!(
            "OpenProcessToken failed: {}",
            std::io::Error::last_os_error()
        );
    }
    // SAFETY: OpenProcessToken succeeded, so this is a new handle we own.
    let Some(token) = (unsafe { owned_or_null(raw_token) }) else {
        bail!("OpenProcessToken returned no token");
    };

    // TOKEN_USER is followed by the SID it points to; 256 bytes covers the
    // largest possible SID. A u64 array keeps the buffer suitably aligned.
    let mut buffer = [0u64; 32];
    let mut needed = 0u32;
    // SAFETY: `buffer` is writable for the stated byte length and `needed`
    // is a valid out pointer.
    let ok = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            std::mem::size_of_val(&buffer) as u32,
            &raw mut needed,
        )
    };
    if ok == 0 {
        bail!(
            "GetTokenInformation failed: {}",
            std::io::Error::last_os_error()
        );
    }
    // SAFETY: On success the buffer starts with an initialised, aligned
    // TOKEN_USER whose SID pointer refers into the same buffer.
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };

    let mut name = [0u16; 256];
    let mut domain = [0u16; 256];
    let (mut name_len, mut domain_len) = (name.len() as u32, domain.len() as u32);
    let mut sid_use: SID_NAME_USE = 0;
    // SAFETY: `sid` is valid while `buffer` lives; both output buffers are
    // writable for the lengths passed in.
    let ok = unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            sid,
            name.as_mut_ptr(),
            &raw mut name_len,
            domain.as_mut_ptr(),
            &raw mut domain_len,
            &raw mut sid_use,
        )
    };
    if ok == 0 {
        bail!(
            "LookupAccountSidW failed: {}",
            std::io::Error::last_os_error()
        );
    }

    let name = String::from_utf16_lossy(&name[..name_len as usize]);
    let domain = String::from_utf16_lossy(&domain[..domain_len as usize]);
    Ok(if domain.is_empty() {
        name
    } else {
        format!("{domain}\\{name}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_account_names_the_running_user() {
        let account = current_account().expect("the process token is readable");
        let user = account.rsplit('\\').next().unwrap_or_default();
        assert!(!user.is_empty(), "empty account name: {account:?}");
    }
}
