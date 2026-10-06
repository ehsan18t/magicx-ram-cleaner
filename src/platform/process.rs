//! Process enumeration, per-process memory counters and working-set trimming.

use anyhow::{Result, bail};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::ProcessStatus::{
    K32EmptyWorkingSet, K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    PROCESS_MEMORY_COUNTERS_EX2,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_ACCESS_RIGHTS, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SET_QUOTA, QueryFullProcessImageNameW,
};

use std::os::windows::io::{AsRawHandle, OwnedHandle};

use super::handle::{owned_or_invalid, owned_or_null};
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
    // The snapshot handle is owned and closed when dropped.
    let Some(snapshot) =
        (unsafe { owned_or_invalid(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)) })
    else {
        bail!("CreateToolhelp32Snapshot failed");
    };

    // SAFETY: PROCESSENTRY32W is plain data; zeroing it and setting dwSize is
    // the documented initialisation.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

    let mut entries = Vec::new();
    // SAFETY: Process32FirstW/Process32NextW iterate the Toolhelp snapshot.
    // The entry struct is properly zeroed and sized.
    let mut has_entry = unsafe { Process32FirstW(snapshot.as_raw_handle(), &raw mut entry) } != 0;
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
        has_entry = unsafe { Process32NextW(snapshot.as_raw_handle(), &raw mut entry) } != 0;
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

/// What [`trim`] did to a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrimOutcome {
    /// The working set was emptied; `freed_bytes` is how much it shrank.
    Trimmed {
        /// Working-set bytes given back (0 if it could not be measured).
        freed_bytes: u64,
    },
    /// Windows refused: a protected process, or one the app may not touch.
    Denied,
    /// The process has exited (or its PID now belongs to another process).
    Exited,
}

/// Why a process could not be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenError {
    /// No process with that PID exists any more.
    Exited,
    /// The process exists but Windows denied the requested access.
    Denied,
}

/// An open handle to one process. Every query on a process goes through a
/// single handle, so a refresh opens each process once and all of its
/// figures describe the same process.
pub struct ProcessHandle(OwnedHandle);

impl ProcessHandle {
    /// Open `pid` for queries.
    ///
    /// `PROCESS_QUERY_LIMITED_INFORMATION` is enough for every query here
    /// (memory counters including `PROCESS_MEMORY_COUNTERS_EX2`, the image
    /// path and the start time) and, unlike `PROCESS_QUERY_INFORMATION`, it
    /// is granted for Chromium/Electron sandboxed children and Protected
    /// Process Light processes. `PROCESS_VM_READ` is deliberately not
    /// requested: nothing needs it and it would make those opens fail.
    pub fn open(pid: u32) -> Result<Self, OpenError> {
        Self::open_with(pid, PROCESS_QUERY_LIMITED_INFORMATION)
    }

    /// Open `pid` for queries plus trimming (`PROCESS_SET_QUOTA`).
    pub fn open_for_trim(pid: u32) -> Result<Self, OpenError> {
        Self::open_with(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA)
    }

    /// Open `pid` with `access`.
    fn open_with(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> Result<Self, OpenError> {
        use windows_sys::Win32::Foundation::{ERROR_INVALID_PARAMETER, GetLastError};

        // SAFETY: OpenProcess returns null or a new handle that we own.
        if let Some(handle) = unsafe { owned_or_null(OpenProcess(access, 0, pid)) } {
            return Ok(Self(handle));
        }
        // SAFETY: Reads the calling thread's last-error value, set by the
        // failed OpenProcess just above.
        let error = unsafe { GetLastError() };
        // OpenProcess reports a PID that no longer exists as an invalid
        // parameter; everything else (mostly access denied) is a refusal.
        Err(if error == ERROR_INVALID_PARAMETER {
            OpenError::Exited
        } else {
            OpenError::Denied
        })
    }

    /// The process's physical-memory counters, or `None` if the query fails.
    ///
    /// Tries `PROCESS_MEMORY_COUNTERS_EX2` first (Windows 10 1709+) to obtain
    /// `PrivateWorkingSetSize`, the metric Task Manager shows as "Memory", and
    /// falls back to `PROCESS_MEMORY_COUNTERS` on older builds.
    #[must_use]
    pub fn memory_counters(&self) -> Option<MemoryCounters> {
        let handle = self.0.as_raw_handle();
        // SAFETY: PROCESS_MEMORY_COUNTERS_EX2 is zeroed, cb is set to its size,
        // and the handle is a valid process handle with a query right.
        let ex2 = unsafe {
            let mut counters: PROCESS_MEMORY_COUNTERS_EX2 = std::mem::zeroed();
            counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
            let ok = K32GetProcessMemoryInfo(
                handle,
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
        // the handle is a valid process handle with a query right.
        let base = unsafe {
            let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
            counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            let ok = K32GetProcessMemoryInfo(handle, &raw mut counters, counters.cb);
            (ok != 0).then_some(counters)
        }?;
        Some(MemoryCounters {
            working_set: base.WorkingSetSize as u64,
            peak_working_set: base.PeakWorkingSetSize as u64,
            // No EX2 data available: use the full working set as the fallback.
            private_working_set: base.WorkingSetSize as u64,
        })
    }

    /// Full path of the executable the process is running, or `None` if it
    /// cannot be queried. Paths longer than the first buffer (long-path
    /// builds) are handled by retrying with the largest path Windows allows.
    #[must_use]
    pub fn image_path(&self) -> Option<std::path::PathBuf> {
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, GetLastError};

        /// The longest path Windows supports, in UTF-16 units.
        const MAX_LONG_PATH: usize = 32_768;

        let mut buf = vec![0u16; 1024];
        loop {
            let mut len = buf.len() as u32;
            // SAFETY: `buf` is writable for `len` UTF-16 units and `len` is a
            // valid in/out pointer; the handle has a query right.
            let ok = unsafe {
                QueryFullProcessImageNameW(
                    self.0.as_raw_handle(),
                    PROCESS_NAME_WIN32,
                    buf.as_mut_ptr(),
                    &raw mut len,
                )
            };
            if ok != 0 {
                return Some(std::ffi::OsString::from_wide(&buf[..len as usize]).into());
            }
            // SAFETY: Reads the last-error value of the failed call above.
            let error = unsafe { GetLastError() };
            if error != ERROR_INSUFFICIENT_BUFFER || buf.len() >= MAX_LONG_PATH {
                return None;
            }
            buf = vec![0u16; MAX_LONG_PATH];
        }
    }

    /// When the process started, as a `FILETIME` tick count (100 ns units
    /// since 1601). Together with the PID it identifies a process uniquely,
    /// because Windows reuses PIDs.
    #[must_use]
    pub fn start_time(&self) -> Option<u64> {
        use windows_sys::Win32::Foundation::FILETIME;
        use windows_sys::Win32::System::Threading::GetProcessTimes;

        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut creation, mut exit, mut kernel, mut user) = (zero, zero, zero, zero);
        // SAFETY: The four FILETIME out-pointers are valid for writes and the
        // handle has the limited query right GetProcessTimes needs.
        let ok = unsafe {
            GetProcessTimes(
                self.0.as_raw_handle(),
                &raw mut creation,
                &raw mut exit,
                &raw mut kernel,
                &raw mut user,
            )
        };
        (ok != 0)
            .then(|| (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
    }

    /// Remove as many pages as possible from the working set. Needs a handle
    /// from [`Self::open_for_trim`].
    fn empty_working_set(&self) -> bool {
        // SAFETY: The handle is a valid process handle with PROCESS_SET_QUOTA
        // and a query right, which is what EmptyWorkingSet requires.
        unsafe { K32EmptyWorkingSet(self.0.as_raw_handle()) != 0 }
    }
}

/// Trim the working set of process `pid`, measuring how much it gave back.
///
/// When `started` is given (from [`ProcessHandle::start_time`] at the time
/// the process was listed), a process whose start time differs is treated
/// as [`TrimOutcome::Exited`]: its PID was reused by an unrelated process.
///
/// `EmptyWorkingSet` needs `PROCESS_SET_QUOTA` plus either query right; the
/// limited one is granted to far more processes (sandboxed browser children,
/// services), so more of them get trimmed.
#[must_use]
pub fn trim(pid: u32, started: Option<u64>) -> TrimOutcome {
    let handle = match ProcessHandle::open_for_trim(pid) {
        Ok(handle) => handle,
        Err(OpenError::Exited) => return TrimOutcome::Exited,
        Err(OpenError::Denied) => return TrimOutcome::Denied,
    };
    if started.is_some() && handle.start_time() != started {
        return TrimOutcome::Exited;
    }
    let before = handle.memory_counters().map(|c| c.working_set);
    if !handle.empty_working_set() {
        return TrimOutcome::Denied;
    }
    let after = handle.memory_counters().map(|c| c.working_set);
    let freed_bytes = match (before, after) {
        (Some(before), Some(after)) => before.saturating_sub(after),
        _ => 0,
    };
    TrimOutcome::Trimmed { freed_bytes }
}

/// Full path of the executable that process `pid` is running, or `None` if
/// the process cannot be opened or queried.
#[must_use]
pub fn image_path(pid: u32) -> Option<std::path::PathBuf> {
    ProcessHandle::open(pid).ok()?.image_path()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_handle_answers_every_query_for_this_process() {
        let handle = ProcessHandle::open(std::process::id()).expect("own process opens");
        let counters = handle.memory_counters().expect("counters");
        assert!(counters.working_set > 0);
        let path = handle.image_path().expect("image path");
        assert!(path.is_absolute());
        assert!(handle.start_time().is_some());
    }

    #[test]
    fn a_reused_pid_is_reported_as_exited() {
        let own = std::process::id();
        let started = ProcessHandle::open(own).unwrap().start_time().unwrap();
        assert_eq!(trim(own, Some(started + 1)), TrimOutcome::Exited);
    }

    #[test]
    fn a_missing_pid_is_reported_as_exited() {
        // PIDs are multiples of 4, so this one can never exist.
        assert_eq!(trim(0xFFFF_FFF1, None), TrimOutcome::Exited);
        assert!(matches!(
            ProcessHandle::open(0xFFFF_FFF1),
            Err(OpenError::Exited)
        ));
    }
}
