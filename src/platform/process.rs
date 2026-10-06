//! Process enumeration and per-process queries.

/// Executable name of this process's parent, via a Toolhelp snapshot.
///
/// Returns `None` if the snapshot fails or the parent has already exited.
#[must_use]
pub fn parent_process_name() -> Option<String> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    // SAFETY: Standard documented call; the handle is owned by the guard.
    let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if raw == INVALID_HANDLE_VALUE {
        return None;
    }
    let snapshot = crate::platform::handle::HandleGuard::new(raw);

    let mut entries: Vec<(u32, u32, String)> = Vec::new();
    // SAFETY: PROCESSENTRY32W is plain data; zeroing it and setting dwSize is
    // the documented initialisation.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: `snapshot` is a valid Toolhelp snapshot and `entry` is sized.
    let mut more = unsafe { Process32FirstW(snapshot.raw(), &raw mut entry) } != 0;
    while more {
        entries.push((
            entry.th32ProcessID,
            entry.th32ParentProcessID,
            crate::platform::wide::extract_exe_name(&entry.szExeFile),
        ));
        // SAFETY: As above.
        more = unsafe { Process32NextW(snapshot.raw(), &raw mut entry) } != 0;
    }

    let own_pid = std::process::id();
    let parent_pid = entries.iter().find(|e| e.0 == own_pid)?.1;
    entries.into_iter().find(|e| e.0 == parent_pid).map(|e| e.2)
}
