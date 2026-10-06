//! # `MagicX` RAM Cleaner - Monitoring Mode
//!
//! Continuous monitoring with optional auto-clean when memory usage
//! exceeds a configurable threshold, stopped gracefully with Ctrl+C (see
//! [`console::watch_interrupts`]).
//!
//! The interrupt flag is process-global (a Win32 console handler cannot
//! capture state), so `MONITOR_ACTIVE` guards against two monitor loops
//! running at once and sharing it. The guard is released by an RAII
//! `MonitorGuard` on every exit path.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::Result;
use colored::Colorize;

use super::display;
use crate::engine::auto_clean::{self, AutoCleanPolicy, Decision};
use crate::engine::{self, CleanLevel};
use crate::memory::MemorySnapshot;
use crate::platform::console;
use crate::strings;

/// Maximum consecutive auto-clean errors before the monitor aborts.
/// Prevents infinite error-clean-error loops on a malfunctioning system.
const MAX_CONSECUTIVE_ERRORS: u32 = 3;

/// Guard preventing concurrent [`run_monitor`] calls.
///
/// The interrupt flag and console handler are process-wide, so running two
/// monitor loops simultaneously would corrupt shared state.
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

/// Run the monitoring loop.
///
/// # Arguments
///
/// * `interval_secs` - Seconds between status checks.
/// * `threshold` - Optional memory load percentage (0–100) that triggers auto-clean.
/// * `auto_level` - The cleaning level to use when auto-cleaning.
/// * `cooldown_secs` - Optional override for cooldown seconds after an auto-clean.
///   Defaults to `2 × interval_secs` if `None`.
/// * `verbose` - Show detailed output during auto-clean.
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
    // Prevent concurrent monitor loops - all state is global (see module docs).
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

    // Ctrl+C / Ctrl+Break end the loop gracefully instead of killing the process.
    console::watch_interrupts()?;

    // Cooldown: skip auto-clean after the last clean to avoid
    // repeated cleaning when memory stays above the threshold.
    let cooldown_val = cooldown_secs.unwrap_or_else(|| interval_secs.saturating_mul(2));
    let cooldown = std::time::Duration::from_secs(cooldown_val);

    println!(
        "\n{} {}",
        "◉".green().bold(),
        strings::cli::monitor::STARTED
    );
    println!(
        "  Interval: {}s | Auto-clean: {} | Level: {} | Cooldown: {}s",
        interval_secs,
        threshold.map_or_else(|| "disabled".into(), |t| format!("{t}%")),
        auto_level.title_case_name(),
        cooldown_val,
    );
    println!("  {}\n", strings::cli::monitor::CTRL_C_HINT);

    let mut state = MonitorState {
        policy: threshold.map(|t| AutoCleanPolicy::new(t, cooldown)),
        snapshot_errors: 0,
        clean_errors: 0,
    };
    let interval = std::time::Duration::from_secs(interval_secs);

    while !console::interrupted() {
        let iteration_start = Instant::now();

        // A transient query failure should not end a long-running monitor;
        // only an unbroken run of them does.
        match MemorySnapshot::capture() {
            Ok(snapshot) => {
                state.snapshot_errors = 0;
                display::print_compact_status(&snapshot);
                if let Some(thresh) = threshold {
                    handle_threshold_clean(thresh, &snapshot, auto_level, verbose, &mut state)?;
                }
            }
            Err(e) => record_error(&mut state.snapshot_errors, &e)?,
        }

        sleep_until(iteration_start + interval);
    }

    println!("\n{} {}", "◉".red().bold(), strings::cli::monitor::STOPPED);
    Ok(())
}

/// Sleep until `deadline` in small increments so Ctrl+C stays responsive.
///
/// Measuring against a deadline (rather than counting whole ticks) keeps the
/// check interval exact, including the time spent capturing and cleaning.
fn sleep_until(deadline: Instant) {
    const TICK: std::time::Duration = std::time::Duration::from_millis(100);
    while !console::interrupted() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        std::thread::sleep(remaining.min(TICK));
    }
}

/// Mutable bookkeeping carried across monitor iterations.
#[derive(Debug)]
struct MonitorState {
    /// Auto-clean timing rules; `None` when auto-clean is disabled.
    policy: Option<AutoCleanPolicy>,
    /// Consecutive failed status queries (reset by a successful query).
    snapshot_errors: u32,
    /// Consecutive failed cleans (reset by a successful clean). Kept apart
    /// from `snapshot_errors` so the good status queries between cleans
    /// cannot mask a clean that keeps failing.
    clean_errors: u32,
}

/// Count an error in `streak` and abort the monitor once
/// [`MAX_CONSECUTIVE_ERRORS`] happen in a row.
fn record_error(streak: &mut u32, error: &anyhow::Error) -> Result<()> {
    *streak += 1;
    eprintln!("  {} Monitor error: {error}", "✗".red().bold());
    if *streak >= MAX_CONSECUTIVE_ERRORS {
        anyhow::bail!(
            "Monitor aborted: {MAX_CONSECUTIVE_ERRORS} consecutive failures. \
                 Last error: {error}"
        );
    }
    eprintln!(
        "  {} ({}/{MAX_CONSECUTIVE_ERRORS} consecutive failures before abort)",
        "⚠".yellow(),
        *streak,
    );
    Ok(())
}

/// Apply the auto-clean policy for a single monitor iteration.
///
/// Prints a skip message while the (backed-off) cooldown is active, and
/// otherwise runs [`engine::Cleaner::smart_clean`], tracking consecutive
/// errors and aborting the monitor after [`MAX_CONSECUTIVE_ERRORS`].
fn handle_threshold_clean(
    thresh: u32,
    snapshot: &MemorySnapshot,
    auto_level: CleanLevel,
    verbose: bool,
    state: &mut MonitorState,
) -> Result<()> {
    let Some(policy) = state.policy.as_mut() else {
        return Ok(());
    };
    let load = snapshot.memory_load_percent;
    match policy.decide(load, Instant::now()) {
        Decision::BelowThreshold => return Ok(()),
        Decision::CoolingDown => {
            println!(
                "  {} Memory {load}% >= {thresh}% but cooldown active - skipping",
                "⏳".yellow(),
            );
            return Ok(());
        }
        Decision::Clean => {}
    }

    println!(
        "
  {} Memory load {load}% >= threshold {thresh}% - auto-cleaning...",
        "⚠".yellow().bold(),
    );
    display::print_clean_start(auto_level);

    let outcome = engine::Cleaner::new(&engine::WindowsMemory, display::progress_printer(verbose))
        .smart_clean(auto_level, &[]);
    // The cooldown runs from when the clean finished.
    let still_high = policy.record_clean(Instant::now(), auto_clean::load_after(&outcome));
    let next_in = policy.effective_cooldown();

    match outcome {
        Ok(output) => {
            // Reset error streak on any successful execution
            state.clean_errors = 0;
            display::print_clean_summary(&output);
        }
        Err(e) => record_error(&mut state.clean_errors, &e)?,
    }
    if still_high {
        println!(
            "  {} Load is still above the threshold; next auto-clean in {}s at the earliest",
            "⏳".yellow(),
            next_in.as_secs()
        );
    }

    Ok(())
}
