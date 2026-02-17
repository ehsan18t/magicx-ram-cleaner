//! # MagicX RAM Cleaner — Core Memory Cleaning Operations
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
use crate::stats::{MemorySnapshot, format_bytes};

// ─── Kernel Settle Detection ─────────────────────────────────────────────────

/// Wait for the kernel to finish processing memory operations.
///
/// After `NtSetSystemInformation` returns, the kernel continues reclaiming pages
/// asynchronously. This function polls `available_physical` until it stabilizes
/// (stops changing between consecutive reads) or a timeout is reached.
///
/// Returns the settled `MemorySnapshot`.
fn wait_for_settle(verbose: bool) -> Result<MemorySnapshot> {
    const POLL_INTERVAL_MS: u64 = 150;
    const MAX_POLLS: u32 = 20; // 20 × 150ms = 3 seconds max
    const STABLE_READS: u32 = 3; // require 3 consecutive identical reads

    let mut prev = MemorySnapshot::capture()?;
    let mut stable_count: u32 = 0;
    let mut polls_done: u32 = 0;

    for _ in 0..MAX_POLLS {
        std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
        polls_done += 1;
        let current = MemorySnapshot::capture()?;

        // Consider "settled" when available memory hasn't moved by more than
        // 1 MB between polls (kernel page transitions produce small jitter)
        let diff =
            (current.available_physical as i64 - prev.available_physical as i64).unsigned_abs();
        if diff < 1_048_576 {
            stable_count += 1;
            if stable_count >= STABLE_READS {
                if verbose {
                    println!(
                        "    {} Memory settled after {}ms",
                        "·".dimmed(),
                        polls_done as u64 * POLL_INTERVAL_MS
                    );
                }
                return Ok(current);
            }
        } else {
            stable_count = 0; // reset — still changing
        }
        prev = current;
    }

    // Timed out but still return the latest snapshot
    if verbose {
        println!(
            "    {} Memory still settling (timeout reached after {}ms, using latest reading)",
            "·".dimmed(),
            MAX_POLLS as u64 * POLL_INTERVAL_MS
        );
    }
    MemorySnapshot::capture()
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

// ─── Individual Operations ───────────────────────────────────────────────────

/// **Operation 1: Trim the file system cache.**
///
/// Calls SetSystemFileCacheSize with minimum values to force Windows to release
/// file system cache pages. Requires SeIncreaseQuotaPrivilege.
///
/// This is more effective than what EmptyStandbyList does because it directly
/// targets the file cache, which is often the biggest consumer of standby pages.
pub fn flush_file_cache(verbose: bool) -> Result<CleanResult> {
    if verbose {
        println!("  {} Flushing file system cache...", "→".cyan());
    }

    let before = MemorySnapshot::capture()?;

    // Setting min and max to usize::MAX is the documented way to purge the cache
    // FILE_CACHE_MAX_HARD_ENABLE (0x1) + FILE_CACHE_MIN_HARD_ENABLE (0x4) = 0x5? No.
    // Actually: pass (usize::MAX, usize::MAX, 0) to release all cached pages,
    // then restore defaults.
    let result = unsafe { SetSystemFileCacheSize(usize::MAX, usize::MAX, 0) };

    if result == 0 {
        let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };

        // Restore is not needed on failure
        return Ok(CleanResult::failure(
            "Flush File Cache",
            format!(
                "SetSystemFileCacheSize failed (error {}). Need SeIncreaseQuotaPrivilege.",
                err
            ),
            &before,
        ));
    }

    // Restore default cache behavior (let Windows manage it again)
    let restore = unsafe { SetSystemFileCacheSize(0, 0, 0) };
    if restore == 0 {
        let err = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        eprintln!(
            "  {} Warning: failed to restore default cache size (error {})",
            "⚠".yellow(),
            err
        );
    }

    let after = wait_for_settle(verbose)?;

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
/// than iterating processes with EmptyWorkingSet():
/// - Hits ALL processes including protected/system processes
/// - Single kernel call vs hundreds of user-mode calls
/// - No handle permission issues
///
/// This is one area where MagicX surpasses EmptyStandbyList significantly.
pub fn empty_working_sets_kernel(verbose: bool) -> Result<CleanResult> {
    if verbose {
        println!("  {} Emptying working sets (kernel-level)...", "→".cyan());
    }

    let before = MemorySnapshot::capture()?;

    match ntapi::execute_memory_command(MemoryListCommand::EmptyWorkingSets) {
        Ok(()) => {
            let after = wait_for_settle(verbose)?;
            Ok(CleanResult::success(
                "Empty Working Sets (Kernel)",
                "All process working sets emptied via kernel",
                &before,
                &after,
            ))
        }
        Err(status) => Ok(CleanResult::failure(
            "Empty Working Sets (Kernel)",
            format!(
                "NtSetSystemInformation failed: 0x{:08X} — {}",
                status as u32,
                ntapi::ntstatus_message(status)
            ),
            &before,
        )),
    }
}

/// **Operation 2b: Empty working sets per-process (user-mode fallback).**
///
/// Iterates all processes and calls EmptyWorkingSet on each.
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

    let mut has_entry = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;

    while has_entry {
        let pid = entry.th32ProcessID;

        // Skip System (PID 0, 4), ourselves, and excluded PIDs
        if pid != 0 && pid != 4 && pid != current_pid && !exclude_pids.contains(&pid) {
            let handle: HANDLE =
                unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_SET_QUOTA, 0, pid) };

            if !handle.is_null() {
                let result = unsafe { K32EmptyWorkingSet(handle) };
                if result != 0 {
                    success_count += 1;
                } else {
                    fail_count += 1;
                }
                unsafe { CloseHandle(handle) };
            } else {
                fail_count += 1;
            }
        }

        has_entry = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }

    unsafe { CloseHandle(snapshot) };

    let after = wait_for_settle(verbose)?;

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
/// EmptyStandbyList supports this but many users don't know to use it first.
/// MagicX's smart cleaning always does this automatically.
pub fn flush_modified_list(verbose: bool) -> Result<CleanResult> {
    if verbose {
        println!("  {} Flushing modified page list...", "→".cyan());
    }

    let before = MemorySnapshot::capture()?;

    match ntapi::execute_memory_command(MemoryListCommand::FlushModifiedList) {
        Ok(()) => {
            let after = wait_for_settle(verbose)?;
            Ok(CleanResult::success(
                "Flush Modified List",
                "Modified pages flushed to disk",
                &before,
                &after,
            ))
        }
        Err(status) => Ok(CleanResult::failure(
            "Flush Modified List",
            format!(
                "NtSetSystemInformation failed: 0x{:08X} — {}",
                status as u32,
                ntapi::ntstatus_message(status)
            ),
            &before,
        )),
    }
}

/// **Operation 4: Purge low-priority standby pages only.**
///
/// Removes only priority-0 standby pages from the standby list.
/// This is the gentlest standby purge — it frees pages that Windows would
/// reclaim first anyway, with minimal impact on cache performance.
pub fn purge_standby_low_priority(verbose: bool) -> Result<CleanResult> {
    if verbose {
        println!("  {} Purging low-priority standby pages...", "→".cyan());
    }

    let before = MemorySnapshot::capture()?;

    match ntapi::execute_memory_command(MemoryListCommand::PurgeLowPriorityStandbyList) {
        Ok(()) => {
            let after = wait_for_settle(verbose)?;
            Ok(CleanResult::success(
                "Purge Low-Priority Standby",
                "Low-priority standby pages purged",
                &before,
                &after,
            ))
        }
        Err(status) => Ok(CleanResult::failure(
            "Purge Low-Priority Standby",
            format!(
                "NtSetSystemInformation failed: 0x{:08X} — {}",
                status as u32,
                ntapi::ntstatus_message(status)
            ),
            &before,
        )),
    }
}

/// **Operation 5: Purge ALL standby pages.**
///
/// The most impactful single operation — removes ALL cached pages from RAM.
/// This is equivalent to EmptyStandbyList's main "standbylist" command.
///
/// **Warning**: After this, programs will need to re-read data from disk,
/// causing temporary I/O spikes.
pub fn purge_standby_all(verbose: bool) -> Result<CleanResult> {
    if verbose {
        println!("  {} Purging ALL standby pages...", "→".cyan());
    }

    let before = MemorySnapshot::capture()?;

    match ntapi::execute_memory_command(MemoryListCommand::PurgeStandbyList) {
        Ok(()) => {
            let after = wait_for_settle(verbose)?;
            Ok(CleanResult::success(
                "Purge ALL Standby",
                "All standby pages purged",
                &before,
                &after,
            ))
        }
        Err(status) => Ok(CleanResult::failure(
            "Purge ALL Standby",
            format!(
                "NtSetSystemInformation failed: 0x{:08X} — {}",
                status as u32,
                ntapi::ntstatus_message(status)
            ),
            &before,
        )),
    }
}

/// **Operation 6: Memory combining (deduplication).**
///
/// Scans physical memory for identical pages and combines them using
/// copy-on-write. Only available on Windows 10+. This is unique to MagicX —
/// EmptyStandbyList doesn't support this.
///
/// This is a heavier operation that scans all memory — may take several seconds.
pub fn combine_memory(verbose: bool) -> Result<CleanResult> {
    if verbose {
        println!("  {} Running memory page combining...", "→".cyan());
    }

    let before = MemorySnapshot::capture()?;

    // MEMORY_COMBINE_INFORMATION_INPUT: Handle (HANDLE, pointer-width), PagesCombined (ULONG_PTR)
    #[repr(C)]
    struct CombineInfo {
        handle: usize,        // HANDLE — 0 for full scan
        page_combined: usize, // ULONG_PTR output
    }

    let mut info = CombineInfo {
        handle: 0,
        page_combined: 0,
    };

    let status = unsafe {
        ntapi::NtSetSystemInformation(
            ntapi::SYSTEM_COMBINE_PHYSICAL_MEMORY_INFORMATION,
            &mut info as *mut CombineInfo as *mut std::ffi::c_void,
            std::mem::size_of::<CombineInfo>() as u32,
        )
    };

    if status != 0 {
        return Ok(CleanResult::failure(
            "Memory Combining",
            format!(
                "NtSetSystemInformation(SystemCombinePhysicalMemoryInformation) failed: 0x{:08X} — {}",
                status as u32,
                ntapi::ntstatus_message(status)
            ),
            &before,
        ));
    }

    let after = wait_for_settle(verbose)?;

    Ok(CleanResult::success(
        "Memory Combining",
        format!("Pages combined: {}", info.page_combined),
        &before,
        &after,
    ))
}

// ─── Smart Cleaning Engine ───────────────────────────────────────────────────

/// Run a smart cleaning sequence based on the given level.
///
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
pub fn smart_clean(level: CleanLevel, verbose: bool) -> Result<Vec<CleanResult>> {
    let mut results = Vec::new();

    println!(
        "\n{} Starting {} clean...\n",
        "⚡".yellow(),
        level.to_string().bold()
    );

    let overall_before = MemorySnapshot::capture()?;

    match level {
        CleanLevel::Gentle => {
            results.push(purge_standby_low_priority(verbose)?);
        }
        CleanLevel::Moderate => {
            results.push(empty_working_sets_kernel(verbose)?);
            results.push(purge_standby_low_priority(verbose)?);
        }
        CleanLevel::Aggressive => {
            results.push(flush_file_cache(verbose)?);
            results.push(empty_working_sets_kernel(verbose)?);
            results.push(flush_modified_list(verbose)?);
            results.push(purge_standby_all(verbose)?);
        }
        CleanLevel::Nuclear => {
            // Phase 1: Cache + Working sets
            results.push(flush_file_cache(verbose)?);
            results.push(empty_working_sets_kernel(verbose)?);

            // Phase 2: Flush modified to disk
            results.push(flush_modified_list(verbose)?);

            // Phase 3: Purge all standby pages (covers low-priority too)
            results.push(purge_standby_all(verbose)?);

            // Phase 4: Memory combining
            results.push(combine_memory(verbose)?);

            // Phase 5: Second pass — modified pages generated during combine
            if verbose {
                println!("  {} Running second pass cleanup...", "→".cyan());
            }
            results.push(flush_modified_list(verbose)?);
            results.push(purge_standby_all(verbose)?);
        }
    }

    // Each operation already settles internally, so just capture final state
    let overall_after = MemorySnapshot::capture()?;
    let total_freed =
        overall_after.available_physical as i64 - overall_before.available_physical as i64;

    // Print summary
    println!();
    print_clean_summary(&results, &overall_before, &overall_after, total_freed);

    Ok(results)
}

/// Print a formatted summary of cleaning results.
fn print_clean_summary(
    results: &[CleanResult],
    before: &MemorySnapshot,
    after: &MemorySnapshot,
    total_freed: i64,
) {
    println!("{}", "─── Cleaning Summary ───────────────────".dimmed());
    println!();

    for r in results {
        let status = if r.success {
            "✓".green().bold().to_string()
        } else {
            "✗".red().bold().to_string()
        };
        let freed_str = if r.freed_bytes > 0 {
            format!("+{}", format_bytes(r.freed_bytes as u64))
                .green()
                .to_string()
        } else if r.freed_bytes < 0 {
            format!("-{}", format_bytes((-r.freed_bytes) as u64))
                .yellow()
                .to_string()
        } else {
            "0 B".dimmed().to_string()
        };

        println!(
            "  {} {:<35} {:>12}   {}",
            status,
            r.operation,
            freed_str,
            r.message.dimmed()
        );
    }

    println!();
    println!("{}", "─── Memory Before/After ────────────────".dimmed());
    println!(
        "  {} Used:      {} → {}",
        "▸".cyan(),
        format_bytes(before.used_physical).red(),
        format_bytes(after.used_physical).green()
    );
    println!(
        "  {} Available: {} → {}",
        "▸".cyan(),
        format_bytes(before.available_physical).yellow(),
        format_bytes(after.available_physical).green()
    );
    println!(
        "  {} Load:      {}% → {}%",
        "▸".cyan(),
        before.memory_load_percent.to_string().red(),
        after.memory_load_percent.to_string().green()
    );

    if total_freed > 0 {
        println!(
            "\n  {} Total freed: {}",
            "★".yellow().bold(),
            format_bytes(total_freed as u64).green().bold()
        );
    } else {
        println!(
            "\n  {} Net change: {} (already clean or pages re-faulted)",
            "•".dimmed(),
            format_bytes(total_freed.unsigned_abs()).yellow()
        );
    }
    println!();
}
