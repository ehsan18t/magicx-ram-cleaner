//! # `MagicX` RAM Cleaner — Memory Statistics
//!
//! Provides comprehensive memory usage reporting using Win32 and NT APIs.
//! Displays physical memory, commit charge, page file, kernel pools, and more.

use anyhow::{Result, bail};
use serde::Serialize;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::ProcessStatus::{
    K32GetPerformanceInfo, K32GetProcessMemoryInfo, PERFORMANCE_INFORMATION,
    PROCESS_MEMORY_COUNTERS,
};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
};

// ─── RAII Handle Guard ─────────────────────────────────────────────────────────

/// RAII wrapper for Win32 `HANDLE` values.
///
/// Automatically calls `CloseHandle` on drop, preventing handle leaks if a
/// panic occurs between the `Open*` / `CreateToolhelp32Snapshot` call and the
/// explicit `CloseHandle`. Null and `INVALID_HANDLE_VALUE` handles are not
/// closed (they are never valid).
pub struct HandleGuard {
    handle: HANDLE,
}

impl HandleGuard {
    /// Wrap a raw `HANDLE`. The caller must ensure the handle is valid
    /// and needs closing, or is null / `INVALID_HANDLE_VALUE`.
    pub const fn new(handle: HANDLE) -> Self {
        Self { handle }
    }

    /// Borrow the underlying handle for FFI calls.
    pub const fn raw(&self) -> HANDLE {
        self.handle
    }
}

impl Drop for HandleGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() && self.handle != INVALID_HANDLE_VALUE {
            // SAFETY: handle is a valid, open Win32 handle that must be closed.
            // CloseHandle is safe for any valid handle and idempotent for closed ones.
            unsafe { CloseHandle(self.handle) };
        }
    }
}

/// Snapshot of system memory state at a point in time.
#[derive(Debug, Clone, Serialize)]
pub struct MemorySnapshot {
    /// Percentage of physical memory in use (0–100).
    pub memory_load_percent: u32,
    /// Total physical RAM in bytes.
    pub total_physical: u64,
    /// Available physical RAM in bytes (free + zero + standby).
    pub available_physical: u64,
    /// Used physical RAM in bytes.
    pub used_physical: u64,
    /// Total page file size in bytes.
    pub total_page_file: u64,
    /// Available page file in bytes.
    pub available_page_file: u64,
    /// Total virtual address space in bytes.
    pub total_virtual: u64,
    /// Available virtual address space in bytes.
    pub available_virtual: u64,
    /// System commit total (pages).
    pub commit_total_pages: u64,
    /// System commit limit (pages).
    pub commit_limit_pages: u64,
    /// System commit peak (pages).
    pub commit_peak_pages: u64,
    /// Physical pages available.
    pub physical_available_pages: u64,
    /// Total physical pages.
    pub physical_total_pages: u64,
    /// Kernel paged pool (pages).
    pub kernel_paged_pages: u64,
    /// Kernel non-paged pool (pages).
    pub kernel_nonpaged_pages: u64,
    /// System page size in bytes.
    pub page_size: u64,
    /// Total open handles.
    pub handle_count: u32,
    /// Total processes.
    pub process_count: u32,
    /// Total threads.
    pub thread_count: u32,
}

impl MemorySnapshot {
    /// Capture current system memory state.
    pub fn capture() -> Result<Self> {
        // SAFETY: Both structs are zeroed and have their size fields set before
        // calling the Win32 functions. These are standard documented Win32 APIs.
        let (ms, pi) = unsafe {
            let mut ms: MEMORYSTATUSEX = std::mem::zeroed();
            ms.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&raw mut ms) == 0 {
                bail!("GlobalMemoryStatusEx failed");
            }

            let mut pi: PERFORMANCE_INFORMATION = std::mem::zeroed();
            pi.cb = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
            if K32GetPerformanceInfo(&raw mut pi, pi.cb) == 0 {
                bail!("GetPerformanceInfo failed");
            }
            (ms, pi)
        };

        Ok(Self {
            memory_load_percent: ms.dwMemoryLoad,
            total_physical: ms.ullTotalPhys,
            available_physical: ms.ullAvailPhys,
            used_physical: ms.ullTotalPhys.saturating_sub(ms.ullAvailPhys),
            total_page_file: ms.ullTotalPageFile,
            available_page_file: ms.ullAvailPageFile,
            total_virtual: ms.ullTotalVirtual,
            available_virtual: ms.ullAvailVirtual,
            commit_total_pages: pi.CommitTotal as u64,
            commit_limit_pages: pi.CommitLimit as u64,
            commit_peak_pages: pi.CommitPeak as u64,
            physical_available_pages: pi.PhysicalAvailable as u64,
            physical_total_pages: pi.PhysicalTotal as u64,
            kernel_paged_pages: pi.KernelPaged as u64,
            kernel_nonpaged_pages: pi.KernelNonpaged as u64,
            page_size: pi.PageSize as u64,
            handle_count: pi.HandleCount,
            process_count: pi.ProcessCount,
            thread_count: pi.ThreadCount,
        })
    }

    /// Get commit charge as a percentage.
    pub fn commit_percent(&self) -> f64 {
        if self.commit_limit_pages == 0 {
            return 0.0;
        }
        (self.commit_total_pages as f64 / self.commit_limit_pages as f64) * 100.0
    }
}

/// Lightweight memory reading for settle-detection polling.
///
/// Only calls `GlobalMemoryStatusEx` (skips `K32GetPerformanceInfo`) to avoid
/// unnecessary work when we only need physical memory metrics for convergence
/// checks.
#[derive(Debug, Clone, Copy)]
pub struct QuickMemoryReading {
    /// Total physical RAM in bytes.
    pub total_physical: u64,
    /// Available physical RAM in bytes.
    pub available_physical: u64,
}

impl QuickMemoryReading {
    /// Capture physical memory metrics (single Win32 call).
    pub fn capture() -> Result<Self> {
        // SAFETY: MEMORYSTATUSEX is zeroed and has dwLength set before calling
        // GlobalMemoryStatusEx. This is a standard documented Win32 API.
        let ms = unsafe {
            let mut ms: MEMORYSTATUSEX = std::mem::zeroed();
            ms.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&raw mut ms) == 0 {
                bail!("GlobalMemoryStatusEx failed");
            }
            ms
        };
        Ok(Self {
            total_physical: ms.ullTotalPhys,
            available_physical: ms.ullAvailPhys,
        })
    }
}

/// Extract a UTF-8 process name from a null-terminated UTF-16 `szExeFile` buffer.
pub fn extract_exe_name(sz_exe_file: &[u16]) -> String {
    let len = sz_exe_file
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(sz_exe_file.len());
    String::from_utf16_lossy(&sz_exe_file[..len])
}

/// Format bytes into a human-readable string (e.g., "3.42 GB").
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Detailed memory list information from the kernel (undocumented API).
/// This gives exact page counts for each memory list (Zeroed, Free, Modified,
/// `ModifiedNoWrite`, Bad, Standby priorities 0-7, Repurposed priorities 0-7).
#[derive(Debug, Clone, Serialize)]
// Every field genuinely represents a page count — the `_pages` suffix is intentional.
#[allow(clippy::struct_field_names)]
pub struct MemoryListInfo {
    pub zeroed_pages: u64,
    pub free_pages: u64,
    pub modified_pages: u64,
    pub modified_no_write_pages: u64,
    pub bad_pages: u64,
    pub standby_pages: [u64; 8],
    pub repurposed_pages: [u64; 8],
    /// Modified pages destined for the pagefile (subset of `modified_pages`).
    pub modified_pagefile_pages: u64,
}

impl MemoryListInfo {
    /// Query the kernel for detailed memory list information.
    ///
    /// The struct layout varies by Windows version:
    /// - Base: 5 + 8 + 8 + 1 = 22 `ULONG_PTR` entries
    /// - Newer builds add `StandbyRepurposedByPriority`\[8\], making it 30 entries
    ///
    /// Uses a fixed-size stack buffer (30 entries = 240 bytes on x86-64) which
    /// covers all known Windows versions. Falls back to a heap-allocated buffer
    /// only if the kernel reports a larger size than expected.
    pub fn query() -> Result<Self> {
        use crate::ntapi::{STATUS_INFO_LENGTH_MISMATCH, SYSTEM_MEMORY_LIST_INFORMATION};

        // Stack buffer covers the largest known layout (30 × 8 = 240 bytes)
        let mut stack_buf = [0usize; 30];
        let mut return_length: u32 = 0;

        let mut status = crate::ntapi::nt_query_system_information(
            SYSTEM_MEMORY_LIST_INFORMATION,
            stack_buf.as_mut_ptr().cast(),
            std::mem::size_of_val(&stack_buf) as u32,
            &raw mut return_length,
        );

        // If the kernel needs more than 30 entries (unlikely but defensive),
        // fall back to a heap allocation with the exact size requested.
        let mut heap_buf: Option<Vec<usize>> = None;
        if status == STATUS_INFO_LENGTH_MISMATCH && return_length > 0 {
            let needed_entries = (return_length as usize).div_ceil(std::mem::size_of::<usize>());
            let buf = vec![0usize; needed_entries];
            heap_buf = Some(buf);
            status = crate::ntapi::nt_query_system_information(
                SYSTEM_MEMORY_LIST_INFORMATION,
                heap_buf
                    .as_mut()
                    .expect("just assigned")
                    .as_mut_ptr()
                    .cast(),
                return_length,
                &raw mut return_length,
            );
        }

        // Use the heap buffer if it was allocated, otherwise the stack buffer
        let buf: &[usize] = heap_buf.as_deref().unwrap_or(&stack_buf);

        if status != 0 {
            bail!(
                "NtQuerySystemInformation(SystemMemoryListInformation) failed: NTSTATUS 0x{status:08X}"
            );
        }

        // We need at least 22 entries to parse the base fields.
        // Guard against return_length = 0 on an otherwise-successful call
        // (defensive — shouldn't happen but prevents reading stale zeros).
        let entry_size = std::mem::size_of::<usize>();
        let count = if return_length > 0 {
            return_length as usize / entry_size
        } else {
            bail!(
                "NtQuerySystemInformation(SystemMemoryListInformation) succeeded but \
                 returned 0 bytes — cannot parse memory list data"
            );
        };
        if count < 22 {
            bail!(
                "NtQuerySystemInformation returned only {} bytes, need at least {} for base fields",
                return_length,
                22 * entry_size
            );
        }

        // Ensure we only read within the bounds reported by the kernel
        let buf = &buf[..count];

        let mut standby = [0u64; 8];
        let mut repurposed = [0u64; 8];
        for i in 0..8 {
            standby[i] = buf[5 + i] as u64;
            repurposed[i] = buf[13 + i] as u64;
        }

        Ok(Self {
            zeroed_pages: buf[0] as u64,
            free_pages: buf[1] as u64,
            modified_pages: buf[2] as u64,
            modified_no_write_pages: buf[3] as u64,
            bad_pages: buf[4] as u64,
            standby_pages: standby,
            repurposed_pages: repurposed,
            modified_pagefile_pages: buf[21] as u64,
        })
    }

    /// Total standby pages across all priority levels.
    pub fn total_standby_pages(&self) -> u64 {
        self.standby_pages.iter().sum()
    }
}

// ─── File Cache Information ──────────────────────────────────────────────────

/// Snapshot of the system file cache working set.
///
/// Queried via `NtQuerySystemInformation(SystemFileCacheInformation)`.
/// Shows how much RAM the file cache is currently consuming and its limits.
#[derive(Debug, Clone, Serialize)]
pub struct FileCacheSnapshot {
    /// Current file cache working set size (bytes).
    pub current_size: u64,
    /// Peak file cache working set size since boot (bytes).
    pub peak_size: u64,
    /// Minimum configured working set (bytes, 0 = system default).
    pub minimum_working_set: u64,
    /// Maximum configured working set (bytes, 0 = system default).
    pub maximum_working_set: u64,
}

impl FileCacheSnapshot {
    /// Query the kernel for current file cache statistics.
    pub fn capture() -> Result<Self> {
        use crate::ntapi::{SYSTEM_FILE_CACHE_INFORMATION, SystemFileCacheInfo};

        let mut info: SystemFileCacheInfo = unsafe { std::mem::zeroed() };
        let mut return_length: u32 = 0;

        let status = crate::ntapi::nt_query_system_information(
            SYSTEM_FILE_CACHE_INFORMATION,
            (&raw mut info).cast(),
            std::mem::size_of::<SystemFileCacheInfo>() as u32,
            &raw mut return_length,
        );

        if status != 0 {
            bail!(
                "NtQuerySystemInformation(SystemFileCacheInformation) failed: NTSTATUS 0x{status:08X}"
            );
        }

        Ok(Self {
            current_size: info.current_size as u64,
            peak_size: info.peak_size as u64,
            minimum_working_set: info.minimum_working_set as u64,
            maximum_working_set: info.maximum_working_set as u64,
        })
    }
}

// ─── Per-Process Memory Usage ────────────────────────────────────────────────

/// Memory usage information for a single process.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessMemoryInfo {
    /// Process ID.
    pub pid: u32,
    /// Executable name (e.g. `chrome.exe`).
    pub name: String,
    /// Current working set size in bytes (physical RAM used).
    pub working_set: u64,
    /// Peak working set size in bytes.
    pub peak_working_set: u64,
}

/// Enumerate running processes and return the top `count` by working set size.
///
/// Uses `Toolhelp32` for process enumeration (same approach as `cleaner.rs`)
/// and `K32GetProcessMemoryInfo` for per-process memory counters.
/// Processes that cannot be opened (system/protected) are silently skipped.
pub fn query_top_processes(count: usize) -> Result<Vec<ProcessMemoryInfo>> {
    // SAFETY: CreateToolhelp32Snapshot with TH32CS_SNAPPROCESS and 0 is the
    // standard documented way to enumerate all running processes.
    let snap_raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap_raw == INVALID_HANDLE_VALUE {
        bail!("CreateToolhelp32Snapshot failed");
    }
    let snapshot = HandleGuard::new(snap_raw);

    let mut processes = Vec::new();
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

    // SAFETY: Process32FirstW/Process32NextW iterate the Toolhelp snapshot.
    // The entry struct is properly zeroed and sized.
    let mut has_entry = unsafe { Process32FirstW(snapshot.raw(), &raw mut entry) } != 0;

    while has_entry {
        let pid = entry.th32ProcessID;

        // Skip System Idle (PID 0) and System (PID 4)
        if pid != 0
            && pid != 4
            && let Some(info) = query_single_process(pid, &entry.szExeFile)
        {
            processes.push(info);
        }

        has_entry = unsafe { Process32NextW(snapshot.raw(), &raw mut entry) } != 0;
    }

    // snapshot guard dropped here — CloseHandle called automatically

    // Sort descending by working set size and truncate to requested count
    processes.sort_unstable_by(|a, b| b.working_set.cmp(&a.working_set));
    processes.truncate(count);

    Ok(processes)
}

/// Query memory info for a single process. Returns `None` if the process
/// cannot be opened (protected/system processes).
fn query_single_process(pid: u32, exe_name: &[u16]) -> Option<ProcessMemoryInfo> {
    // SAFETY: OpenProcess with PROCESS_QUERY_INFORMATION | PROCESS_VM_READ is the
    // documented way to open a process for memory info queries.
    let proc_handle = HandleGuard::new(unsafe {
        OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid)
    });
    if proc_handle.raw().is_null() {
        return None;
    }

    // SAFETY: PROCESS_MEMORY_COUNTERS is zeroed, cb is set to struct size,
    // and handle is a valid process handle from OpenProcess.
    let counters = unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let ok = K32GetProcessMemoryInfo(
            proc_handle.raw(),
            &raw mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        );
        // proc_handle dropped here — CloseHandle called automatically
        if ok == 0 {
            return None;
        }
        counters
    };

    // Extract process name from the wide-char szExeFile buffer
    let name = extract_exe_name(exe_name);

    Some(ProcessMemoryInfo {
        pid,
        name,
        working_set: counters.WorkingSetSize as u64,
        peak_working_set: counters.PeakWorkingSetSize as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_exe_name_from_utf16() {
        // Simulate a null-terminated UTF-16 "chrome.exe"
        let name: Vec<u16> = "chrome.exe\0\0\0\0".encode_utf16().collect();
        assert_eq!(extract_exe_name(&name), "chrome.exe");
    }

    #[test]
    fn format_bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn format_bytes_bytes_range() {
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn format_bytes_kilobytes() {
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
    }

    #[test]
    fn format_bytes_megabytes() {
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1_572_864), "1.50 MB"); // 1.5 MB
    }

    #[test]
    fn format_bytes_gigabytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_bytes(17_179_869_184), "16.00 GB");
    }

    #[test]
    fn format_bytes_terabytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024), "1.00 TB");
    }

    #[test]
    fn extract_exe_name_no_null() {
        // If the buffer has no null terminator, use the full slice
        let name: Vec<u16> = "svchost.exe".encode_utf16().collect();
        assert_eq!(extract_exe_name(&name), "svchost.exe");
    }

    #[test]
    fn extract_exe_name_empty() {
        let name: Vec<u16> = vec![0];
        assert_eq!(extract_exe_name(&name), "");
    }

    #[test]
    fn commit_percent_normal() {
        let snap = MemorySnapshot {
            memory_load_percent: 50,
            total_physical: 16 * 1024 * 1024 * 1024,
            available_physical: 8 * 1024 * 1024 * 1024,
            used_physical: 8 * 1024 * 1024 * 1024,
            total_page_file: 0,
            available_page_file: 0,
            total_virtual: 0,
            available_virtual: 0,
            commit_total_pages: 500_000,
            commit_limit_pages: 1_000_000,
            commit_peak_pages: 0,
            physical_available_pages: 0,
            physical_total_pages: 0,
            kernel_paged_pages: 0,
            kernel_nonpaged_pages: 0,
            page_size: 4096,
            handle_count: 0,
            process_count: 0,
            thread_count: 0,
        };
        let pct = snap.commit_percent();
        assert!(
            (pct - 50.0).abs() < 0.01,
            "commit_percent should be 50.0, got {pct}"
        );
    }

    #[test]
    fn commit_percent_zero_limit() {
        let snap = MemorySnapshot {
            memory_load_percent: 0,
            total_physical: 0,
            available_physical: 0,
            used_physical: 0,
            total_page_file: 0,
            available_page_file: 0,
            total_virtual: 0,
            available_virtual: 0,
            commit_total_pages: 100,
            commit_limit_pages: 0, // zero limit — edge case
            commit_peak_pages: 0,
            physical_available_pages: 0,
            physical_total_pages: 0,
            kernel_paged_pages: 0,
            kernel_nonpaged_pages: 0,
            page_size: 4096,
            handle_count: 0,
            process_count: 0,
            thread_count: 0,
        };
        assert!(
            snap.commit_percent().abs() < f64::EPSILON,
            "commit_percent should be 0.0 when limit is 0"
        );
    }

    #[test]
    fn memory_list_info_total_standby_pages() {
        let info = MemoryListInfo {
            zeroed_pages: 0,
            free_pages: 0,
            modified_pages: 0,
            modified_no_write_pages: 0,
            bad_pages: 0,
            standby_pages: [100, 200, 300, 400, 500, 600, 700, 800],
            repurposed_pages: [0; 8],
            modified_pagefile_pages: 0,
        };
        assert_eq!(
            info.total_standby_pages(),
            3600,
            "sum of 100..800 should be 3600"
        );
    }

    #[test]
    fn memory_list_info_total_standby_all_zero() {
        let info = MemoryListInfo {
            zeroed_pages: 0,
            free_pages: 0,
            modified_pages: 0,
            modified_no_write_pages: 0,
            bad_pages: 0,
            standby_pages: [0; 8],
            repurposed_pages: [0; 8],
            modified_pagefile_pages: 0,
        };
        assert_eq!(info.total_standby_pages(), 0);
    }

    #[test]
    fn format_bytes_boundary_values() {
        // Exactly at boundary between ranges
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024), "1.00 TB");
    }
}
