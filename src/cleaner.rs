//! # `MagicX` RAM Cleaner — Core Memory Cleaning Operations
//!
//! This module implements all memory cleaning operations, from gentle
//! working set trimming to aggressive full standby list purging.
//! Each operation is independently callable for maximum control.

use anyhow::{Result, bail};
use colored::Colorize;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Memory::SetSystemFileCacheSize;
use windows_sys::Win32::System::ProcessStatus::K32EmptyWorkingSet;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_SET_QUOTA,
};

use crate::ntapi::{self, MemoryListCommand};
use crate::stats::{MemorySnapshot, QuickMemoryReading};

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
    // Use the full snapshot's total_physical via a one-time read.
    let total_physical = {
        let snap = MemorySnapshot::capture()?;
        snap.total_physical
    };
    let jitter_threshold = (total_physical / 10_000).max(MIN_JITTER_BYTES);

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
            stable_count = 0; // reset — still changing
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

/// Format an NTSTATUS code into a human-readable `NtSetSystemInformation` error message.
fn ntstatus_error_message(status: ntapi::NtStatus) -> String {
    format!(
        "NtSetSystemInformation failed: 0x{:08X} — {}",
        status as u32,
        ntapi::ntstatus_message(status)
    )
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
                "Purge ALL Standby",
                "All standby pages purged",
                "Purging ALL standby pages...",
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

    match ntapi::execute_memory_command(command) {
        Ok(()) => {
            let after = wait_for_settle(verbose, settle)?;
            Ok(CleanResult::success(name, success_msg, &before, &after))
        }
        Err(status) => Ok(CleanResult::failure(
            name,
            ntstatus_error_message(status),
            &before,
        )),
    }
}

/// Result of a cleaning operation, with before/after memory stats.
#[derive(Debug)]
pub struct CleanResult {
    pub operation: String,
    pub success: bool,
    pub freed_bytes: i64, // can be negative if memory increased during clean
    pub message: String,
    /// Available physical memory before the operation (bytes).
    pub available_before: u64,
    /// Available physical memory after the operation settled (bytes).
    pub available_after: u64,
    /// Memory load percentage before.
    pub load_before: u32,
    /// Memory load percentage after.
    pub load_after: u32,
}

impl CleanResult {
    /// Create a successful result from before/after snapshots.
    fn success(
        operation: &str,
        message: impl Into<String>,
        before: &MemorySnapshot,
        after: &MemorySnapshot,
    ) -> Self {
        Self {
            operation: operation.into(),
            success: true,
            freed_bytes: after.available_physical as i64 - before.available_physical as i64,
            message: message.into(),
            available_before: before.available_physical,
            available_after: after.available_physical,
            load_before: before.memory_load_percent,
            load_after: after.memory_load_percent,
        }
    }

    /// Create a failure result (no memory change).
    fn failure(operation: &str, message: String, before: &MemorySnapshot) -> Self {
        Self {
            operation: operation.into(),
            success: false,
            freed_bytes: 0,
            message,
            available_before: before.available_physical,
            available_after: before.available_physical,
            load_before: before.memory_load_percent,
            load_after: before.memory_load_percent,
        }
    }
}

/// Cleaning aggressiveness level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub enum CleanLevel {
    /// Gentle: Only purge low-priority standby pages.
    /// Safe for everyday use — preserves important cached data.
    Gentle,
    /// Moderate: Empty working sets + purge low-priority standby.
    /// Good balance between freeing RAM and keeping frequently-used data cached.
    Moderate,
    /// Aggressive: File cache trim + empty working sets + flush modified + purge ALL standby.
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

/// Output from a smart cleaning run, including per-operation results and overall metrics.
///
/// Returned by [`smart_clean`] so callers can decide how to present the results
/// (e.g. summary table, JSON, or logging).
#[derive(Debug)]
pub struct SmartCleanResult {
    /// Individual operation results.
    pub results: Vec<CleanResult>,
    /// Memory state before any cleaning started.
    pub overall_before: MemorySnapshot,
    /// Memory state after all cleaning completed.
    pub overall_after: MemorySnapshot,
    /// Net bytes freed (positive = more available memory after cleaning).
    pub total_freed: i64,
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

    // Pass (usize::MAX, usize::MAX, 0) to purge all cached pages,
    // then restore default limits so Windows resumes normal cache management.
    // SAFETY: SetSystemFileCacheSize with (MAX, MAX, 0) is the documented way to
    // purge the file system cache. Requires SeIncreaseQuotaPrivilege.
    let result = unsafe { SetSystemFileCacheSize(usize::MAX, usize::MAX, 0) };

    if result == 0 {
        let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };

        // Restore is not needed on failure
        return Ok(CleanResult::failure(
            "Flush File Cache",
            format!("SetSystemFileCacheSize failed (error {err}). Need SeIncreaseQuotaPrivilege."),
            &before,
        ));
    }

    // Restore default cache behavior (let Windows manage it again)
    let restore = unsafe { SetSystemFileCacheSize(0, 0, 0) };
    if restore == 0 {
        let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        return Ok(CleanResult::failure(
            "Flush File Cache",
            format!(
                "Cache purged but restore to default failed (error {err}). \
                 System file cache may be in a degraded state until next reboot."
            ),
            &before,
        ));
    }

    let after = wait_for_settle(verbose, settle)?;

    Ok(CleanResult::success(
        "Flush File Cache",
        "File system cache flushed successfully",
        &before,
        &after,
    ))
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
/// Protected/system processes may fail — that's normal.
pub fn empty_working_sets_per_process(verbose: bool, exclude_pids: &[u32]) -> Result<CleanResult> {
    if verbose {
        println!("  {} Emptying working sets per-process...", "→".cyan());
    }

    let before = MemorySnapshot::capture()?;
    let mut success_count = 0u32;
    let mut fail_count = 0u32;
    let current_pid = std::process::id();

    // Use Toolhelp32 snapshot for process enumeration (more reliable than K32EnumProcesses)
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        bail!("CreateToolhelp32Snapshot failed");
    }

    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

    let mut has_entry = unsafe { Process32FirstW(snapshot, &raw mut entry) } != 0;

    while has_entry {
        let pid = entry.th32ProcessID;

        // Skip System (PID 0, 4), ourselves, and excluded PIDs
        if pid != 0 && pid != 4 && pid != current_pid && !exclude_pids.contains(&pid) {
            let handle: HANDLE =
                unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_SET_QUOTA, 0, pid) };

            if handle.is_null() {
                fail_count += 1;
            } else {
                let result = unsafe { K32EmptyWorkingSet(handle) };
                if result != 0 {
                    success_count += 1;
                } else {
                    fail_count += 1;
                }
                unsafe { CloseHandle(handle) };
            }
        }

        has_entry = unsafe { Process32NextW(snapshot, &raw mut entry) } != 0;
    }

    unsafe { CloseHandle(snapshot) };

    let after = wait_for_settle(verbose, SettleMode::Full)?;

    Ok(CleanResult::success(
        "Empty Working Sets (Per-Process)",
        format!("Trimmed {success_count} processes, {fail_count} skipped (protected/system)"),
        &before,
        &after,
    ))
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
/// This is the gentlest standby purge — it frees pages that Windows would
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
/// The most impactful single operation — removes ALL cached pages from RAM.
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
/// copy-on-write. Only available on Windows 10+. This is unique to `MagicX` —
/// `EmptyStandbyList` doesn't support this.
///
/// This is a heavier operation that scans all memory — may take several seconds.
pub fn combine_memory(verbose: bool) -> Result<CleanResult> {
    combine_memory_with_settle(verbose, SettleMode::Full)
}

/// Inner implementation of memory combining with configurable settle mode.
fn combine_memory_with_settle(verbose: bool, settle: SettleMode) -> Result<CleanResult> {
    if verbose {
        println!("  {} Running memory page combining...", "→".cyan());
    }

    let before = MemorySnapshot::capture()?;

    match ntapi::execute_combine_memory() {
        Ok(pages_combined) => {
            let after = wait_for_settle(verbose, settle)?;
            Ok(CleanResult::success(
                "Memory Combining",
                format!("Pages combined: {pages_combined}"),
                &before,
                &after,
            ))
        }
        Err(status) => Ok(CleanResult::failure(
            "Memory Combining",
            format!(
                "NtSetSystemInformation(SystemCombinePhysicalMemoryInformation) failed: 0x{:08X} — {}",
                status as u32,
                ntapi::ntstatus_message(status)
            ),
            &before,
        )),
    }
}

// ─── Smart Cleaning Engine ───────────────────────────────────────────────────

/// Run a smart cleaning sequence based on the given level.
///
/// Execute the aggressive cleaning sequence (4 operations).
///
/// File cache flush → Empty working sets → Flush modified → Purge ALL standby.
/// All intermediate operations use [`SettleMode::Quick`]; only the final purge
/// uses [`SettleMode::Full`] for an accurate delta measurement.
fn execute_aggressive_chain(verbose: bool) -> Result<Vec<CleanResult>> {
    Ok(vec![
        flush_file_cache_with_settle(verbose, SettleMode::Quick)?,
        execute_kernel_memory_op(
            MemoryListCommand::EmptyWorkingSets,
            verbose,
            SettleMode::Quick,
        )?,
        execute_kernel_memory_op(
            MemoryListCommand::FlushModifiedList,
            verbose,
            SettleMode::Quick,
        )?,
        execute_kernel_memory_op(
            MemoryListCommand::PurgeStandbyList,
            verbose,
            SettleMode::Full,
        )?,
    ])
}

/// Execute the nuclear cleaning sequence (7 operations).
///
/// All of aggressive + memory combining + a second pass of flush modified +
/// purge all standby. Only the very last purge uses [`SettleMode::Full`].
fn execute_nuclear_chain(verbose: bool) -> Result<Vec<CleanResult>> {
    let mut results = Vec::with_capacity(7);

    // Phase 1: Cache + Working sets
    results.push(flush_file_cache_with_settle(verbose, SettleMode::Quick)?);
    results.push(execute_kernel_memory_op(
        MemoryListCommand::EmptyWorkingSets,
        verbose,
        SettleMode::Quick,
    )?);

    // Phase 2: Flush modified to disk
    results.push(execute_kernel_memory_op(
        MemoryListCommand::FlushModifiedList,
        verbose,
        SettleMode::Quick,
    )?);

    // Phase 3: Purge all standby pages (covers low-priority too)
    results.push(execute_kernel_memory_op(
        MemoryListCommand::PurgeStandbyList,
        verbose,
        SettleMode::Quick,
    )?);

    // Phase 4: Memory combining
    results.push(combine_memory_with_settle(verbose, SettleMode::Quick)?);

    // Phase 5: Second pass — modified pages generated during combine
    if verbose {
        println!("  {} Running second pass cleanup...", "→".cyan());
    }
    results.push(execute_kernel_memory_op(
        MemoryListCommand::FlushModifiedList,
        verbose,
        SettleMode::Quick,
    )?);
    results.push(execute_kernel_memory_op(
        MemoryListCommand::PurgeStandbyList,
        verbose,
        SettleMode::Full,
    )?);

    Ok(results)
}

/// This is the main cleaning entry point that orchestrates multiple operations
/// in the optimal order for maximum RAM recovery.
///
/// ## Cleaning Sequence by Level
///
/// | Level | Operations |
/// |---|---|
/// | **Gentle** | Purge low-priority standby only |
/// | **Moderate** | Empty working sets (kernel) → Purge low-priority standby |
/// | **Aggressive** | File cache flush → Empty working sets → Flush modified → Purge ALL standby |
/// | **Nuclear** | All of aggressive + memory combining + second pass |
///
/// ## Settle Optimisation
///
/// Intermediate operations use [`SettleMode::Quick`] (1 stable read, 0.8 s max)
/// instead of [`SettleMode::Full`] (3 stable reads, 2 s max). Only the final
/// operation in each chain uses `Full`, since only the overall delta matters.
/// This reduces worst-case settle overhead from ~14 s (7 ops × 2 s) to ~4.8 s
/// (6 × 0.8 s + 1 × 2 s) on a Nuclear clean.
pub fn smart_clean(level: CleanLevel, verbose: bool) -> Result<SmartCleanResult> {
    let overall_before = MemorySnapshot::capture()?;

    let results = match level {
        CleanLevel::Gentle => {
            // Single operation — always Full
            vec![execute_kernel_memory_op(
                MemoryListCommand::PurgeLowPriorityStandbyList,
                verbose,
                SettleMode::Full,
            )?]
        }
        CleanLevel::Moderate => {
            vec![
                execute_kernel_memory_op(
                    MemoryListCommand::EmptyWorkingSets,
                    verbose,
                    SettleMode::Quick,
                )?,
                execute_kernel_memory_op(
                    MemoryListCommand::PurgeLowPriorityStandbyList,
                    verbose,
                    SettleMode::Full, // last op
                )?,
            ]
        }
        CleanLevel::Aggressive => execute_aggressive_chain(verbose)?,
        CleanLevel::Nuclear => execute_nuclear_chain(verbose)?,
    };

    // Each operation already settles internally, so just capture final state
    let overall_after = MemorySnapshot::capture()?;
    let total_freed =
        overall_after.available_physical as i64 - overall_before.available_physical as i64;

    Ok(SmartCleanResult {
        results,
        overall_before,
        overall_after,
        total_freed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::MemorySnapshot;

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
        }
    }

    #[test]
    fn clean_result_success_calculates_freed_bytes() {
        let before = mock_snapshot(4_000_000_000, 75);
        let after = mock_snapshot(6_000_000_000, 62);
        let result = CleanResult::success("Test Op", "ok", &before, &after);

        assert!(result.success);
        assert_eq!(result.freed_bytes, 2_000_000_000);
        assert_eq!(result.operation, "Test Op");
        assert_eq!(result.message, "ok");
        assert_eq!(result.load_before, 75);
        assert_eq!(result.load_after, 62);
    }

    #[test]
    fn clean_result_success_with_dynamic_message() {
        let before = mock_snapshot(4_000_000_000, 75);
        let after = mock_snapshot(5_000_000_000, 69);
        let result = CleanResult::success("Op", format!("freed {} items", 42), &before, &after);

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
    }

    #[test]
    fn clean_result_negative_freed_when_memory_decreases() {
        let before = mock_snapshot(6_000_000_000, 62);
        let after = mock_snapshot(4_000_000_000, 75);
        let result = CleanResult::success("Test", "mem decreased", &before, &after);

        assert!(result.freed_bytes < 0);
        assert_eq!(result.freed_bytes, -2_000_000_000);
    }

    #[test]
    fn clean_level_ordering() {
        assert!(CleanLevel::Gentle < CleanLevel::Moderate);
        assert!(CleanLevel::Moderate < CleanLevel::Aggressive);
        assert!(CleanLevel::Aggressive < CleanLevel::Nuclear);
    }
}
