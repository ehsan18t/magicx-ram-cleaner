//! System-wide memory queries and the system file cache control.

use anyhow::{Result, bail};
use serde::Serialize;
use windows_sys::Win32::System::Memory::SetSystemFileCacheSize;
use windows_sys::Win32::System::ProcessStatus::{K32GetPerformanceInfo, PERFORMANCE_INFORMATION};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

/// Physical, page-file and virtual memory status (`GlobalMemoryStatusEx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryStatus {
    /// Percentage of physical memory in use (0-100).
    pub load_percent: u32,
    /// Total physical RAM in bytes.
    pub total_physical: u64,
    /// Available physical RAM in bytes (free + zeroed + standby).
    pub available_physical: u64,
    /// Commit limit in bytes (RAM + page files). Win32 calls this "page file".
    pub total_page_file: u64,
    /// Commit still available in bytes.
    pub available_page_file: u64,
    /// Total user-mode virtual address space in bytes.
    pub total_virtual: u64,
    /// Unreserved user-mode virtual address space in bytes.
    pub available_virtual: u64,
}

/// Query physical, commit and virtual memory status.
pub fn memory_status() -> Result<MemoryStatus> {
    // SAFETY: MEMORYSTATUSEX is plain data; it is zeroed and dwLength is set
    // before the call, as the API requires.
    let ms = unsafe {
        let mut ms: MEMORYSTATUSEX = std::mem::zeroed();
        ms.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&raw mut ms) == 0 {
            bail!("GlobalMemoryStatusEx failed");
        }
        ms
    };
    Ok(MemoryStatus {
        load_percent: ms.dwMemoryLoad,
        total_physical: ms.ullTotalPhys,
        available_physical: ms.ullAvailPhys,
        total_page_file: ms.ullTotalPageFile,
        available_page_file: ms.ullAvailPageFile,
        total_virtual: ms.ullTotalVirtual,
        available_virtual: ms.ullAvailVirtual,
    })
}

/// System performance counters (`GetPerformanceInfo`). Memory figures are in
/// pages of `page_size` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_field_names)] // `*_count` / `*_pages` suffixes carry the unit
pub struct PerformanceInfo {
    /// System commit charge (pages).
    pub commit_total_pages: u64,
    /// System commit limit (pages).
    pub commit_limit_pages: u64,
    /// Peak commit charge since boot (pages).
    pub commit_peak_pages: u64,
    /// Total physical memory (pages).
    pub physical_total_pages: u64,
    /// Available physical memory (pages).
    pub physical_available_pages: u64,
    /// Kernel paged pool (pages).
    pub kernel_paged_pages: u64,
    /// Kernel non-paged pool (pages).
    pub kernel_nonpaged_pages: u64,
    /// Page size in bytes.
    pub page_size: u64,
    /// Open handles system-wide.
    pub handle_count: u32,
    /// Running processes.
    pub process_count: u32,
    /// Running threads.
    pub thread_count: u32,
}

/// Query system performance counters.
pub fn performance_info() -> Result<PerformanceInfo> {
    // SAFETY: PERFORMANCE_INFORMATION is plain data; it is zeroed and cb is
    // set before the call, as the API requires.
    let pi = unsafe {
        let mut pi: PERFORMANCE_INFORMATION = std::mem::zeroed();
        pi.cb = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
        if K32GetPerformanceInfo(&raw mut pi, pi.cb) == 0 {
            bail!("GetPerformanceInfo failed");
        }
        pi
    };
    Ok(PerformanceInfo {
        commit_total_pages: pi.CommitTotal as u64,
        commit_limit_pages: pi.CommitLimit as u64,
        commit_peak_pages: pi.CommitPeak as u64,
        physical_total_pages: pi.PhysicalTotal as u64,
        physical_available_pages: pi.PhysicalAvailable as u64,
        kernel_paged_pages: pi.KernelPaged as u64,
        kernel_nonpaged_pages: pi.KernelNonpaged as u64,
        page_size: pi.PageSize as u64,
        handle_count: pi.HandleCount,
        process_count: pi.ProcessCount,
        thread_count: pi.ThreadCount,
    })
}

/// Trim the system file cache working set.
///
/// `(SIZE_T)-1` for both limits is the documented one-shot "flush the cache"
/// request: it trims the cache without changing the configured limits, so
/// there is nothing to restore afterwards. (Calling
/// `SetSystemFileCacheSize(0, 0, 0)` to "restore" would instead overwrite any
/// limits the administrator configured.) Requires `SeIncreaseQuotaPrivilege`.
///
/// # Errors
///
/// Returns the Win32 error code on failure.
pub fn flush_system_file_cache() -> Result<(), u32> {
    // SAFETY: Plain Win32 call with value arguments.
    if unsafe { SetSystemFileCacheSize(usize::MAX, usize::MAX, 0) } == 0 {
        // SAFETY: Reads the calling thread's last-error value.
        return Err(unsafe { windows_sys::Win32::Foundation::GetLastError() });
    }
    Ok(())
}

/// Detailed memory list information from the kernel (undocumented API).
///
/// This gives exact page counts for each memory list (Zeroed, Free, Modified,
/// `ModifiedNoWrite`, Bad, Standby priorities 0-7, Repurposed priorities 0-7).
#[derive(Debug, Clone, Serialize)]
// Every field genuinely represents a page count - the `_pages` suffix is intentional.
#[allow(clippy::struct_field_names)]
pub struct MemoryListInfo {
    /// Pages on the zeroed-page list (already zero-filled, ready for allocation).
    pub zeroed_pages: u64,
    /// Pages on the free-page list (available but not yet zeroed).
    pub free_pages: u64,
    /// Pages on the modified-page list (dirty, awaiting writeback).
    pub modified_pages: u64,
    /// Modified pages that will not be written to the pagefile.
    pub modified_no_write_pages: u64,
    /// Pages flagged as physically defective.
    pub bad_pages: u64,
    /// Standby pages by priority (index 0 = lowest, 7 = highest).
    pub standby_pages: [u64; 8],
    /// Repurposed standby pages by priority.
    pub repurposed_pages: [u64; 8],
    /// Modified pages destined for the pagefile (subset of `modified_pages`).
    pub modified_pagefile_pages: u64,
}

impl MemoryListInfo {
    /// Query the kernel for detailed memory list information.
    ///
    /// Maps `SYSTEM_MEMORY_LIST_INFORMATION`: 22 `ULONG_PTR` entries
    /// (5 list counters, 8 standby priorities, 8 repurposed priorities and the
    /// pagefile-backed modified count). The exact size is requested first; if a
    /// future kernel reports a larger structure, the query is retried with a
    /// heap buffer of the size it asks for and only the known prefix is parsed.
    ///
    /// Requires `SeProfileSingleProcessPrivilege` to be enabled.
    pub fn query() -> Result<Self> {
        use crate::platform::nt::{STATUS_INFO_LENGTH_MISMATCH, SYSTEM_MEMORY_LIST_INFORMATION};

        const ENTRIES: usize = 22;

        let mut stack_buf = [0usize; ENTRIES];
        let mut return_length: u32 = 0;

        // SAFETY: stack_buf is a valid, zero-initialized array of the stated size.
        // return_length is a valid stack-allocated u32.
        let mut status = unsafe {
            crate::platform::nt::nt_query_system_information(
                SYSTEM_MEMORY_LIST_INFORMATION,
                stack_buf.as_mut_ptr().cast(),
                std::mem::size_of_val(&stack_buf) as u32,
                &raw mut return_length,
            )
        };

        let mut heap_buf: Vec<usize> = Vec::new();
        if status == STATUS_INFO_LENGTH_MISMATCH
            && return_length as usize > std::mem::size_of_val(&stack_buf)
        {
            heap_buf =
                vec![0usize; (return_length as usize).div_ceil(std::mem::size_of::<usize>())];
            // SAFETY: heap_buf holds at least `return_length` bytes, the size the
            // kernel asked for.
            status = unsafe {
                crate::platform::nt::nt_query_system_information(
                    SYSTEM_MEMORY_LIST_INFORMATION,
                    heap_buf.as_mut_ptr().cast(),
                    return_length,
                    &raw mut return_length,
                )
            };
        }

        if status != crate::platform::nt::STATUS_SUCCESS {
            bail!(
                "NtQuerySystemInformation(SystemMemoryListInformation) failed: NTSTATUS 0x{status:08X}"
            );
        }

        let buf: &[usize] = if heap_buf.is_empty() {
            &stack_buf
        } else {
            &heap_buf
        };
        // Only trust what the kernel says it wrote. Some builds report 0 on
        // success when the buffer is exactly the structure size.
        let written = if return_length == 0 {
            buf.len()
        } else {
            (return_length as usize / std::mem::size_of::<usize>()).min(buf.len())
        };
        if written < ENTRIES {
            bail!(
                "NtQuerySystemInformation(SystemMemoryListInformation) returned {return_length} bytes, need at least {}",
                ENTRIES * std::mem::size_of::<usize>()
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

    /// Pages on the zeroed and free lists combined (truly unused RAM).
    #[must_use]
    pub const fn free_and_zeroed_pages(&self) -> u64 {
        self.zeroed_pages + self.free_pages
    }

    /// Total standby pages across all priority levels.
    #[must_use]
    pub fn total_standby_pages(&self) -> u64 {
        self.standby_pages.iter().sum()
    }
}

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
        use crate::platform::nt::{SYSTEM_FILE_CACHE_INFORMATION, SystemFileCacheInfo};

        let mut info: SystemFileCacheInfo = unsafe { std::mem::zeroed() };
        let mut return_length: u32 = 0;

        // SAFETY: info is a valid, zero-initialized SystemFileCacheInfo struct.
        // return_length is a valid stack-allocated u32.
        let status = unsafe {
            crate::platform::nt::nt_query_system_information(
                SYSTEM_FILE_CACHE_INFORMATION,
                (&raw mut info).cast(),
                std::mem::size_of::<SystemFileCacheInfo>() as u32,
                &raw mut return_length,
            )
        };

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

#[cfg(test)]
mod tests {
    use super::*;

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
