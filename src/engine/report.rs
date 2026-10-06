//! Results of cleaning operations.

use serde::Serialize;

use crate::memory::MemorySnapshot;

/// Result of a cleaning operation, with before/after memory stats.
#[derive(Debug, Serialize)]
pub struct CleanResult {
    /// Human-readable name of the cleaning operation performed.
    pub operation: String,
    /// Whether the operation completed successfully.
    pub success: bool,
    /// Net change in *available* memory in bytes (free + zeroed + standby).
    ///
    /// Grows when pages leave working sets or the modified list. Purging the
    /// standby list barely moves it, because standby pages already count as
    /// available; see [`free_delta_bytes`](Self::free_delta_bytes) for that.
    /// Can be negative if other processes allocated memory meanwhile.
    pub freed_bytes: i64,
    /// Net change in *free* memory in bytes (zeroed + free page lists only).
    ///
    /// This is what purging the standby list increases. `None` when the
    /// kernel page-list query was unavailable before or after the operation.
    pub free_delta_bytes: Option<i64>,
    /// Bytes the operation reclaimed: the figure the UI shows (see
    /// [`reclaimed_bytes`](Self::reclaimed_bytes)). Stored so `--report`
    /// JSON carries it too.
    pub reclaimed_bytes: i64,
    /// Human-readable status or error message.
    pub message: String,
    /// Available physical memory before the operation (bytes).
    pub available_before: u64,
    /// Available physical memory after the operation settled (bytes).
    pub available_after: u64,
    /// Memory load percentage before.
    pub load_before: u32,
    /// Memory load percentage after.
    pub load_after: u32,
    /// Wall-clock time for the entire operation (seconds), including settle.
    pub elapsed_secs: f64,
}

impl CleanResult {
    /// Create a successful result from before/after snapshots.
    pub(super) fn success(
        operation: &str,
        message: impl Into<String>,
        before: &MemorySnapshot,
        after: &MemorySnapshot,
        elapsed: std::time::Duration,
    ) -> Self {
        let freed_bytes = after.available_physical as i64 - before.available_physical as i64;
        let free_delta_bytes = free_delta(before, after);
        Self {
            operation: operation.into(),
            success: true,
            freed_bytes,
            free_delta_bytes,
            reclaimed_bytes: larger_delta(freed_bytes, free_delta_bytes),
            message: message.into(),
            available_before: before.available_physical,
            available_after: after.available_physical,
            load_before: before.memory_load_percent,
            load_after: after.memory_load_percent,
            elapsed_secs: elapsed.as_secs_f64(),
        }
    }

    /// Create a failure result (no memory change).
    pub(super) fn failure(operation: &str, message: String, before: &MemorySnapshot) -> Self {
        Self {
            operation: operation.into(),
            success: false,
            freed_bytes: 0,
            free_delta_bytes: None,
            reclaimed_bytes: 0,
            message,
            available_before: before.available_physical,
            available_after: before.available_physical,
            load_before: before.memory_load_percent,
            load_after: before.memory_load_percent,
            elapsed_secs: 0.0,
        }
    }

    /// Bytes this operation reclaimed: the larger of the available and free
    /// deltas.
    ///
    /// Each operation moves pages in only one of these measures (trimming
    /// working sets or flushing modified pages grows Available, purging
    /// standby grows Free), so the larger one is what the operation achieved.
    #[must_use]
    pub const fn reclaimed_bytes(&self) -> i64 {
        self.reclaimed_bytes
    }
}

/// The larger of an available-memory delta and a free-memory delta (when
/// known): what an operation or run reclaimed.
pub(super) fn larger_delta(available_delta: i64, free_delta: Option<i64>) -> i64 {
    free_delta.map_or(available_delta, |free| free.max(available_delta))
}

/// Signed change in free (zeroed + free list) memory between two snapshots.
pub(super) fn free_delta(before: &MemorySnapshot, after: &MemorySnapshot) -> Option<i64> {
    Some(after.free_bytes()? as i64 - before.free_bytes()? as i64)
}

/// Output from a smart cleaning run, including per-operation results and overall metrics.
///
/// Returned by [`Cleaner::smart_clean`](super::Cleaner::smart_clean) so callers can decide how to present the results
/// (e.g. summary table, JSON, or logging).
#[derive(Debug, Serialize)]
pub struct SmartCleanResult {
    /// Individual operation results.
    pub results: Vec<CleanResult>,
    /// Memory state before any cleaning started.
    pub overall_before: MemorySnapshot,
    /// Memory state after all cleaning completed.
    pub overall_after: MemorySnapshot,
    /// Net change in available memory (positive = more available after cleaning).
    pub total_freed: i64,
    /// Net change in free (zeroed + free list) memory, when known.
    pub total_free_delta: Option<i64>,
    /// Bytes the whole run reclaimed: the headline figure (see
    /// [`reclaimed_bytes`](Self::reclaimed_bytes)).
    pub total_reclaimed: i64,
    /// Total wall-clock time for all operations (seconds).
    pub total_elapsed_secs: f64,
}

impl SmartCleanResult {
    /// Bytes the whole run reclaimed: the larger of the available and free
    /// deltas (see [`CleanResult::reclaimed_bytes`]).
    ///
    /// Every level ends with a standby purge, so the free delta is normally
    /// the larger one; the available delta is the fallback when the kernel
    /// page-list query is unavailable.
    #[must_use]
    pub const fn reclaimed_bytes(&self) -> i64 {
        self.total_reclaimed
    }

    /// Number of operations that reported failure.
    #[must_use]
    pub fn failed_count(&self) -> usize {
        self.results.iter().filter(|r| !r.success).count()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// Helper to build a minimal `MemorySnapshot` for testing.
    fn mock_snapshot(available: u64, load: u32) -> MemorySnapshot {
        MemorySnapshot {
            memory_load_percent: load,
            total_physical: 16 * 1024 * 1024 * 1024,
            available_physical: available,
            used_physical: 16 * 1024 * 1024 * 1024 - available,
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
    fn clean_result_success_calculates_freed_bytes() {
        let before = mock_snapshot(4_000_000_000, 75);
        let after = mock_snapshot(6_000_000_000, 62);
        let result =
            CleanResult::success("Test Op", "ok", &before, &after, Duration::from_millis(150));

        assert!(result.success);
        assert_eq!(result.freed_bytes, 2_000_000_000);
        assert_eq!(result.operation, "Test Op");
        assert_eq!(result.message, "ok");
        assert_eq!(result.load_before, 75);
        assert_eq!(result.load_after, 62);
        assert!(result.elapsed_secs > 0.0, "elapsed_secs should be positive");
    }

    #[test]
    fn clean_result_success_with_dynamic_message() {
        let before = mock_snapshot(4_000_000_000, 75);
        let after = mock_snapshot(5_000_000_000, 69);
        let result = CleanResult::success(
            "Op",
            format!("freed {} items", 42),
            &before,
            &after,
            Duration::from_secs(1),
        );

        assert!(result.success);
        assert_eq!(result.message, "freed 42 items");
    }

    #[test]
    fn clean_result_failure_has_zero_freed() {
        let snap = mock_snapshot(4_000_000_000, 75);
        let result = CleanResult::failure("Bad Op", "something broke".into(), &snap);

        assert!(!result.success);
        assert_eq!(result.freed_bytes, 0);
        assert_eq!(result.available_before, result.available_after);
        assert_eq!(result.load_before, result.load_after);
        assert!(
            result.elapsed_secs.abs() < f64::EPSILON,
            "failure elapsed should be 0"
        );
    }

    #[test]
    fn clean_result_negative_freed_when_memory_decreases() {
        let before = mock_snapshot(6_000_000_000, 62);
        let after = mock_snapshot(4_000_000_000, 75);
        let result = CleanResult::success(
            "Test",
            "mem decreased",
            &before,
            &after,
            Duration::from_millis(500),
        );

        assert!(result.freed_bytes < 0);
        assert_eq!(result.freed_bytes, -2_000_000_000);
    }

    #[test]
    fn clean_result_success_zero_freed_when_no_change() {
        let snap = mock_snapshot(4_000_000_000, 75);
        let result = CleanResult::success(
            "NoChange",
            "nothing changed",
            &snap,
            &snap,
            Duration::from_millis(100),
        );
        assert!(result.success);
        assert_eq!(
            result.freed_bytes, 0,
            "same before/after should yield 0 freed"
        );
    }

    #[test]
    fn clean_result_failure_preserves_snapshot_values() {
        let snap = mock_snapshot(8_000_000_000, 50);
        let result = CleanResult::failure("Op", "error".into(), &snap);
        assert_eq!(result.available_before, 8_000_000_000);
        assert_eq!(result.available_after, 8_000_000_000);
        assert_eq!(result.load_before, 50);
        assert_eq!(result.load_after, 50);
    }

    #[test]
    fn reclaimed_bytes_prefers_larger_measure() {
        assert_eq!(
            larger_delta(0, None),
            0,
            "no list data: falls back to available"
        );
        assert_eq!(
            larger_delta(0, Some(2_000_000_000)),
            2_000_000_000,
            "standby purge shows up as free"
        );
        assert_eq!(
            larger_delta(3_000_000_000, Some(2_000_000_000)),
            3_000_000_000
        );
    }

    #[test]
    fn the_report_json_carries_the_reclaimed_figure() {
        let snap = mock_snapshot(4_000_000_000, 75);
        let result = CleanResult::success("Purge", "ok", &snap, &snap, Duration::from_millis(10));
        let json = serde_json::to_value(&result).expect("serializes");
        assert_eq!(json["reclaimed_bytes"], 0);
    }
}
