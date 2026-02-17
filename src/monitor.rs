//! # `MagicX` RAM Cleaner — Monitoring Mode
//!
//! Continuous monitoring with optional auto-clean when memory usage
//! exceeds a configurable threshold. Uses Win32 `SetConsoleCtrlHandler`
//! for graceful Ctrl+C shutdown.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

use crate::cleaner::{self, CleanLevel};
use crate::display;
use crate::stats::MemorySnapshot;

/// Global flag set to `false` by the console control handler to signal shutdown.
static RUNNING: AtomicBool = AtomicBool::new(true);

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
/// * `verbose` — Show detailed output during auto-clean.
pub fn run_monitor(
    interval_secs: u64,
    threshold: Option<u32>,
    auto_level: CleanLevel,
    verbose: bool,
) -> Result<()> {
    // Validate interval to prevent spin-loop
    anyhow::ensure!(
        interval_secs > 0,
        "Monitor interval must be at least 1 second"
    );

    // Reset the RUNNING flag in case run_monitor is called more than once
    RUNNING.store(true, Ordering::Release);

    // Install Ctrl+C handler for graceful shutdown
    // SAFETY: ctrl_handler is a valid extern "system" fn with the correct signature.
    unsafe { SetConsoleCtrlHandler(Some(ctrl_handler), 1) };

    println!("\n{} MagicX RAM Monitor started", "◉".green().bold());
    println!(
        "  Interval: {}s | Auto-clean: {} | Level: {}",
        interval_secs,
        threshold.map_or_else(|| "disabled".into(), |t| format!("{t}%")),
        auto_level,
    );
    println!("  Press Ctrl+C to stop.\n");

    // Cooldown: skip auto-clean for 2× interval after the last clean to avoid
    // repeated cleaning when memory stays above the threshold.
    let cooldown = std::time::Duration::from_secs(interval_secs * 2);
    let mut last_clean: Option<Instant> = None;

    while RUNNING.load(Ordering::Acquire) {
        let snapshot = MemorySnapshot::capture()?;
        display::print_compact_status(&snapshot);

        // Auto-clean if threshold exceeded (let chain — stable since Rust 1.88)
        if let Some(thresh) = threshold
            && snapshot.memory_load_percent >= thresh
        {
            let in_cooldown = last_clean.is_some_and(|t| t.elapsed() < cooldown);

            if in_cooldown {
                println!(
                    "  {} Memory {}% >= {}% but cooldown active — skipping",
                    "⏳".yellow(),
                    snapshot.memory_load_percent,
                    thresh
                );
            } else {
                println!(
                    "\n  {} Memory load {}% >= threshold {}% — auto-cleaning...",
                    "⚠".yellow().bold(),
                    snapshot.memory_load_percent,
                    thresh
                );
                last_clean = Some(Instant::now());
                display::print_clean_start(auto_level);
                match cleaner::smart_clean(auto_level, verbose) {
                    Ok(output) => {
                        display::print_clean_summary(
                            &output.results,
                            &output.overall_before,
                            &output.overall_after,
                            output.total_freed,
                        );
                        let failures: Vec<_> =
                            output.results.iter().filter(|r| !r.success).collect();
                        if !failures.is_empty() {
                            eprintln!(
                                "  {} Auto-clean completed with {} failed operation(s)",
                                "⚠".yellow(),
                                failures.len()
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("  {} Auto-clean error: {}", "✗".red().bold(), e);
                    }
                }
            }
        }

        // Sleep in small increments so Ctrl+C is responsive
        for _ in 0..interval_secs * 10 {
            if !RUNNING.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    println!("\n{} Monitor stopped.", "◉".red());
    Ok(())
}
