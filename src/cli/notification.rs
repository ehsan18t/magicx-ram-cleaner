//! Short result summaries shown in balloon notifications (`--notify`).

use crate::engine::{CleanResult, SmartCleanResult};
use crate::memory::{self, MemorySnapshot};

/// Notification body for a smart clean.
pub(super) fn clean_summary(output: &SmartCleanResult) -> String {
    let freed = memory::format_signed_bytes(output.reclaimed_bytes());
    let before_load = output.overall_before.memory_load_percent;
    let after_load = output.overall_after.memory_load_percent;
    let ops = output.results.len();
    let ok = ops - output.failed_count();
    format!(
        "Freed {freed}\n{ok}/{ops} operations succeeded\nRAM usage: {before_load}% → {after_load}%"
    )
}

/// Notification body for a single operation.
pub(super) fn operation_summary(result: &CleanResult) -> String {
    let status = if result.success { "OK" } else { "FAILED" };
    let freed = memory::format_signed_bytes(result.reclaimed_bytes());
    format!(
        "{}: {status}\nFreed {freed}\nRAM usage: {}% → {}%",
        result.operation, result.load_before, result.load_after
    )
}

/// Notification body for a memory status snapshot.
pub(super) fn status_summary(snapshot: &MemorySnapshot) -> String {
    format!(
        "RAM: {} / {} ({}% used)\nAvailable: {}",
        memory::format_bytes(snapshot.used_physical),
        memory::format_bytes(snapshot.total_physical),
        snapshot.memory_load_percent,
        memory::format_bytes(snapshot.available_physical),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn snapshot(available: u64, load: u32) -> MemorySnapshot {
        MemorySnapshot {
            memory_load_percent: load,
            total_physical: 16 * GIB,
            available_physical: available,
            used_physical: 16 * GIB - available,
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
            lists: None,
        }
    }

    #[test]
    fn status_summary_shows_used_total_and_available() {
        assert_eq!(
            status_summary(&snapshot(4 * GIB, 75)),
            "RAM: 12.00 GB / 16.00 GB (75% used)\nAvailable: 4.00 GB"
        );
    }

    #[test]
    fn clean_summary_reports_freed_and_failures() {
        let output = SmartCleanResult {
            results: Vec::new(),
            overall_before: snapshot(4 * GIB, 75),
            overall_after: snapshot(6 * GIB, 62),
            total_freed: 2 * GIB as i64,
            total_free_delta: None,
            total_elapsed_secs: 1.0,
        };
        assert_eq!(
            clean_summary(&output),
            "Freed +2.00 GB\n0/0 operations succeeded\nRAM usage: 75% → 62%"
        );
    }
}
