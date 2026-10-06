//! Windows registry access: one owned key type plus the few operations the
//! app needs (create, set string values, delete trees and values, test for
//! existence).

use anyhow::{Result, bail};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, KEY_ALL_ACCESS, KEY_READ, KEY_SET_VALUE,
    REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS, REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteTreeW,
    RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
};

use super::wide::to_wide;

/// `ERROR_FILE_NOT_FOUND`: the key or value does not exist.
const ERROR_FILE_NOT_FOUND: u32 = 2;

/// A predefined registry root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    /// `HKEY_CLASSES_ROOT` (machine-wide shell registrations; needs elevation
    /// to write).
    ClassesRoot,
    /// `HKEY_CURRENT_USER` (settings of the user this process runs as).
    CurrentUser,
}

impl Hive {
    /// The predefined `HKEY` handle for this root.
    const fn raw(self) -> HKEY {
        match self {
            Self::ClassesRoot => HKEY_CLASSES_ROOT,
            Self::CurrentUser => HKEY_CURRENT_USER,
        }
    }
}

/// An open registry key, closed when dropped.
#[derive(Debug)]
pub struct RegKey(HKEY);

impl RegKey {
    /// Open `path` under `hive` with `access`. `Ok(None)` if it does not exist.
    fn open(hive: Hive, path: &str, access: REG_SAM_FLAGS) -> Result<Option<Self>> {
        let wide = to_wide(path);
        let mut hkey: HKEY = std::ptr::null_mut();
        // SAFETY: `wide` is a null-terminated UTF-16 string and `hkey` a valid
        // out pointer; on success the handle is owned by the returned RegKey.
        let rc = unsafe { RegOpenKeyExW(hive.raw(), wide.as_ptr(), 0, access, &raw mut hkey) };
        match rc {
            0 => Ok(Some(Self(hkey))),
            ERROR_FILE_NOT_FOUND => Ok(None),
            _ => bail!("RegOpenKeyExW failed for '{path}': error {rc}"),
        }
    }

    /// Open or create `path` under `hive` for writing.
    pub fn create(hive: Hive, path: &str) -> Result<Self> {
        let wide = to_wide(path);
        let mut hkey: HKEY = std::ptr::null_mut();
        let mut disposition: u32 = 0;
        // SAFETY: `wide` is a null-terminated UTF-16 string; `hkey` and
        // `disposition` are valid out pointers. On success the handle is owned
        // by the returned RegKey.
        let rc = unsafe {
            RegCreateKeyExW(
                hive.raw(),
                wide.as_ptr(),
                0,
                std::ptr::null_mut(),
                REG_OPTION_NON_VOLATILE,
                KEY_ALL_ACCESS,
                std::ptr::null(),
                &raw mut hkey,
                &raw mut disposition,
            )
        };
        if rc != 0 {
            bail!("RegCreateKeyExW failed for '{path}': error {rc}");
        }
        Ok(Self(hkey))
    }

    /// Set a `REG_SZ` value. Use `""` as `name` for the key's default value.
    pub fn set_string(&self, name: &str, value: &str) -> Result<()> {
        let wide_name = to_wide(name);
        let wide_value = to_wide(value);
        // Byte length including the null terminator (REG_SZ requires it).
        let byte_len = (wide_value.len() * std::mem::size_of::<u16>()) as u32;
        // SAFETY: Both strings are valid null-terminated UTF-16 buffers and
        // `byte_len` is the exact size of `wide_value` in bytes.
        let rc = unsafe {
            RegSetValueExW(
                self.0,
                wide_name.as_ptr(),
                0,
                REG_SZ,
                wide_value.as_ptr().cast(),
                byte_len,
            )
        };
        if rc != 0 {
            bail!("RegSetValueExW failed for value '{name}': error {rc}");
        }
        Ok(())
    }
}

impl Drop for RegKey {
    fn drop(&mut self) {
        // SAFETY: The handle came from a successful open/create call and is
        // closed exactly once, here.
        unsafe { RegCloseKey(self.0) };
    }
}

/// Whether `path` exists under `hive`.
#[must_use]
pub fn key_exists(hive: Hive, path: &str) -> bool {
    matches!(RegKey::open(hive, path, KEY_READ), Ok(Some(_)))
}

/// Delete `path` and everything below it. A missing key counts as success.
pub fn delete_tree(hive: Hive, path: &str) -> Result<()> {
    let wide = to_wide(path);
    // SAFETY: `wide` is a null-terminated UTF-16 string and the hive handle
    // is a predefined, always-valid root key.
    let rc = unsafe { RegDeleteTreeW(hive.raw(), wide.as_ptr()) };
    if rc != 0 && rc != ERROR_FILE_NOT_FOUND {
        bail!("RegDeleteTreeW failed for '{path}': error {rc}");
    }
    Ok(())
}

/// Delete value `name` of key `path`. A missing key or value counts as
/// success.
pub fn delete_value(hive: Hive, path: &str, name: &str) -> Result<()> {
    let Some(key) = RegKey::open(hive, path, KEY_SET_VALUE)? else {
        return Ok(());
    };
    let wide_name = to_wide(name);
    // SAFETY: `key` is an open key and `wide_name` a null-terminated string.
    let rc = unsafe { RegDeleteValueW(key.0, wide_name.as_ptr()) };
    if rc != 0 && rc != ERROR_FILE_NOT_FOUND {
        bail!("RegDeleteValueW failed for '{path}\\{name}': error {rc}");
    }
    Ok(())
}
