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
    ])
}

/// Delete task `name`. A missing task counts as success.
pub fn delete(name: &str) -> Result<()> {
    let deleted = run_schtasks(&[
        "/Delete".as_ref(),
        "/TN".as_ref(),
        name.as_ref(),
        "/F".as_ref(),
    ]);
    // A failed delete is only fine when the task is really gone. Checking
    // with a query (exit status) avoids parsing localised schtasks text, and
    // a delete is always attempted, so a failing query cannot hide a task.
    match deleted {
        Err(_) if !exists(name) => Ok(()),
        other => other,
    }
}

/// Whether task `name` exists.
#[must_use]
pub fn exists(name: &str) -> bool {
    run_schtasks(&["/Query".as_ref(), "/TN".as_ref(), name.as_ref()]).is_ok()
}

/// Run `schtasks.exe` with `args` without showing a console window.
///
/// Fails on a non-zero exit status with the `schtasks` stderr (falling back
/// to stdout) in the message.
fn run_schtasks(args: &[&std::ffi::OsStr]) -> Result<()> {
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
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = if stderr.trim().is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    } else {
        stderr.trim().to_owned()
    };
    bail!("schtasks.exe failed ({}): {detail}", output.status)
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
