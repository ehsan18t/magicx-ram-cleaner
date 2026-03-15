//! # `MagicX` RAM Cleaner — Monitoring Mode
//!
//! Continuous monitoring with optional auto-clean when memory usage
//! exceeds a configurable threshold. Uses Win32 `SetConsoleCtrlHandler`
//! for graceful Ctrl+C shutdown.
//!
//! ## Why global state is required
//!
//! Win32's `SetConsoleCtrlHandler` requires an `extern "system"` callback,
//! which cannot capture any state (no closures, no `Arc`, no context pointer).
//! Therefore the shutdown flag (`RUNNING`) **must** be a `static AtomicBool`.
//! This is an inherent Win32 API limitation, not a design choice.
//!
//! `MONITOR_ACTIVE` is a separate guard that prevents concurrent calls to
//! `run_monitor` — since all state is global, running two monitor loops
//! simultaneously would produce undefined behaviour (both polling the same
//! `RUNNING` flag, both registering the same ctrl handler). The guard is
//! enforced via an RAII `MonitorGuard` that clears the flag on drop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

use crate::cleaner::{self, CleanLevel};
use crate::display;
use crate::stats::MemorySnapshot;

/// Maximum consecutive auto-clean errors before the monitor aborts.
/// Prevents infinite error-clean-error loops on a malfunctioning system.
const MAX_CONSECUTIVE_ERRORS: u32 = 3;

/// Global shutdown flag set to `false` by the console control handler.
///
/// Must be `static` because Win32 `SetConsoleCtrlHandler` callbacks are
/// `extern "system"` functions that cannot capture any state.
static RUNNING: AtomicBool = AtomicBool::new(true);

/// Guard preventing concurrent [`run_monitor`] calls.
///
/// Since `RUNNING` is process-global and the ctrl handler is process-wide,
/// running two monitor loops simultaneously would corrupt shared state.
/// This flag is checked at entry and cleared on exit via [`MonitorGuard`].
static MONITOR_ACTIVE: AtomicBool = AtomicBool::new(false);

/// RAII guard that clears `MONITOR_ACTIVE` when the monitor exits.
///
/// Ensures the flag is always reset, even if [`run_monitor`] returns early
/// via `?` or an error path. Without this, a failed monitor run would
/// permanently block future monitor calls for the process lifetime.
struct MonitorGuard;

impl Drop for MonitorGuard {
    fn drop(&mut self) {
        MONITOR_ACTIVE.store(false, Ordering::Release);
    }
}

/// Win32 console control handler callback registered via `SetConsoleCtrlHandler`.
///
/// Handles `CTRL_C_EVENT` (0), `CTRL_BREAK_EVENT` (1), and `CTRL_CLOSE_EVENT` (2)
/// by setting the `RUNNING` flag to `false` for graceful loop termination.
unsafe extern "system" fn ctrl_handler(ctrl_type: u32) -> i32 {
    // CTRL_C_EVENT = 0, CTRL_BREAK_EVENT = 1, CTRL_CLOSE_EVENT = 2
    if ctrl_type <= 2 {
        RUNNING.store(false, Ordering::Release);
        1 // TRUE — handled, prevent default process termination
    } else {
        0 // FALSE — not handled, pass to next handler
    }
}

/// Run the monitoring loop.
///
/// # Arguments
///
/// * `interval_secs` — Seconds between status checks.
/// * `threshold` — Optional memory load percentage (0–100) that triggers auto-clean.
/// * `auto_level` — The cleaning level to use when auto-cleaning.
/// * `cooldown_secs` — Optional override for cooldown seconds after an auto-clean.
///   Defaults to `2 × interval_secs` if `None`.
/// * `verbose` — Show detailed output during auto-clean.
///
/// # Errors
///
/// Returns an error if a monitor loop is already running in this process
/// (concurrent calls are prevented by the `MONITOR_ACTIVE` guard), or if
/// the console ctrl handler cannot be installed.
pub fn run_monitor(
    interval_secs: u64,
    threshold: Option<u32>,
    auto_level: CleanLevel,
    cooldown_secs: Option<u64>,
    verbose: bool,
) -> Result<()> {
    // Prevent concurrent monitor loops — all state is global (see module docs).
    if MONITOR_ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        anyhow::bail!(
            "A monitor loop is already running in this process. \
             Only one monitor instance can run at a time because Win32 \
             SetConsoleCtrlHandler state is process-global."
        );
    }
    // RAII guard: clears MONITOR_ACTIVE on all exit paths (success, error, panic)
    let _guard = MonitorGuard;

    // Reset the RUNNING flag in case run_monitor is called more than once
    RUNNING.store(true, Ordering::Release);

    // Install Ctrl+C handler for graceful shutdown
    // SAFETY: ctrl_handler is a valid extern "system" fn with the correct signature.
    let handler_ok = unsafe { SetConsoleCtrlHandler(Some(ctrl_handler), 1) };
    anyhow::ensure!(
        handler_ok != 0,
        "SetConsoleCtrlHandler failed — cannot guarantee graceful shutdown"
    );

    // Cooldown: skip auto-clean after the last clean to avoid
    // repeated cleaning when memory stays above the threshold.
    let cooldown_val = cooldown_secs.unwrap_or_else(|| interval_secs.saturating_mul(2));
    let cooldown = std::time::Duration::from_secs(cooldown_val);

    println!("\n{} MagicX RAM Monitor started", "◉".green().bold());
    println!(
        "  Interval: {}s | Auto-clean: {} | Level: {} | Cooldown: {}s",
        interval_secs,
        threshold.map_or_else(|| "disabled".into(), |t| format!("{t}%")),
        auto_level.title_case_name(),
        cooldown_val,
    );
    println!("  Press Ctrl+C to stop.\n");

    let mut last_clean: Option<Instant> = None;
    let mut consecutive_errors: u32 = 0;

    while RUNNING.load(Ordering::Acquire) {
        let iteration_start = Instant::now();
        let snapshot = MemorySnapshot::capture()?;
        display::print_compact_status(&snapshot);

        // Auto-clean if threshold exceeded (let chain — stable since Rust 1.88)
        if let Some(thresh) = threshold
            && snapshot.memory_load_percent >= thresh
        {
            handle_threshold_clean(
                thresh,
                &snapshot,
                auto_level,
                verbose,
                cooldown,
                &mut last_clean,
                &mut consecutive_errors,
            )?;
        }

        // Sleep in small increments so Ctrl+C is responsive.
        // Subtract the time already spent on this iteration (status capture +
        // potential cleaning) so the effective check interval stays consistent.
        let interval = std::time::Duration::from_secs(interval_secs);
        let remaining = interval.saturating_sub(iteration_start.elapsed());
        let ticks = remaining.as_millis() / 100;
        for _ in 0..ticks {
            if !RUNNING.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    println!("\n{} Monitor stopped.", "◉".red().bold());
    Ok(())
}

/// Handle threshold-triggered auto-cleaning for a single monitor iteration.
///
/// Checks whether the cooldown period has elapsed since the last clean. If
/// cooldown is active, prints a skip message. Otherwise executes
/// [`cleaner::smart_clean`] and tracks consecutive errors, aborting the
/// monitor after `max_errors` consecutive failures.
fn handle_threshold_clean(
    thresh: u32,
    snapshot: &MemorySnapshot,
    auto_level: CleanLevel,
    verbose: bool,
    cooldown: std::time::Duration,
    last_clean: &mut Option<Instant>,
    consecutive_errors: &mut u32,
) -> Result<()> {
    let in_cooldown = last_clean.is_some_and(|t| t.elapsed() < cooldown);

    if in_cooldown {
        println!(
            "  {} Memory {}% >= {}% but cooldown active — skipping",
            "⏳".yellow(),
            snapshot.memory_load_percent,
            thresh
        );
        return Ok(());
    }

    println!(
        "\n  {} Memory load {}% >= threshold {}% — auto-cleaning...",
        "⚠".yellow().bold(),
        snapshot.memory_load_percent,
        thresh
    );
    *last_clean = Some(Instant::now());
    display::print_clean_start(auto_level);

    match cleaner::smart_clean(auto_level, verbose, &[]) {
        Ok(output) => {
            // Reset error streak on any successful execution
            *consecutive_errors = 0;
            display::print_clean_summary(
                &output.results,
                &output.overall_before,
                &output.overall_after,
                output.total_freed,
                output.total_elapsed_secs,
            );
            let failures: Vec<_> = output.results.iter().filter(|r| !r.success).collect();
            if !failures.is_empty() {
                eprintln!(
                    "  {} Auto-clean completed with {} failed operation(s)",
                    "⚠".yellow(),
                    failures.len()
                );
            }
        }
        Err(e) => {
            *consecutive_errors += 1;
            eprintln!("  {} Auto-clean error: {}", "✗".red().bold(), e);
            if *consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                anyhow::bail!(
                    "Monitor aborted: {MAX_CONSECUTIVE_ERRORS} consecutive \
                     auto-clean failures. Last error: {e}"
                );
            }
            eprintln!(
                "  {} ({}/{MAX_CONSECUTIVE_ERRORS} consecutive failures before abort)",
                "⚠".yellow(),
                *consecutive_errors,
            );
        }
    }

    Ok(())
}
