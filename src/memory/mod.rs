//! # Memory domain types
//!
//! System memory snapshots, per-process usage and byte formatting, built on
//! the safe queries in [`crate::platform`]. Everything here is plain data
//! plus the logic to derive figures from it.

mod format;
mod process;

use anyhow::Result;
use serde::Serialize;

pub use self::format::{format_bytes, format_signed_bytes};
pub use self::process::{ProcessMemoryInfo, query_all_processes, query_top_processes};
use crate::platform::memory;
pub use crate::platform::memory::{FileCacheSnapshot, MemoryListInfo};

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

/// One of the kernel's physical memory lists, as the GUI presents them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryList {
    /// Pages held by running processes and the system (working sets).
    InUse,
    /// Changed pages waiting to be written to disk.
    Modified,
    /// Cached pages Windows can hand back instantly.
    Standby,
    /// Pages holding nothing (free and zeroed lists).
    Free,
}

/// How installed RAM divides into the four [`MemoryList`]s at one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryComposition {
    /// Bytes in use: installed RAM minus the other three lists.
    pub in_use: u64,
    /// Bytes on the modified list.
    pub modified: u64,
    /// Bytes on the standby lists (all priorities).
    pub standby: u64,
    /// Bytes on the free and zeroed lists.
    pub free: u64,
}

impl MemoryComposition {
    /// Bytes in `list`.
    #[must_use]
    pub const fn bytes(&self, list: MemoryList) -> u64 {
        match list {
            MemoryList::InUse => self.in_use,
            MemoryList::Modified => self.modified,
            MemoryList::Standby => self.standby,
            MemoryList::Free => self.free,
        }
    }

    /// Sum of all four lists.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.in_use + self.modified + self.standby + self.free
    }
}

impl MemorySnapshot {
    /// The split of installed RAM into memory lists, when the kernel page
    /// lists could be read.
    ///
    /// In use is derived as the remainder, so the four parts always add up
    /// to installed RAM.
    #[must_use]
    pub fn composition(&self) -> Option<MemoryComposition> {
        let free = self.free_bytes()?;
        let standby = self.standby_bytes()?;
        let modified = self.modified_bytes()?;
        let free = free.min(self.total_physical);
        let standby = standby.min(self.total_physical - free);
        let modified = modified.min(self.total_physical - free - standby);
        Some(MemoryComposition {
            in_use: self.total_physical - free - standby - modified,
            modified,
            standby,
            free,
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A snapshot of 16 GiB with the given page-list sizes in GiB.
    fn snapshot_with_lists(free: u64, standby: u64, modified: u64) -> MemorySnapshot {
        const GIB: u64 = 1024 * 1024 * 1024;
        let pages = |gib: u64| gib * GIB / 4096;
        MemorySnapshot {
            memory_load_percent: 0,
            total_physical: 16 * GIB,
            available_physical: 0,
            used_physical: 0,
            total_page_file: 0,
            available_page_file: 0,
            total_virtual: 0,
            available_virtual: 0,
            commit_total_pages: 0,
            commit_limit_pages: 0,
            commit_peak_pages: 0,
            physical_available_pages: 0,
            physical_total_pages: 0,
            kernel_paged_pages: 0,
            kernel_nonpaged_pages: 0,
            page_size: 4096,
            handle_count: 0,
            process_count: 0,
            thread_count: 0,
            lists: Some(MemoryListInfo {
                zeroed_pages: 0,
                free_pages: pages(free),
                modified_pages: pages(modified),
                modified_no_write_pages: 0,
                bad_pages: 0,
                standby_pages: [pages(standby), 0, 0, 0, 0, 0, 0, 0],
                repurposed_pages: [0; 8],
                modified_pagefile_pages: 0,
            }),
        }
    }

    #[test]
    fn composition_adds_up_to_installed_ram() {
        const GIB: u64 = 1024 * 1024 * 1024;
        let c = snapshot_with_lists(2, 5, 1)
            .composition()
            .expect("lists known");
        assert_eq!(c.free, 2 * GIB);
        assert_eq!(c.standby, 5 * GIB);
        assert_eq!(c.modified, GIB);
        assert_eq!(c.in_use, 8 * GIB);
        assert_eq!(c.total(), 16 * GIB);
    }

    #[test]
    fn composition_clamps_lists_that_overshoot_installed_ram() {
        let c = snapshot_with_lists(10, 10, 10)
            .composition()
            .expect("lists known");
        assert_eq!(c.total(), 16 * 1024 * 1024 * 1024);
        assert_eq!(c.in_use, 0);
        assert_eq!(c.modified, 0);
    }

    #[test]
    fn composition_is_unknown_without_page_lists() {
        let mut snap = snapshot_with_lists(1, 1, 1);
        snap.lists = None;
        assert!(snap.composition().is_none());
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
}
