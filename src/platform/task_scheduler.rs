//! Task Scheduler tasks, managed through `schtasks.exe`.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use super::wide::to_wide;

/// `CREATE_NO_WINDOW` process creation flag: keeps `schtasks.exe` from
/// flashing a console window.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Register (or replace) task `name` from a Task Scheduler XML definition.
///
/// schtasks reads the definition from a file, on behalf of this elevated
/// process. The file therefore lives in a private temporary folder that a
/// non-elevated process of the same user cannot tamper with; otherwise it
/// could swap the XML and get an arbitrary elevated task registered.
pub fn register_from_xml(name: &str, xml: &str) -> Result<()> {
    // schtasks expects UTF-16 LE with a byte-order mark, matching an
    // `encoding="UTF-16"` XML prolog.
    let mut bytes = Vec::with_capacity(2 + xml.len() * 2);
    bytes.extend_from_slice(&[0xFF, 0xFE]);
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }

    let dir = PrivateTempDir::create()?;
    let xml_path = dir.path.join("task.xml");
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&xml_path)
            .context("Cannot create task XML")?;
        file.write_all(&bytes).context("Cannot write task XML")?;
    }

    // `dir` removes the file and directory when dropped, on every path.
    run_schtasks(&[
        "/Create".as_ref(),
        "/TN".as_ref(),
        name.as_ref(),
        "/XML".as_ref(),
        xml_path.as_os_str(),
        "/F".as_ref(),
    ])?
    .map(drop)
}

/// Delete task `name`. A missing task counts as success.
pub fn delete(name: &str) -> Result<()> {
    let deleted = run_schtasks(&[
        "/Delete".as_ref(),
        "/TN".as_ref(),
        name.as_ref(),
        "/F".as_ref(),
    ])?;
    // A failed delete is only fine when the task is really gone. Checking
    // with a query (exit status) avoids parsing localised schtasks text.
    match deleted {
        Err(e) if exists(name)? => Err(e),
        _ => Ok(()),
    }
}

/// Whether task `name` exists.
///
/// # Errors
///
/// Fails if `schtasks.exe` cannot be run at all.
pub fn exists(name: &str) -> Result<bool> {
    Ok(query_xml(name)?.is_some())
}

/// The XML definition of task `name`, or `Ok(None)` if there is no such task.
///
/// # Errors
///
/// Fails if `schtasks.exe` cannot be run at all.
pub fn query_xml(name: &str) -> Result<Option<String>> {
    Ok(run_schtasks(&[
        "/Query".as_ref(),
        "/TN".as_ref(),
        name.as_ref(),
        "/XML".as_ref(),
    ])?
    .ok())
}

/// Run `schtasks.exe` with `args` without showing a console window.
///
/// The outer error means `schtasks.exe` could not be run. The inner result
/// is its outcome: its output on success, or an error with its stderr
/// (falling back to stdout) for a non-zero exit status.
fn run_schtasks(args: &[&std::ffi::OsStr]) -> Result<Result<String>> {
    use std::os::windows::process::CommandExt;

    // Use the absolute System32 path from the API, so this elevated process
    // never runs a `schtasks.exe` planted on PATH or behind a %SystemRoot%
    // value set by a non-elevated launcher.
    let exe = super::paths::system_directory()?.join("schtasks.exe");

    let output = std::process::Command::new(exe)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .context("Cannot run schtasks.exe")?;

    if output.status.success() {
        return Ok(Ok(decode_oem(&output.stdout)));
    }

    let stderr = decode_oem(&output.stderr);
    let detail = if stderr.trim().is_empty() {
        decode_oem(&output.stdout).trim().to_owned()
    } else {
        stderr.trim().to_owned()
    };
    Ok(Err(anyhow::anyhow!(
        "schtasks.exe failed ({}): {detail}",
        output.status
    )))
}

/// Decode console program output, which `schtasks.exe` writes in the OEM
/// code page (850, 866, 936, ...), not UTF-8.
fn decode_oem(bytes: &[u8]) -> String {
    use windows_sys::Win32::Globalization::{CP_OEMCP, MultiByteToWideChar};

    let Ok(len) = i32::try_from(bytes.len()) else {
        return String::from_utf8_lossy(bytes).into_owned();
    };
    if len == 0 {
        return String::new();
    }
    // SAFETY: `bytes` is valid for `len` bytes; a null output buffer asks
    // only for the required length in UTF-16 units.
    let needed =
        unsafe { MultiByteToWideChar(CP_OEMCP, 0, bytes.as_ptr(), len, std::ptr::null_mut(), 0) };
    let Ok(capacity) = usize::try_from(needed) else {
        return String::from_utf8_lossy(bytes).into_owned();
    };
    let mut wide = vec![0u16; capacity];
    // SAFETY: `wide` is writable for `needed` UTF-16 units, as just measured.
    let written =
        unsafe { MultiByteToWideChar(CP_OEMCP, 0, bytes.as_ptr(), len, wide.as_mut_ptr(), needed) };
    wide.truncate(usize::try_from(written).unwrap_or(0));
    String::from_utf16_lossy(&wide)
}

/// A freshly created temporary directory that only `SYSTEM` and the
/// Administrators group can access. Removed with its contents on drop.
struct PrivateTempDir {
    /// Full path of the directory.
    path: PathBuf,
}

impl PrivateTempDir {
    /// Create a new uniquely named directory under `%SystemRoot%\Temp` with a
    /// protected DACL. Fails if the directory already exists; it is never
    /// reused.
    ///
    /// The user's own `%TEMP%` is not safe even with a protected DACL: the
    /// user owns that folder, so any of their non-elevated processes can
    /// rename our directory away and plant a look-alike. In the Windows temp
    /// folder standard users may create entries but not delete or rename
    /// anyone else's. The Windows directory is read from the API rather than
    /// the environment, which a non-elevated launcher controls.
    fn create() -> Result<Self> {
        use std::hash::{BuildHasher, Hasher};

        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
        use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;

        // Protected DACL (no inheritance from the parent): full access for
        // SYSTEM and Administrators only, inherited by the file inside.
        const SDDL: &str = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";

        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u32(std::process::id());
        if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            hasher.write_u128(now.as_nanos());
        }
        let path = super::paths::windows_directory()?
            .join("Temp")
            .join(format!("magicx-{:016x}", hasher.finish()));

        let sddl_wide = to_wide(SDDL);
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: `sddl_wide` is a valid null-terminated UTF-16 string and
        // `descriptor` receives a LocalAlloc'd descriptor on success, freed below.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl_wide.as_ptr(),
                SDDL_REVISION_1,
                &raw mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            bail!(
                "Cannot build security descriptor: {}",
                std::io::Error::last_os_error()
            );
        }

        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let path_wide = to_wide(&path.to_string_lossy());
        // SAFETY: `path_wide` is null-terminated and `sa` points at a valid
        // descriptor for the duration of the call. CreateDirectoryW fails
        // (ERROR_ALREADY_EXISTS) rather than reusing an existing directory.
        let created = unsafe { CreateDirectoryW(path_wide.as_ptr(), &raw const sa) };
        let create_err = std::io::Error::last_os_error();
        // SAFETY: `descriptor` was allocated by the conversion call above and
        // is not used after this point.
        unsafe { LocalFree(descriptor) };

        if created == 0 {
            bail!("Cannot create private temp directory: {create_err}");
        }
        Ok(Self { path })
    }
}

impl Drop for PrivateTempDir {
    fn drop(&mut self) {
        // Named binding avoids `let_underscore_drop`; cleanup is best-effort.
        let _cleanup = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oem_output_decodes_ascii_and_empty_input() {
        assert_eq!(decode_oem(b"SUCCESS: done"), "SUCCESS: done");
        assert_eq!(decode_oem(b""), "");
    }

    #[test]
    fn a_missing_task_is_not_an_error() {
        assert!(!exists("MagicX test task that does not exist").expect("schtasks runs"));
        delete("MagicX test task that does not exist").expect("missing counts as deleted");
    }
}
