//! Per-process memory usage.

use anyhow::Result;
use serde::Serialize;

use crate::platform::process;

/// Memory usage information for a single process.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessMemoryInfo {
    /// Process ID.
    pub pid: u32,
    /// Executable name (e.g. `chrome.exe`).
    pub name: String,
    /// Current working set size in bytes (physical RAM used, shared + private).
    pub working_set: u64,
    /// Peak working set size in bytes.
    pub peak_working_set: u64,
    /// Private working set size in bytes - the portion of the working set
    /// that is not shared with other processes.
    ///
    /// This matches the "Memory" column shown in Windows Task Manager.
    /// Obtained from `PROCESS_MEMORY_COUNTERS_EX2::PrivateWorkingSetSize`
    /// (Windows 10 1709+). Falls back to the full `working_set` on older
    /// builds where the extended struct is not supported.
    pub private_working_set: u64,
}

/// Enumerate running processes and return the top `count` by working set size.
///
/// Processes that cannot be opened (system/protected) are silently skipped.
pub fn query_top_processes(count: usize) -> Result<Vec<ProcessMemoryInfo>> {
    let mut processes = query_all_processes()?;
    processes.truncate(count);
    Ok(processes)
}

/// Enumerate all running processes sorted by working set size (descending).
///
/// Unlike [`query_top_processes`], this function returns every process that can
/// be queried without any limit.  Use this when caller-side aggregation (e.g.
/// grouping by executable name) must see all instances before deciding what to
/// keep. Processes that cannot be opened (system/protected) are skipped.
pub fn query_all_processes() -> Result<Vec<ProcessMemoryInfo>> {
    let mut processes: Vec<ProcessMemoryInfo> = process::processes()?
        .into_iter()
        .filter_map(|entry| {
            let counters = process::memory_counters(entry.pid)?;
            Some(ProcessMemoryInfo {
                pid: entry.pid,
                name: entry.name,
                working_set: counters.working_set,
                peak_working_set: counters.peak_working_set,
                private_working_set: counters.private_working_set,
            })
        })
        .collect();
    processes.sort_unstable_by_key(|p| std::cmp::Reverse(p.working_set));
    Ok(processes)
}
