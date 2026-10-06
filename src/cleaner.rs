//! # `MagicX` RAM Cleaner - Core Memory Cleaning Operations
//!
//! This module implements all memory cleaning operations, from gentle
//! working set trimming to aggressive full standby list purging.
//! Each operation is independently callable for maximum control.

use crate::memory::{MemorySnapshot, QuickMemoryReading, format_bytes};
use crate::platform::nt::{self, MemoryListCommand};
use crate::platform::{memory, process};
use anyhow::Result;
use colored::Colorize;
use serde::{Deserialize, Serialize};

// ─── Kernel Settle Detection ─────────────────────────────────────────────────

/// How thoroughly to wait for kernel memory settling.
///
/// `Full` is used for standalone operations and the last operation in a chain.
/// `Quick` is used for intermediate operations in `smart_clean` to avoid
/// spending 3+ seconds per operation waiting for sub-megabyte variations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettleMode {
    /// Standard: 3 consecutive stable reads, up to 20 polls (3s max).
    Full,
    /// Fast: 1 stable read, up to 8 polls (1.2s max). Good enough for
    /// intermediate operations where only per-op deltas are needed.
    Quick,
}

/// Wait for the kernel to finish processing memory operations.
///
/// After `NtSetSystemInformation` returns, the kernel continues reclaiming pages
/// asynchronously. This function polls `available_physical` until it stabilizes
/// (stops changing between consecutive reads) or a timeout is reached.
///
/// Uses [`QuickMemoryReading`] for polling (single Win32 call) and only captures
/// a full [`MemorySnapshot`] once memory has settled.
fn wait_for_settle(verbose: bool, mode: SettleMode) -> Result<MemorySnapshot> {
    const POLL_INTERVAL_MS: u64 = 100;
    const MIN_JITTER_BYTES: u64 = 4 * 1024 * 1024; // 4 MB absolute floor

    let (max_polls, stable_reads): (u32, u32) = match mode {
        SettleMode::Full => (20, 3), // 20 × 100ms = 2s max
        SettleMode::Quick => (8, 1), //  8 × 100ms = 0.8s max
    };

    let first = QuickMemoryReading::capture()?;

    // Scale the jitter threshold to total RAM: 0.01% of physical memory,
    // with a 4 MB floor. On a 16 GB system this is ~1.6 MB; on 128 GB ~13 MB.
    let jitter_threshold = (first.total_physical / 10_000).max(MIN_JITTER_BYTES);

    let mut prev_available = first.available_physical;
    let mut stable_count: u32 = 0;
    let mut polls_done: u32 = 0;

    for _ in 0..max_polls {
        std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
        polls_done += 1;
        let current = QuickMemoryReading::capture()?;

        // Consider "settled" when available memory hasn't moved by more than
        // the jitter threshold between polls (kernel page transitions produce jitter)
        let diff = (current.available_physical as i64 - prev_available as i64).unsigned_abs();
        if diff < jitter_threshold {
            stable_count += 1;
            if stable_count >= stable_reads {
                if verbose {
                    println!(
                        "    {} Memory settled after {}ms",
                        "·".dimmed(),
                        u64::from(polls_done) * POLL_INTERVAL_MS
                    );
                }
                // Only do the expensive full capture once settled
                return MemorySnapshot::capture();
            }
        } else {
            stable_count = 0; // reset - still changing
        }
        prev_available = current.available_physical;
    }

    // Timed out but still return the latest snapshot
    if verbose {
        println!(
            "    {} Memory still settling (timeout reached after {}ms, using latest reading)",
            "·".dimmed(),
            u64::from(max_polls) * POLL_INTERVAL_MS
        );
    }
    MemorySnapshot::capture()
}

/// Display metadata for kernel memory commands.
///
/// Centralises the operation name, success message, and verbose label for each
/// [`MemoryListCommand`] variant so they are defined once and reused across
/// public wrappers, chain helpers, and `smart_clean` dispatch.
impl MemoryListCommand {
    /// Returns `(operation_name, success_message, verbose_label)`.
    const fn display_info(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::CaptureAccessedBits | Self::CaptureAndResetAccessedBits => (
                "Capture Accessed Bits",
                "PTE accessed bits captured",
                "Capturing PTE accessed bits...",
            ),
            Self::EmptyWorkingSets => (
                "Empty Working Sets (Kernel)",
                "All process working sets emptied via kernel",
                "Emptying working sets (kernel-level)...",
            ),
            Self::FlushModifiedList => (
                "Flush Modified List",
                "Modified pages flushed to disk",
                "Flushing modified page list...",
            ),
            Self::PurgeLowPriorityStandbyList => (
                "Purge Low-Priority Standby",
                "Low-priority standby pages purged",
                "Purging low-priority standby pages...",
            ),
            Self::PurgeStandbyList => (
                "Purge All Standby",
                "All standby pages purged",
                "Purging all standby pages...",
            ),
        }
    }
}

/// Execute a kernel memory command with before/after measurement.
///
/// This is the common pattern for operations that go through
/// `NtSetSystemInformation(SystemMemoryListInformation)`: capture a before
/// snapshot, execute the command, wait for the kernel to settle, and return
/// a [`CleanResult`] with the delta.
///
/// Display strings (operation name, success message, verbose label) are derived
/// from [`MemoryListCommand::display_info`] so callers need only pass the
/// command variant, `verbose`, and [`SettleMode`].
fn execute_kernel_memory_op(
    command: MemoryListCommand,
    verbose: bool,
    settle: SettleMode,
) -> Result<CleanResult> {
    let (name, success_msg, verbose_label) = command.display_info();

    if verbose {
        println!("  {} {verbose_label}", "→".cyan());
    }

    let before = MemorySnapshot::capture()?;
    let start = std::time::Instant::now();

    match nt::execute_memory_command(command) {
        Ok(()) => {
            let after = wait_for_settle(verbose, settle)?;
            let elapsed = start.elapsed();
            Ok(CleanResult::success(
                name,
                success_msg,
                &before,
                &after,
                elapsed,
            ))
        }
        Err(status) => Ok(CleanResult::failure(
            name,
            format!(
                "NtSetSystemInformation failed: 0x{:08X}: {}",
                status as u32,
                nt::ntstatus_message(status)
            ),
            &before,
        )),
    }
}

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
    fn success(
        operation: &str,
        message: impl Into<String>,
        before: &MemorySnapshot,
        after: &MemorySnapshot,
        elapsed: std::time::Duration,
    ) -> Self {
        Self {
            operation: operation.into(),
            success: true,
            freed_bytes: after.available_physical as i64 - before.available_physical as i64,
            free_delta_bytes: free_delta(before, after),
            message: message.into(),
            available_before: before.available_physical,
            available_after: after.available_physical,
            load_before: before.memory_load_percent,
            load_after: after.memory_load_percent,
            elapsed_secs: elapsed.as_secs_f64(),
        }
    }

    /// Create a failure result (no memory change).
    fn failure(operation: &str, message: String, before: &MemorySnapshot) -> Self {
        Self {
            operation: operation.into(),
            success: false,
            freed_bytes: 0,
            free_delta_bytes: None,
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
    pub fn reclaimed_bytes(&self) -> i64 {
        self.free_delta_bytes
            .map_or(self.freed_bytes, |free| free.max(self.freed_bytes))
    }
}

/// Signed change in free (zeroed + free list) memory between two snapshots.
fn free_delta(before: &MemorySnapshot, after: &MemorySnapshot) -> Option<i64> {
    Some(after.free_bytes()? as i64 - before.free_bytes()? as i64)
}

/// Cleaning aggressiveness level.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum, Serialize, Deserialize,
)]
pub enum CleanLevel {
    /// Gentle: Purge ALL standby pages (priorities 0–7).
    /// Standby pages are already outside every process's working set;
    /// purging them is completely safe and frees the disk-page cache.
    Gentle,
    /// Moderate: Flush modified pages to disk, then purge ALL standby.
    /// No process working sets are touched - safe for running apps.
    /// More thorough than Gentle because it also drains the modified list.
    Moderate,
    /// Aggressive: File cache flush + registry flush + empty working sets + flush modified + purge ALL standby.
    /// Frees maximum RAM but may cause brief I/O spike as apps re-fault pages.
    Aggressive,
    /// Nuclear: Everything aggressive does, plus memory combining.
    /// Use when you need every last byte freed. May cause temporary slowdown.
    Nuclear,
}

impl std::fmt::Display for CleanLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gentle => write!(f, "gentle"),
            Self::Moderate => write!(f, "moderate"),
            Self::Aggressive => write!(f, "aggressive"),
            Self::Nuclear => write!(f, "nuclear"),
        }
    }
}

impl CleanLevel {
    /// Returns the Title Case display name for use in GUI labels, status
    /// messages, and terminal output where the level is shown as a proper name.
    ///
    /// Use this instead of [`Display`](std::fmt::Display) (which returns
    /// lowercase for CLI argument compatibility) whenever the context requires
    /// a capitalised label: result cards, combo boxes, log messages, etc.
    #[must_use]
    pub const fn title_case_name(self) -> &'static str {
        match self {
            Self::Gentle => crate::strings::levels::GENTLE_NAME,
            Self::Moderate => crate::strings::levels::MODERATE_NAME,
            Self::Aggressive => crate::strings::levels::AGGRESSIVE_NAME,
            Self::Nuclear => crate::strings::levels::NUCLEAR_NAME,
        }
    }
}

/// Output from a smart cleaning run, including per-operation results and overall metrics.
///
/// Returned by [`smart_clean`] so callers can decide how to present the results
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
    pub fn reclaimed_bytes(&self) -> i64 {
        self.total_free_delta
            .map_or(self.total_freed, |free| free.max(self.total_freed))
    }

    /// Number of operations that reported failure.
    #[must_use]
    pub fn failed_count(&self) -> usize {
        self.results.iter().filter(|r| !r.success).count()
    }
}

// ─── Individual Operations ───────────────────────────────────────────────────

/// **Operation 1: Trim the file system cache.**
///
/// Calls `SetSystemFileCacheSize` with minimum values to force Windows to release
/// file system cache pages. Requires `SeIncreaseQuotaPrivilege`.
///
/// This is more effective than what `EmptyStandbyList` does because it directly
/// targets the file cache, which is often the biggest consumer of standby pages.
pub fn flush_file_cache(verbose: bool) -> Result<CleanResult> {
    flush_file_cache_with_settle(verbose, SettleMode::Full)
}

/// Inner implementation of file cache flush with configurable settle mode.
fn flush_file_cache_with_settle(verbose: bool, settle: SettleMode) -> Result<CleanResult> {
    if verbose {
        println!("  {} Flushing file system cache...", "→".cyan());
    }

    let before = MemorySnapshot::capture()?;
    let start = std::time::Instant::now();

    if let Err(err) = memory::flush_system_file_cache() {
        return Ok(CleanResult::failure(
            "Flush File Cache",
            format!("SetSystemFileCacheSize failed (error {err}). Need SeIncreaseQuotaPrivilege."),
            &before,
        ));
    }

    let after = wait_for_settle(verbose, settle)?;
    let elapsed = start.elapsed();

    Ok(CleanResult::success(
        "Flush File Cache",
        "File system cache flushed successfully",
        &before,
        &after,
        elapsed,
    ))
}

/// **Operation 1b: Flush the Windows registry cache to disk.**
///
/// Calls `NtSetSystemInformation(SystemRegistryReconciliationInformation)` to
/// force all dirty registry hive pages to be written to disk. This frees
/// modified memory occupied by cached registry data.
///
/// Unique to `MagicX` - `EmptyStandbyList` cannot do this.
pub fn flush_registry_cache(verbose: bool) -> Result<CleanResult> {
    flush_registry_cache_with_settle(verbose, SettleMode::Full)
}

/// Inner implementation of registry cache flush with configurable settle mode.
fn flush_registry_cache_with_settle(verbose: bool, settle: SettleMode) -> Result<CleanResult> {
    if verbose {
        println!("  {} Flushing registry cache to disk...", "→".cyan());
    }

    let before = MemorySnapshot::capture()?;
    let start = std::time::Instant::now();

    match nt::execute_registry_flush() {
        Ok(()) => {
            let after = wait_for_settle(verbose, settle)?;
            let elapsed = start.elapsed();
            Ok(CleanResult::success(
                "Flush Registry Cache",
                "Registry hive cache flushed to disk",
                &before,
                &after,
                elapsed,
            ))
        }
        Err(status) => Ok(CleanResult::failure(
            "Flush Registry Cache",
            format!(
                "NtSetSystemInformation(SystemRegistryReconciliationInformation) failed: \
                 0x{:08X}: {}",
                status as u32,
                nt::ntstatus_message(status)
            ),
            &before,
        )),
    }
}

/// **Operation 2: Empty all process working sets (kernel-level).**
///
/// Uses NtSetSystemInformation(MemoryEmptyWorkingSets) which is MORE powerful
/// than iterating processes with `EmptyWorkingSet()`:
/// - Hits ALL processes including protected/system processes
/// - Single kernel call vs hundreds of user-mode calls
/// - No handle permission issues
///
/// This is one area where `MagicX` surpasses `EmptyStandbyList` significantly.
pub fn empty_working_sets_kernel(verbose: bool) -> Result<CleanResult> {
    execute_kernel_memory_op(
        MemoryListCommand::EmptyWorkingSets,
        verbose,
        SettleMode::Full,
    )
}

/// **Operation 2b: Empty working sets per-process (user-mode fallback).**
///
/// Iterates all processes and calls `EmptyWorkingSet` on each.
/// Less powerful than kernel-level but provides per-process reporting.
/// Protected/system processes may fail - that's normal.
///
/// Processes whose executable name (case-insensitive) matches any entry in
/// `exclude_names` are skipped. Names are matched with or without the `.exe`
/// suffix - e.g. `"chrome"` matches `chrome.exe`.
pub fn empty_working_sets_per_process(
    verbose: bool,
    exclude_names: &[String],
) -> Result<CleanResult> {
    empty_working_sets_per_process_with_settle(verbose, exclude_names, SettleMode::Full)
}

/// Inner implementation of per-process working set emptying with configurable
/// settle mode.
fn empty_working_sets_per_process_with_settle(
    verbose: bool,
    exclude_names: &[String],
    settle: SettleMode,
) -> Result<CleanResult> {
    if verbose {
        println!("  {} Emptying working sets per-process...", "→".cyan());
    }

    let before = MemorySnapshot::capture()?;
    let start = std::time::Instant::now();
    let mut success_count = 0u32;
    let mut fail_count = 0u32;
    let mut excluded_count = 0u32;
    let current_pid = std::process::id();

    // Normalise exclude names: lowercase, strip trailing `.exe` if present
    let normalised_excludes: Vec<String> = exclude_names
        .iter()
        .map(|n| {
            let lower = n.to_lowercase();
            lower.strip_suffix(".exe").unwrap_or(&lower).to_owned()
        })
        .collect();

    for entry in process::processes()? {
        // Skip ourselves
        if entry.pid == current_pid {
            continue;
        }

        if is_excluded(&entry.name, &normalised_excludes) {
            if verbose {
                println!(
                    "    {} Skipping {} (PID {}, excluded)",
                    "·".dimmed(),
                    entry.name.yellow(),
                    entry.pid
                );
            }
            excluded_count += 1;
        } else if process::empty_working_set(entry.pid) {
            success_count += 1;
        } else {
            fail_count += 1;
        }
    }

    let after = wait_for_settle(verbose, settle)?;
    let elapsed = start.elapsed();

    let mut message =
        format!("Trimmed {success_count} processes, {fail_count} skipped (protected/system)");
    if excluded_count > 0 {
        use std::fmt::Write;
        // Writing to a String cannot fail.
        let _ = write!(message, ", {excluded_count} excluded by name");
    }

    Ok(CleanResult::success(
        "Empty Working Sets (Per-Process)",
        message,
        &before,
        &after,
        elapsed,
    ))
}

/// Check whether a process name matches any entry in the normalised exclude list.
///
/// Comparison is case-insensitive and matches with or without the `.exe` suffix.
fn is_excluded(exe_name: &str, normalised_excludes: &[String]) -> bool {
    let lower = exe_name.to_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    normalised_excludes.iter().any(|ex| ex == stem)
}

/// **Operation 3: Flush the modified page list.**
///
/// Forces Windows to write all modified (dirty) pages to disk/pagefile.
/// This MUST be done before purging standby for maximum effect, because
/// modified pages transition to standby after being written.
///
/// `EmptyStandbyList` supports this but many users don't know to use it first.
/// `MagicX`'s smart cleaning always does this automatically.
pub fn flush_modified_list(verbose: bool) -> Result<CleanResult> {
    execute_kernel_memory_op(
        MemoryListCommand::FlushModifiedList,
        verbose,
        SettleMode::Full,
    )
}

/// **Operation 4: Purge low-priority standby pages only.**
///
/// Removes only priority-0 standby pages from the standby list.
/// This is the gentlest standby purge - it frees pages that Windows would
/// reclaim first anyway, with minimal impact on cache performance.
pub fn purge_standby_low_priority(verbose: bool) -> Result<CleanResult> {
    execute_kernel_memory_op(
        MemoryListCommand::PurgeLowPriorityStandbyList,
        verbose,
        SettleMode::Full,
    )
}

/// **Operation 5: Purge ALL standby pages.**
///
/// The most impactful single operation - removes ALL cached pages from RAM.
/// This is equivalent to `EmptyStandbyList`'s main "standbylist" command.
///
/// **Warning**: After this, programs will need to re-read data from disk,
/// causing temporary I/O spikes.
pub fn purge_standby_all(verbose: bool) -> Result<CleanResult> {
    execute_kernel_memory_op(
        MemoryListCommand::PurgeStandbyList,
        verbose,
        SettleMode::Full,
    )
}

/// **Operation 6: Memory combining (deduplication).**
///
/// Scans physical memory for identical pages and combines them using
/// copy-on-write. Only available on Windows 10+. This is unique to `MagicX`  -
/// `EmptyStandbyList` doesn't support this.
///
/// This is a heavier operation that scans all memory - may take several seconds.
pub fn combine_memory(verbose: bool) -> Result<CleanResult> {
    combine_memory_with_settle(verbose, SettleMode::Full)
}

/// Inner implementation of memory combining with configurable settle mode.
fn combine_memory_with_settle(verbose: bool, settle: SettleMode) -> Result<CleanResult> {
    if verbose {
        println!("  {} Running memory page combining...", "→".cyan());
    }

    let before = MemorySnapshot::capture()?;
    let start = std::time::Instant::now();

    match nt::execute_combine_memory() {
        Ok(pages_combined) => {
            let after = wait_for_settle(verbose, settle)?;
            let elapsed = start.elapsed();
            Ok(CleanResult::success(
                "Memory Combining",
                format!("Pages combined: {pages_combined}"),
                &before,
                &after,
                elapsed,
            ))
        }
        Err(status) => Ok(CleanResult::failure(
            "Memory Combining",
            format!(
                "NtSetSystemInformation(SystemCombinePhysicalMemoryInformation) failed: 0x{:08X}: {}",
                status as u32,
                nt::ntstatus_message(status)
            ),
            &before,
        )),
    }
}

// ─── Smart Cleaning Engine ───────────────────────────────────────────────────

/// Maximum number of adaptive leftover sweeps after a chain's final purge.
const MAX_SWEEP_PASSES: u32 = 2;

/// Plan label for the adaptive leftover sweep shown by `--dry-run`.
const SWEEP_PLAN_LABEL: &str = "Leftover Sweep (only if needed)";

/// Choose between kernel-level or per-process working set emptying.
///
/// Uses the kernel-level `NtSetSystemInformation(EmptyWorkingSets)` command when
/// no process exclusions are requested. When `exclude_names` is non-empty, falls
/// back to per-process trimming so excluded processes are skipped.
fn empty_working_sets_op(
    verbose: bool,
    exclude_names: &[String],
    settle: SettleMode,
) -> Result<CleanResult> {
    if exclude_names.is_empty() {
        execute_kernel_memory_op(MemoryListCommand::EmptyWorkingSets, verbose, settle)
    } else {
        empty_working_sets_per_process_with_settle(verbose, exclude_names, settle)
    }
}

/// Flush the modified list, then purge all standby pages.
///
/// This is the tail of every level from Moderate up. The flush always uses a
/// full settle: the modified page writer finishes its I/O asynchronously, and
/// pages still in flight when the purge runs land on the standby list right
/// afterwards as leftovers.
fn flush_and_purge(verbose: bool, results: &mut Vec<CleanResult>) -> Result<()> {
    results.push(execute_kernel_memory_op(
        MemoryListCommand::FlushModifiedList,
        verbose,
        SettleMode::Full,
    )?);
    results.push(execute_kernel_memory_op(
        MemoryListCommand::PurgeStandbyList,
        verbose,
        SettleMode::Full,
    )?);
    Ok(())
}

/// Standby plus pagefile-backed modified memory in bytes: what another
/// flush + purge could still reclaim. `None` when the page lists are unknown.
fn leftover_bytes(snapshot: &MemorySnapshot) -> Option<u64> {
    let lists = snapshot.lists.as_ref()?;
    Some(
        (lists.total_standby_pages() + lists.modified_pagefile_pages)
            .saturating_mul(snapshot.page_size),
    )
}

/// Leftovers below this are normal background churn and not worth another
/// pass: 1% of physical RAM, at least 64 MB.
fn sweep_threshold(total_physical: u64) -> u64 {
    (total_physical / 100).max(64 * 1024 * 1024)
}

/// Whether another sweep is worthwhile given the current and previous
/// leftover sizes.
///
/// Requires leftovers above [`sweep_threshold`], and after a first pass also
/// requires that pass to have shrunk them by at least a quarter. Less progress
/// than that means the system refills the cache as fast as it is purged
/// (`SysMain` prefetching, heavy file I/O) and more passes would only burn time.
fn should_sweep(leftover: u64, previous: Option<u64>, total_physical: u64) -> bool {
    leftover >= sweep_threshold(total_physical)
        && previous.is_none_or(|prev| leftover.saturating_mul(4) < prev.saturating_mul(3))
}

/// Re-run flush + purge while meaningful leftovers remain (see [`should_sweep`]).
///
/// Each pass that runs is appended to `results` as one operation.
fn leftover_sweep(verbose: bool, results: &mut Vec<CleanResult>) -> Result<()> {
    let mut previous: Option<u64> = None;

    for pass in 1..=MAX_SWEEP_PASSES {
        let before = MemorySnapshot::capture()?;
        let Some(leftover) = leftover_bytes(&before) else {
            return Ok(()); // page lists unavailable: nothing to measure against
        };
        if !should_sweep(leftover, previous, before.total_physical) {
            return Ok(());
        }
        previous = Some(leftover);

        if verbose {
            println!(
                "  {} Sweeping {} of leftover standby/modified pages (pass {pass})...",
                "→".cyan(),
                format_bytes(leftover)
            );
        }

        let name = format!("Leftover Sweep (pass {pass})");
        let start = std::time::Instant::now();

        // A failed flush is not fatal: the purge still reclaims the standby part.
        if nt::execute_memory_command(MemoryListCommand::FlushModifiedList).is_ok() {
            wait_for_settle(false, SettleMode::Quick)?;
        }

        let result = match nt::execute_memory_command(MemoryListCommand::PurgeStandbyList) {
            Ok(()) => {
                let after = wait_for_settle(verbose, SettleMode::Full)?;
                let remaining = leftover_bytes(&after).unwrap_or(0);
                CleanResult::success(
                    &name,
                    format!(
                        "Leftovers {} -> {}",
                        format_bytes(leftover),
                        format_bytes(remaining)
                    ),
                    &before,
                    &after,
                    start.elapsed(),
                )
            }
            Err(status) => CleanResult::failure(
                &name,
                format!(
                    "Standby purge failed: 0x{:08X}: {}",
                    status as u32,
                    nt::ntstatus_message(status)
                ),
                &before,
            ),
        };

        let failed = !result.success;
        results.push(result);
        if failed {
            return Ok(());
        }
    }

    Ok(())
}

/// Execute the aggressive cleaning sequence.
///
/// File cache flush → Registry flush → Empty working sets → Flush modified →
/// Purge ALL standby. The first three use [`SettleMode::Quick`]; the flush and
/// purge use [`SettleMode::Full`] (see [`flush_and_purge`]).
///
/// When `exclude_names` is non-empty, working sets are emptied per-process
/// (skipping excluded names) instead of using the kernel-level command.
fn execute_aggressive_chain(verbose: bool, exclude_names: &[String]) -> Result<Vec<CleanResult>> {
    let mut results = vec![
        flush_file_cache_with_settle(verbose, SettleMode::Quick)?,
        flush_registry_cache_with_settle(verbose, SettleMode::Quick)?,
        empty_working_sets_op(verbose, exclude_names, SettleMode::Quick)?,
    ];
    flush_and_purge(verbose, &mut results)?;
    Ok(results)
}

/// Execute the nuclear cleaning sequence.
///
/// The aggressive chain, then memory combining, then a second flush + purge
/// for the pages that combining released or dirtied.
///
/// When `exclude_names` is non-empty, working sets are emptied per-process.
fn execute_nuclear_chain(verbose: bool, exclude_names: &[String]) -> Result<Vec<CleanResult>> {
    let mut results = execute_aggressive_chain(verbose, exclude_names)?;
    results.push(combine_memory_with_settle(verbose, SettleMode::Quick)?);

    if verbose {
        println!("  {} Running second pass cleanup...", "→".cyan());
    }
    let second_pass_start = results.len();
    flush_and_purge(verbose, &mut results)?;
    // Label the second pass so it matches the dry-run plan and users can tell
    // the passes apart.
    for result in &mut results[second_pass_start..] {
        result.operation.push_str(" (2nd pass)");
    }

    Ok(results)
}

/// Return the ordered list of operation names that would run for a given level.
///
/// This is used by `--dry-run` to preview the cleaning plan without executing
/// any kernel operations. `has_excludes` only affects the working-set label
/// for `Aggressive` and `Nuclear` (the only levels that empty working sets).
/// `Gentle` and `Moderate` do not touch process working sets.
#[must_use]
pub fn dry_run_plan(level: CleanLevel, has_excludes: bool) -> Vec<&'static str> {
    let ws_label = if has_excludes {
        "Empty Working Sets (Per-Process, with exclusions)"
    } else {
        "Empty Working Sets (Kernel)"
    };

    match level {
        CleanLevel::Gentle => vec!["Purge All Standby"],
        CleanLevel::Moderate => vec!["Flush Modified List", "Purge All Standby", SWEEP_PLAN_LABEL],
        CleanLevel::Aggressive => vec![
            "Flush File Cache",
            "Flush Registry Cache",
            ws_label,
            "Flush Modified List",
            "Purge All Standby",
            SWEEP_PLAN_LABEL,
        ],
        CleanLevel::Nuclear => vec![
            "Flush File Cache",
            "Flush Registry Cache",
            ws_label,
            "Flush Modified List",
            "Purge All Standby",
            "Memory Combining",
            "Flush Modified List (2nd pass)",
            "Purge All Standby (2nd pass)",
            SWEEP_PLAN_LABEL,
        ],
    }
}

/// This is the main cleaning entry point that orchestrates multiple operations
/// in the optimal order for maximum RAM recovery.
///
/// ## Cleaning Sequence by Level
///
/// | Level | Operations |
/// |---|---|
/// | **Gentle** | Purge ALL standby (all priorities) |
/// | **Moderate** | Flush modified list → Purge ALL standby → leftover sweep |
/// | **Aggressive** | File cache flush → Registry flush → Empty working sets → Flush modified → Purge ALL standby → leftover sweep |
/// | **Nuclear** | Aggressive + memory combining + second flush/purge → leftover sweep |
///
/// ## Leftover Sweep
///
/// From Moderate up, the run ends with up to [`MAX_SWEEP_PASSES`] extra
/// flush + purge passes, each one only if standby/modified leftovers are still
/// significant and the previous pass made progress (see [`should_sweep`]).
///
/// ## Settle Optimisation
///
/// Operations whose effect is synchronous (file cache, registry, working sets,
/// combining) use `SettleMode::Quick` (1 stable read, 0.8 s max). The modified
/// flush and standby purge use `SettleMode::Full` (3 stable reads, 2 s max)
/// because write-back completes asynchronously.
///
/// ## Process Exclusion
///
/// When `exclude_names` is non-empty, the working-set-emptying step uses
/// per-process trimming instead of the kernel-level command. This allows
/// protecting specific applications (e.g. `chrome`, `firefox`) from having
/// their pages evicted. Only Aggressive and Nuclear empty working sets, so
/// exclusions have no effect at lower levels.
pub fn smart_clean(
    level: CleanLevel,
    verbose: bool,
    exclude_names: &[String],
) -> Result<SmartCleanResult> {
    let overall_before = MemorySnapshot::capture()?;
    let start = std::time::Instant::now();

    let mut results = match level {
        // Standby pages are already outside every process's working set, so
        // purging them is safe at any time.
        CleanLevel::Gentle => vec![execute_kernel_memory_op(
            MemoryListCommand::PurgeStandbyList,
            verbose,
            SettleMode::Full,
        )?],
        // No working-set eviction: running processes are unaffected; only
        // triggers an I/O spike while dirty pages are written out.
        CleanLevel::Moderate => {
            let mut results = Vec::with_capacity(4);
            flush_and_purge(verbose, &mut results)?;
            results
        }
        CleanLevel::Aggressive => execute_aggressive_chain(verbose, exclude_names)?,
        CleanLevel::Nuclear => execute_nuclear_chain(verbose, exclude_names)?,
    };

    if level >= CleanLevel::Moderate {
        leftover_sweep(verbose, &mut results)?;
    }

    // Each operation already settles internally, so just capture final state
    let overall_after = MemorySnapshot::capture()?;
    let total_freed =
        overall_after.available_physical as i64 - overall_before.available_physical as i64;
    let total_free_delta = free_delta(&overall_before, &overall_after);
    let total_elapsed_secs = start.elapsed().as_secs_f64();

    Ok(SmartCleanResult {
        results,
        overall_before,
        overall_after,
        total_freed,
        total_free_delta,
        total_elapsed_secs,
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::memory::MemorySnapshot;

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
    fn clean_level_ordering() {
        assert!(CleanLevel::Gentle < CleanLevel::Moderate);
        assert!(CleanLevel::Moderate < CleanLevel::Aggressive);
        assert!(CleanLevel::Aggressive < CleanLevel::Nuclear);
    }

    #[test]
    fn dry_run_plan_operation_counts() {
        assert_eq!(
            dry_run_plan(CleanLevel::Gentle, false).len(),
            1,
            "gentle = 1 op"
        );
        assert_eq!(
            dry_run_plan(CleanLevel::Moderate, false).len(),
            3,
            "moderate = 2 ops + sweep"
        );
        assert_eq!(
            dry_run_plan(CleanLevel::Aggressive, false).len(),
            6,
            "aggressive = 5 ops + sweep"
        );
        assert_eq!(
            dry_run_plan(CleanLevel::Nuclear, false).len(),
            9,
            "nuclear = 8 ops + sweep"
        );
    }

    #[test]
    fn dry_run_plan_with_excludes_shows_per_process() {
        let plan = dry_run_plan(CleanLevel::Aggressive, true);
        assert!(
            plan.iter().any(|op| op.contains("Per-Process")),
            "plan with excludes should mention per-process mode"
        );
        assert!(
            !plan.contains(&"Empty Working Sets (Kernel)"),
            "plan with excludes should NOT show kernel-level working set op"
        );
    }

    #[test]
    fn dry_run_plan_moderate_ops() {
        let plan = dry_run_plan(CleanLevel::Moderate, false);
        assert_eq!(
            plan,
            vec!["Flush Modified List", "Purge All Standby", SWEEP_PLAN_LABEL]
        );
        assert!(
            !plan.iter().any(|op| op.contains("Working Set")),
            "moderate should not touch process working sets"
        );
    }

    #[test]
    fn is_excluded_case_insensitive() {
        let excludes = vec!["chrome".to_owned(), "firefox".to_owned()];
        assert!(is_excluded("chrome.exe", &excludes));
        assert!(is_excluded("Chrome.EXE", &excludes));
        assert!(is_excluded("FIREFOX.exe", &excludes));
        assert!(!is_excluded("notepad.exe", &excludes));
    }

    #[test]
    fn is_excluded_empty_list() {
        let excludes: Vec<String> = vec![];
        assert!(
            !is_excluded("anything.exe", &excludes),
            "nothing should be excluded with an empty list"
        );
    }

    #[test]
    fn is_excluded_without_exe_suffix() {
        let excludes = vec!["notepad".to_owned()];
        assert!(
            is_excluded("notepad", &excludes),
            "should match process name without .exe suffix"
        );
    }

    #[test]
    fn is_excluded_with_exe_suffix_in_list() {
        // User passes "chrome.exe" - should still match "chrome.exe"
        let raw = ["chrome.exe".to_owned()];
        let normalised: Vec<String> = raw
            .iter()
            .map(|n| {
                let lower = n.to_lowercase();
                lower.strip_suffix(".exe").unwrap_or(&lower).to_owned()
            })
            .collect();
        assert!(is_excluded("chrome.exe", &normalised));
        assert!(is_excluded("Chrome.EXE", &normalised));
    }

    #[test]
    fn clean_level_display() {
        assert_eq!(CleanLevel::Gentle.to_string(), "gentle");
        assert_eq!(CleanLevel::Moderate.to_string(), "moderate");
        assert_eq!(CleanLevel::Aggressive.to_string(), "aggressive");
        assert_eq!(CleanLevel::Nuclear.to_string(), "nuclear");
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

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn sweep_threshold_scales_with_ram_and_has_floor() {
        assert_eq!(sweep_threshold(4 * GIB), 64 * 1024 * 1024, "floor applies");
        assert_eq!(sweep_threshold(64 * GIB), 64 * GIB / 100, "1% of RAM");
    }

    #[test]
    fn should_sweep_requires_meaningful_leftovers() {
        assert!(!should_sweep(10 * 1024 * 1024, None, 16 * GIB));
        assert!(should_sweep(GIB, None, 16 * GIB));
    }

    #[test]
    fn should_sweep_stops_without_progress() {
        // The first pass only shrank leftovers from 1 GiB to 900 MiB: refilling.
        assert!(!should_sweep(900 * 1024 * 1024, Some(GIB), 16 * GIB));
        // Halved: keep going.
        assert!(should_sweep(GIB / 2, Some(GIB), 16 * GIB));
    }

    #[test]
    fn reclaimed_bytes_prefers_larger_measure() {
        let snap = mock_snapshot(4_000_000_000, 75);
        let mut result =
            CleanResult::success("Purge", "ok", &snap, &snap, Duration::from_millis(10));
        assert_eq!(
            result.reclaimed_bytes(),
            0,
            "no list data: falls back to available"
        );
        result.free_delta_bytes = Some(2_000_000_000);
        assert_eq!(
            result.reclaimed_bytes(),
            2_000_000_000,
            "standby purge shows up as free"
        );
        result.freed_bytes = 3_000_000_000;
        assert_eq!(result.reclaimed_bytes(), 3_000_000_000);
    }
}
