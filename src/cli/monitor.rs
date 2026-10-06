//! # `MagicX` RAM Cleaner - Monitoring Mode
//!
//! Continuous monitoring with optional auto-clean when memory usage
//! reaches a configurable threshold, stopped gracefully with Ctrl+C (see
//! [`console::watch_interrupts`]).

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
/// Returns whether any auto-clean had a failed operation, for the exit code.
///
/// # Errors
///
/// Returns an error if the console ctrl handler cannot be installed, or
/// after [`MAX_CONSECUTIVE_ERRORS`] failed status queries or cleans in a row.
pub fn run_monitor(
    interval_secs: u64,
    threshold: Option<u32>,
    auto_level: CleanLevel,
    cooldown_secs: Option<u64>,
    verbose: bool,
) -> Result<bool> {
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
        had_failure: false,
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
                handle_threshold_clean(&snapshot, auto_level, verbose, &mut state)?;
            }
            Err(e) => record_error(&mut state.snapshot_errors, &e)?,
        }

        sleep_until(iteration_start.checked_add(interval));
    }

    println!("\n{} {}", "◉".red().bold(), strings::cli::monitor::STOPPED);
    Ok(state.had_failure)
}

/// Sleep until `deadline` in small increments so Ctrl+C stays responsive.
/// `None` (a deadline past what `Instant` can hold) waits for Ctrl+C.
///
/// Measuring against a deadline (rather than counting whole ticks) keeps the
/// check interval exact, including the time spent capturing and cleaning.
fn sleep_until(deadline: Option<Instant>) {
    const TICK: std::time::Duration = std::time::Duration::from_millis(100);
    while !console::interrupted() {
        let remaining = deadline.map_or(TICK, |deadline| {
            deadline.saturating_duration_since(Instant::now())
        });
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
    /// Consecutive failed cleans (reset by a fully successful clean). Kept
    /// apart from `snapshot_errors` so the good status queries between
    /// cleans cannot mask a clean that keeps failing. A clean whose
    /// operations failed counts, not only one that could not run at all.
    clean_errors: u32,
    /// Whether any auto-clean so far had a failed operation (exit code 1).
    had_failure: bool,
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

/// Apply the auto-clean policy for a single monitor iteration. Does nothing
/// when auto-clean is off.
///
/// Prints a skip message while the (backed-off) cooldown is active, and
/// otherwise runs [`engine::Cleaner::smart_clean`], tracking consecutive
/// errors and aborting the monitor after [`MAX_CONSECUTIVE_ERRORS`].
fn handle_threshold_clean(
    snapshot: &MemorySnapshot,
    auto_level: CleanLevel,
    verbose: bool,
    state: &mut MonitorState,
) -> Result<()> {
    let Some(policy) = state.policy.as_mut() else {
        return Ok(());
    };
    let thresh = policy.threshold();
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
            display::print_clean_summary(&output);
            let failed = output.failed_count();
            if failed == 0 {
                state.clean_errors = 0;
            } else {
                state.had_failure = true;
                let error =
                    anyhow::anyhow!("{failed} of {} operations failed", output.results.len());
                record_error(&mut state.clean_errors, &error)?;
            }
        }
        Err(e) => {
            state.had_failure = true;
            record_error(&mut state.clean_errors, &e)?;
        }
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
