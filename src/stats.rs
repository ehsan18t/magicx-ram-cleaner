//! # `MagicX` RAM Cleaner - Memory Statistics
//!
//! Domain types for memory usage reporting: system snapshots, per-process
//! usage and byte formatting. All operating-system access goes through
//! [`crate::platform`].

use anyhow::Result;
use serde::Serialize;

pub use crate::platform::memory::{FileCacheSnapshot, MemoryListInfo};
use crate::platform::{memory, process};

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
    /// Kernel page-list breakdown (free, standby, modified) at the same instant.
    ///
    /// `None` when `NtQuerySystemInformation(SystemMemoryListInformation)` is
    /// unavailable, which happens when `SeProfileSingleProcessPrivilege` is not
    /// enabled (e.g. a non-elevated `status` call).
    pub lists: Option<MemoryListInfo>,
}

impl MemorySnapshot {
    /// Capture current system memory state.
    pub fn capture() -> Result<Self> {
        let ms = memory::memory_status()?;
        let pi = memory::performance_info()?;
        Ok(Self {
            memory_load_percent: ms.load_percent,
            total_physical: ms.total_physical,
            available_physical: ms.available_physical,
            used_physical: ms.total_physical.saturating_sub(ms.available_physical),
            total_page_file: ms.total_page_file,
            available_page_file: ms.available_page_file,
            total_virtual: ms.total_virtual,
            available_virtual: ms.available_virtual,
            commit_total_pages: pi.commit_total_pages,
            commit_limit_pages: pi.commit_limit_pages,
            commit_peak_pages: pi.commit_peak_pages,
            physical_available_pages: pi.physical_available_pages,
            physical_total_pages: pi.physical_total_pages,
            kernel_paged_pages: pi.kernel_paged_pages,
            kernel_nonpaged_pages: pi.kernel_nonpaged_pages,
            page_size: pi.page_size,
            handle_count: pi.handle_count,
            process_count: pi.process_count,
            thread_count: pi.thread_count,
            lists: MemoryListInfo::query().ok(),
        })
    }

    /// Truly unused RAM in bytes (zeroed + free page lists), if known.
    ///
    /// Unlike [`available_physical`](Self::available_physical), this excludes
    /// the standby cache, so it is the figure that rises when standby pages
    /// are purged.
    #[must_use]
    pub fn free_bytes(&self) -> Option<u64> {
        self.lists
            .as_ref()
            .map(|l| l.free_and_zeroed_pages().saturating_mul(self.page_size))
    }

    /// Standby cache size in bytes (all priorities), if known.
    #[must_use]
    pub fn standby_bytes(&self) -> Option<u64> {
        self.lists
            .as_ref()
            .map(|l| l.total_standby_pages().saturating_mul(self.page_size))
    }

    /// Modified (dirty, awaiting write-back) page list size in bytes, if known.
    #[must_use]
    pub fn modified_bytes(&self) -> Option<u64> {
        self.lists
            .as_ref()
            .map(|l| l.modified_pages.saturating_mul(self.page_size))
    }

    /// Get commit charge as a percentage.
    #[must_use]
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
        let ms = memory::memory_status()?;
        Ok(Self {
            total_physical: ms.total_physical,
            available_physical: ms.available_physical,
        })
    }
}

/// Format bytes into a human-readable string (e.g., "3.42 GB").
#[must_use]
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

/// Format a signed byte delta with an explicit sign (e.g. "+1.50 GB", "-12.00 MB").
///
/// Zero is rendered without a sign ("0 B").
#[must_use]
pub fn format_signed_bytes(bytes: i64) -> String {
    match bytes.cmp(&0) {
        std::cmp::Ordering::Greater => format!("+{}", format_bytes(bytes.unsigned_abs())),
        std::cmp::Ordering::Less => format!("-{}", format_bytes(bytes.unsigned_abs())),
        std::cmp::Ordering::Equal => format_bytes(0),
    }
}

// ─── File Cache Information ──────────────────────────────────────────────────

// ─── Per-Process Memory Usage ────────────────────────────────────────────────

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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn format_signed_bytes_sign_handling() {
        assert_eq!(format_signed_bytes(0), "0 B");
        assert_eq!(format_signed_bytes(1536), "+1.50 KB");
        assert_eq!(format_signed_bytes(-1024 * 1024), "-1.00 MB");
        assert_eq!(format_signed_bytes(i64::MIN).chars().next(), Some('-'));
    }

    #[test]
    fn format_bytes_terabytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024), "1.00 TB");
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
            lists: None,
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
            commit_limit_pages: 0, // zero limit - edge case
            commit_peak_pages: 0,
            physical_available_pages: 0,
            physical_total_pages: 0,
            kernel_paged_pages: 0,
            kernel_nonpaged_pages: 0,
            page_size: 4096,
            handle_count: 0,
            process_count: 0,
            thread_count: 0,
            lists: None,
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
}
