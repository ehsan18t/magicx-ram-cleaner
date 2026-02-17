//! # MagicX RAM Cleaner — Monitoring Mode
//!
//! Continuous monitoring with optional auto-clean when memory usage
//! exceeds a configurable threshold. Uses Win32 `SetConsoleCtrlHandler`
//! for graceful Ctrl+C shutdown.

use std::sync::atomic::{AtomicBool, Ordering};

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
/// Handles CTRL_C_EVENT (0), CTRL_BREAK_EVENT (1), and CTRL_CLOSE_EVENT (2)
/// by setting the `RUNNING` flag to `false` for graceful loop termination.
unsafe extern "system" fn ctrl_handler(ctrl_type: u32) -> i32 {
    // CTRL_C_EVENT = 0, CTRL_BREAK_EVENT = 1, CTRL_CLOSE_EVENT = 2
    if ctrl_type <= 2 {
        RUNNING.store(false, Ordering::SeqCst);
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
    anyhow::ensure!(interval_secs > 0, "Monitor interval must be at least 1 second");

    // Reset the RUNNING flag in case run_monitor is called more than once
    RUNNING.store(true, Ordering::SeqCst);

    // Install Ctrl+C handler for graceful shutdown
    unsafe { SetConsoleCtrlHandler(Some(ctrl_handler), 1) };

    println!("\n{} MagicX RAM Monitor started", "◉".green().bold());
    println!(
        "  Interval: {}s | Auto-clean: {} | Level: {}",
        interval_secs,
        threshold
            .map(|t| format!("{}%", t))
            .unwrap_or_else(|| "disabled".into()),
        auto_level,
    );
    println!("  Press Ctrl+C to stop.\n");

    while RUNNING.load(Ordering::SeqCst) {
        let snapshot = MemorySnapshot::capture()?;
        display::print_compact_status(&snapshot);

        // Auto-clean if threshold exceeded (let chain — stable since Rust 1.88)
        if let Some(thresh) = threshold
            && snapshot.memory_load_percent >= thresh
        {
            println!(
                "\n  {} Memory load {}% >= threshold {}% — auto-cleaning...",
                "⚠".yellow().bold(),
                snapshot.memory_load_percent,
                thresh
            );
            let _ = cleaner::smart_clean(auto_level, verbose);
        }

        // Sleep in small increments so Ctrl+C is responsive
        for _ in 0..interval_secs * 10 {
            if !RUNNING.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    println!("\n{} Monitor stopped.", "◉".red());
    Ok(())
}
