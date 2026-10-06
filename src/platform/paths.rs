//! Well-known Windows directories, resolved through the API.
//!
//! The process environment (`%SystemRoot%`, `%windir%`) is not used: an
//! elevated process inherits it from a non-elevated launcher, which could
//! point it anywhere.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use anyhow::{Result, bail};
use windows_sys::Win32::System::SystemInformation::{
    GetSystemDirectoryW, GetSystemWindowsDirectoryW,
};

/// Read a directory path through a `Get*DirectoryW`-style API.
fn read_directory(getter: unsafe extern "system" fn(*mut u16, u32) -> u32) -> Result<PathBuf> {
    let mut buf = [0u16; 260];
    // SAFETY: `buf` is a writable buffer of the stated length; these APIs
    // write at most that many UTF-16 units and return the length written.
    let len = unsafe { getter(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if len == 0 || len >= buf.len() {
        bail!(
            "Cannot locate a system directory: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(PathBuf::from(OsString::from_wide(&buf[..len])))
}

/// The Windows directory, e.g. `C:\Windows`.
pub fn windows_directory() -> Result<PathBuf> {
    read_directory(GetSystemWindowsDirectoryW)
}

/// The system directory, e.g. `C:\Windows\System32`.
pub fn system_directory() -> Result<PathBuf> {
    read_directory(GetSystemDirectoryW)
}
