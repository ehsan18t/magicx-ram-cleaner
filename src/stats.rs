//! # `MagicX` RAM Cleaner — Memory Statistics
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
    /// This implementation starts with a 30-entry buffer and falls back to
    /// dynamic sizing via `return_length` if the kernel reports a mismatch.
    pub fn query() -> Result<Self> {
        use crate::ntapi::{STATUS_INFO_LENGTH_MISMATCH, SYSTEM_MEMORY_LIST_INFORMATION};

        // Start with the larger known layout (30 entries covers newer Windows)
        let mut buf: Vec<usize> = vec![0usize; 30];
        let mut return_length: u32 = 0;

        let mut status = crate::ntapi::nt_query_system_information(
            SYSTEM_MEMORY_LIST_INFORMATION,
            buf.as_mut_ptr().cast(),
            (buf.len() * std::mem::size_of::<usize>()) as u32,
            &raw mut return_length,
        );

        // If buffer is too small, retry with the size the kernel told us
        if status == STATUS_INFO_LENGTH_MISMATCH && return_length > 0 {
            let needed_entries = (return_length as usize).div_ceil(std::mem::size_of::<usize>());
            buf.resize(needed_entries, 0);
            status = crate::ntapi::nt_query_system_information(
                SYSTEM_MEMORY_LIST_INFORMATION,
                buf.as_mut_ptr().cast(),
                return_length,
                &raw mut return_length,
            );
        }

        if status != 0 {
            bail!(
                "NtQuerySystemInformation(SystemMemoryListInformation) failed: NTSTATUS 0x{status:08X}"
            );
        }

        // We need at least 22 entries to parse the base fields
        let count = return_length as usize / std::mem::size_of::<usize>();
        let count = if count > 0 { count } else { buf.len() };
        if count < 22 {
            bail!(
                "NtQuerySystemInformation returned only {} bytes, need at least {} for base fields",
                return_length,
                22 * std::mem::size_of::<usize>()
            );
        }

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
    fn format_bytes_terabytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024), "1.00 TB");
    }
}
