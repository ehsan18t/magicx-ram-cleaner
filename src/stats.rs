//! # MagicX RAM Cleaner — Memory Statistics
//!
//! Provides comprehensive memory usage reporting using Win32 and NT APIs.
//! Displays physical memory, commit charge, page file, kernel pools, and more.

use anyhow::{Result, bail};
use serde::Serialize;
use windows_sys::Win32::System::ProcessStatus::{K32GetPerformanceInfo, PERFORMANCE_INFORMATION};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

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
        let (ms, pi) = unsafe {
            let mut ms: MEMORYSTATUSEX = std::mem::zeroed();
            ms.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut ms) == 0 {
                bail!("GlobalMemoryStatusEx failed");
            }

            let mut pi: PERFORMANCE_INFORMATION = std::mem::zeroed();
            pi.cb = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
            if K32GetPerformanceInfo(&mut pi, pi.cb) == 0 {
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

    /// Calculate estimated standby list size in bytes.
    /// This is an approximation — for exact values use `MemoryListInfo::query()`.
    #[allow(dead_code)]
    pub fn estimated_standby(&self) -> u64 {
        // Available includes standby + free + zero pages.
        // Free/zero pages are typically a small fraction of available.
        // A rough heuristic: standby ≈ available - (physical_available_pages count
        // can include zeroed pages which are a small %). Without the undocumented
        // query, we can't separate free from standby, so return available as
        // the upper bound.
        self.available_physical
    }

    /// Get commit charge as a percentage.
    pub fn commit_percent(&self) -> f64 {
        if self.commit_limit_pages == 0 {
            return 0.0;
        }
        (self.commit_total_pages as f64 / self.commit_limit_pages as f64) * 100.0
    }
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
        format!("{} B", bytes)
    }
}

/// Detailed memory list information from the kernel (undocumented API).
/// This gives exact page counts for each memory list (Zeroed, Free, Modified,
/// ModifiedNoWrite, Bad, Standby priorities 0-7, Repurposed priorities 0-7).
#[derive(Debug, Clone, Serialize)]
pub struct MemoryListInfo {
    pub zeroed_pages: u64,
    pub free_pages: u64,
    pub modified_pages: u64,
    pub modified_no_write_pages: u64,
    pub bad_pages: u64,
    pub standby_pages: [u64; 8],
    pub repurposed_pages: [u64; 8],
    pub modified_pages_total: u64,
}

impl MemoryListInfo {
    /// Query the kernel for detailed memory list information.
    pub fn query() -> Result<Self> {
        // SYSTEM_MEMORY_LIST_INFORMATION is information class 0x50 (80)
        // The struct contains: ULONG_PTR[5] for zeroed/free/modified/modifiednowrite/bad
        //                      ULONG_PTR[8] for standby priorities 0-7
        //                      ULONG_PTR[8] for repurposed priorities 0-7
        //                      ULONG_PTR for modified page count total
        // Total: 5 + 8 + 8 + 1 = 22 ULONG_PTR entries
        let mut info = [0usize; 22];
        let status = super::ntapi::nt_query_system_information(
            80, // SystemMemoryListInformation
            info.as_mut_ptr() as *mut _,
            std::mem::size_of_val(&info) as u32,
            std::ptr::null_mut(),
        );
        if status != 0 {
            bail!(
                "NtQuerySystemInformation(SystemMemoryListInformation) failed: NTSTATUS 0x{:08X}",
                status
            );
        }

        let mut standby = [0u64; 8];
        let mut repurposed = [0u64; 8];
        for i in 0..8 {
            standby[i] = info[5 + i] as u64;
            repurposed[i] = info[13 + i] as u64;
        }

        Ok(Self {
            zeroed_pages: info[0] as u64,
            free_pages: info[1] as u64,
            modified_pages: info[2] as u64,
            modified_no_write_pages: info[3] as u64,
            bad_pages: info[4] as u64,
            standby_pages: standby,
            repurposed_pages: repurposed,
            modified_pages_total: info[21] as u64,
        })
    }

    /// Total standby pages across all priority levels.
    pub fn total_standby_pages(&self) -> u64 {
        self.standby_pages.iter().sum()
    }

    /// Low-priority standby pages (priority 0 only).
    #[allow(dead_code)]
    pub fn low_priority_standby_pages(&self) -> u64 {
        self.standby_pages[0]
    }
}
