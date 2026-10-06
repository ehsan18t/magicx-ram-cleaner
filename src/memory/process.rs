//! Per-process memory usage.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::platform::{paths, process};

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
    /// Whether this is a Windows process (see [`is_windows_process`]).
    pub windows_process: bool,
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
    let windows_dir = paths::windows_directory().ok();
    let mut processes: Vec<ProcessMemoryInfo> = process::processes()?
        .into_iter()
        .filter_map(|entry| {
            let counters = process::memory_counters(entry.pid)?;
            let image = process::image_path(entry.pid);
            Some(ProcessMemoryInfo {
                pid: entry.pid,
                name: entry.name,
                working_set: counters.working_set,
                peak_working_set: counters.peak_working_set,
                private_working_set: counters.private_working_set,
                windows_process: is_windows_process(image.as_deref(), windows_dir.as_deref()),
            })
        })
        .collect();
    processes.sort_unstable_by_key(|p| std::cmp::Reverse(p.working_set));
    Ok(processes)
}

/// Whether a process is part of Windows: its program file is inside the
/// Windows folder (`windows_dir`), or it has no readable program file at all.
///
/// The second case covers the core processes Windows protects or runs
/// without an executable: System, Registry, Memory Compression, and the
/// virtual machine memory of WSL and Hyper-V (`vmmem`).
#[must_use]
pub fn is_windows_process(image: Option<&Path>, windows_dir: Option<&Path>) -> bool {
    let Some(image) = image else {
        return true;
    };
    let Some(windows_dir) = windows_dir else {
        return false;
    };
    let image = image.to_string_lossy().to_lowercase();
    let mut dir = windows_dir.to_string_lossy().to_lowercase();
    if !dir.ends_with('\\') {
        dir.push('\\');
    }
    image.starts_with(&dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Windows folder the tests classify against.
    fn windows_dir() -> &'static Path {
        Path::new(r"C:\Windows")
    }

    #[test]
    fn programs_in_the_windows_folder_are_windows_processes() {
        for image in [
            r"C:\Windows\System32\svchost.exe",
            r"C:\Windows\explorer.exe",
            r"c:\windows\SystemApps\ShellExperienceHost.exe",
        ] {
            assert!(
                is_windows_process(Some(Path::new(image)), Some(windows_dir())),
                "{image}"
            );
        }
    }

    #[test]
    fn programs_elsewhere_are_not() {
        for image in [
            r"C:\Program Files\Mozilla Firefox\firefox.exe",
            r"C:\Users\me\AppData\Local\Temp\svchost.exe",
            r"C:\Windows2\tool.exe",
            r"D:\Windows\app.exe",
        ] {
            assert!(
                !is_windows_process(Some(Path::new(image)), Some(windows_dir())),
                "{image}"
            );
        }
    }

    #[test]
    fn processes_without_a_readable_program_file_are_windows_processes() {
        assert!(is_windows_process(None, Some(windows_dir())));
        assert!(is_windows_process(None, None));
    }

    #[test]
    fn an_unknown_windows_folder_marks_only_pathless_processes() {
        assert!(!is_windows_process(
            Some(Path::new(r"C:\Windows\explorer.exe")),
            None
        ));
    }
}
