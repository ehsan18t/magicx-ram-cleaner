//! Process enumeration, per-process memory counters and working-set trimming.

use anyhow::{Result, bail};
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::ProcessStatus::{
    K32EmptyWorkingSet, K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    PROCESS_MEMORY_COUNTERS_EX2,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_ACCESS_RIGHTS, PROCESS_QUERY_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA,
};

use super::handle::HandleGuard;
use super::wide::extract_exe_name;

/// One running process, as listed by a Toolhelp snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEntry {
    /// Process ID.
    pub pid: u32,
    /// Parent process ID (may belong to a process that has since exited).
    pub parent_pid: u32,
    /// Executable file name, e.g. `chrome.exe`.
    pub name: String,
}

/// List all running processes except System Idle (PID 0) and System (PID 4),
/// which can never be opened or trimmed.
pub fn processes() -> Result<Vec<ProcessEntry>> {
    // SAFETY: CreateToolhelp32Snapshot with TH32CS_SNAPPROCESS and 0 is the
    // standard documented way to enumerate all running processes.
    let snap_raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap_raw == INVALID_HANDLE_VALUE {
        bail!("CreateToolhelp32Snapshot failed");
    }
    let snapshot = HandleGuard::new(snap_raw);

    // SAFETY: PROCESSENTRY32W is plain data; zeroing it and setting dwSize is
    // the documented initialisation.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

    let mut entries = Vec::new();
    // SAFETY: Process32FirstW/Process32NextW iterate the Toolhelp snapshot.
    // The entry struct is properly zeroed and sized.
    let mut has_entry = unsafe { Process32FirstW(snapshot.raw(), &raw mut entry) } != 0;
    while has_entry {
        let pid = entry.th32ProcessID;
        if pid != 0 && pid != 4 {
            entries.push(ProcessEntry {
                pid,
                parent_pid: entry.th32ParentProcessID,
                name: extract_exe_name(&entry.szExeFile),
            });
        }
        // SAFETY: As above.
        has_entry = unsafe { Process32NextW(snapshot.raw(), &raw mut entry) } != 0;
    }
    Ok(entries)
}

/// Executable name of this process's parent.
///
/// Returns `None` if enumeration fails or the parent has already exited.
#[must_use]
pub fn parent_process_name() -> Option<String> {
    let entries = processes().ok()?;
    let own_pid = std::process::id();
    let parent_pid = entries.iter().find(|e| e.pid == own_pid)?.parent_pid;
    entries
        .into_iter()
        .find(|e| e.pid == parent_pid)
        .map(|e| e.name)
}

/// Physical-memory counters of one process, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryCounters {
    /// Current working set (shared + private pages).
    pub working_set: u64,
    /// Peak working set since the process started.
    pub peak_working_set: u64,
    /// Private working set (what Task Manager's "Memory" column shows), or
    /// the full working set on builds without `PROCESS_MEMORY_COUNTERS_EX2`.
    pub private_working_set: u64,
}

/// Open `pid` with `access`, or `None` if access is denied or it has exited.
fn open_process(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> Option<HandleGuard> {
    // SAFETY: OpenProcess has no memory-safety preconditions; the handle (or
    // null on failure) is owned by the guard.
    let handle = HandleGuard::new(unsafe { OpenProcess(access, 0, pid) });
    (!handle.raw().is_null()).then_some(handle)
}

/// Query the memory counters of process `pid`. Returns `None` if the process
/// cannot be opened (protected/system processes) or queried.
///
/// Tries `PROCESS_MEMORY_COUNTERS_EX2` first (Windows 10 1709+) to obtain
/// `PrivateWorkingSetSize`, the metric Task Manager shows as "Memory", and
/// falls back to `PROCESS_MEMORY_COUNTERS` on older builds.
///
/// Uses a tiered `OpenProcess` strategy to maximise process visibility:
///
/// 1. `PROCESS_QUERY_INFORMATION` - sufficient for `K32GetProcessMemoryInfo`
///    including the EX2 struct with `PrivateWorkingSetSize`.
/// 2. `PROCESS_QUERY_LIMITED_INFORMATION` - weaker right that succeeds for
///    Chromium/Electron sandboxed child processes and Protected Process Light
///    (PPL) processes whose DACLs deny full query access.
///
/// `PROCESS_VM_READ` is intentionally **not** requested: it is not required
/// by `K32GetProcessMemoryInfo` and makes `OpenProcess` fail for sandboxed
/// processes, leading to missing entries and inaccurate RAM totals.
#[must_use]
pub fn memory_counters(pid: u32) -> Option<MemoryCounters> {
    let handle = open_process(pid, PROCESS_QUERY_INFORMATION)
        .or_else(|| open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION))?;

    // SAFETY: PROCESS_MEMORY_COUNTERS_EX2 is zeroed, cb is set to its size,
    // and the handle is a valid process handle.
    let ex2 = unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS_EX2 = std::mem::zeroed();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
        let ok = K32GetProcessMemoryInfo(
            handle.raw(),
            std::ptr::from_mut(&mut counters).cast::<PROCESS_MEMORY_COUNTERS>(),
            counters.cb,
        );
        (ok != 0).then_some(counters)
    };
    if let Some(c) = ex2 {
        return Some(MemoryCounters {
            working_set: c.WorkingSetSize as u64,
            peak_working_set: c.PeakWorkingSetSize as u64,
            private_working_set: c.PrivateWorkingSetSize as u64,
        });
    }

    // SAFETY: PROCESS_MEMORY_COUNTERS is zeroed, cb is set to its size, and
    // the handle is a valid process handle.
    let base = unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let ok = K32GetProcessMemoryInfo(handle.raw(), &raw mut counters, counters.cb);
        (ok != 0).then_some(counters)
    }?;
    Some(MemoryCounters {
        working_set: base.WorkingSetSize as u64,
        peak_working_set: base.PeakWorkingSetSize as u64,
        // No EX2 data available: use the full working set as the fallback.
        private_working_set: base.WorkingSetSize as u64,
    })
}

/// Remove as many pages as possible from the working set of process `pid`.
///
/// Returns `false` if the process cannot be opened (protected or system
/// processes) or the trim fails. `EmptyWorkingSet` needs `PROCESS_SET_QUOTA`
/// plus either query right; the limited one is granted to far more processes
/// (sandboxed browser children, services), so more of them get trimmed.
#[must_use]
pub fn empty_working_set(pid: u32) -> bool {
    open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA)
        // SAFETY: The handle is a valid process handle with the required rights.
        .is_some_and(|handle| unsafe { K32EmptyWorkingSet(handle.raw()) } != 0)
}
