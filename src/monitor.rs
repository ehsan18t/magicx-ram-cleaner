//! # `MagicX` RAM Cleaner - Monitoring Mode
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
//! `run_monitor` - since all state is global, running two monitor loops
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
use crate::strings;

/// Maximum consecutive auto-clean errors before the monitor aborts.
/// Prevents infinite error-clean-error loops on a malfunctioning system.
const MAX_CONSECUTIVE_ERRORS: u32 = 3;

/// Upper bound for the cooldown backoff multiplier (see [`AutoCleanState`]).
const MAX_COOLDOWN_MULTIPLIER: u32 = 8;

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
/// For `CTRL_CLOSE_EVENT` Windows terminates the process shortly after the
/// handler returns, so only Ctrl+C / Ctrl+Break reach the "stopped" message.
unsafe extern "system" fn ctrl_handler(ctrl_type: u32) -> i32 {
    // CTRL_C_EVENT = 0, CTRL_BREAK_EVENT = 1, CTRL_CLOSE_EVENT = 2
    if ctrl_type <= 2 {
        RUNNING.store(false, Ordering::Release);
        1 // TRUE - handled, prevent default process termination
    } else {
        0 // FALSE - not handled, pass to next handler
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

    // Reset the RUNNING flag in case run_monitor is called more than once
    RUNNING.store(true, Ordering::Release);

    // Install Ctrl+C handler for graceful shutdown
    // SAFETY: ctrl_handler is a valid extern "system" fn with the correct signature.
    let handler_ok = unsafe { SetConsoleCtrlHandler(Some(ctrl_handler), 1) };
    anyhow::ensure!(
        handler_ok != 0,
        "SetConsoleCtrlHandler failed - cannot guarantee graceful shutdown"
    );

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

    let mut state = AutoCleanState::default();
    let interval = std::time::Duration::from_secs(interval_secs);

    while RUNNING.load(Ordering::Acquire) {
        let iteration_start = Instant::now();

        // A transient query failure should not end a long-running monitor;
        // only an unbroken run of them does.
        match MemorySnapshot::capture() {
            Ok(snapshot) => {
                state.snapshot_errors = 0;
                display::print_compact_status(&snapshot);
                if let Some(thresh) = threshold {
                    if snapshot.memory_load_percent >= thresh {
                        handle_threshold_clean(
                            thresh, &snapshot, auto_level, verbose, cooldown, &mut state,
                        )?;
                    } else {
                        state.cooldown_multiplier = 1;
                    }
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
    while RUNNING.load(Ordering::Acquire) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        std::thread::sleep(remaining.min(TICK));
    }
}

/// Mutable auto-clean bookkeeping carried across monitor iterations.
#[derive(Debug)]
struct AutoCleanState {
    /// When the last auto-clean finished.
    last_clean: Option<Instant>,
    /// Consecutive failed status queries (reset by a successful query).
    snapshot_errors: u32,
    /// Consecutive failed cleans (reset by a successful clean). Kept apart
    /// from `snapshot_errors` so the good status queries between cleans
    /// cannot mask a clean that keeps failing.
    clean_errors: u32,
    /// Cooldown multiplier: doubles (up to [`MAX_COOLDOWN_MULTIPLIER`]) after
    /// a clean that leaves memory load at or above the threshold, and resets
    /// to 1 once load drops below it. Stops futile back-to-back cleans when
    /// the load is held up by memory that cleaning cannot reclaim.
    cooldown_multiplier: u32,
}

impl Default for AutoCleanState {
    fn default() -> Self {
        Self {
            last_clean: None,
            snapshot_errors: 0,
            clean_errors: 0,
            cooldown_multiplier: 1,
        }
    }
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

/// Handle threshold-triggered auto-cleaning for a single monitor iteration.
///
/// Checks whether the (backed-off) cooldown has elapsed since the last clean
/// finished. If cooldown is active, prints a skip message. Otherwise executes
/// [`cleaner::smart_clean`] and tracks consecutive errors, aborting the
/// monitor after [`MAX_CONSECUTIVE_ERRORS`] consecutive failures.
fn handle_threshold_clean(
    thresh: u32,
    snapshot: &MemorySnapshot,
    auto_level: CleanLevel,
    verbose: bool,
    cooldown: std::time::Duration,
    state: &mut AutoCleanState,
) -> Result<()> {
    let effective_cooldown = cooldown.saturating_mul(state.cooldown_multiplier);
    let in_cooldown = state
        .last_clean
        .is_some_and(|t| t.elapsed() < effective_cooldown);

    if in_cooldown {
        println!(
            "  {} Memory {}% >= {}% but cooldown active - skipping",
            "⏳".yellow(),
            snapshot.memory_load_percent,
            thresh
        );
        return Ok(());
    }

    println!(
        "\n  {} Memory load {}% >= threshold {}% - auto-cleaning...",
        "⚠".yellow().bold(),
        snapshot.memory_load_percent,
        thresh
    );
    display::print_clean_start(auto_level);

    let outcome = cleaner::smart_clean(auto_level, verbose, &[]);
    // The cooldown runs from when the clean finished, so a clean longer than
    // the cooldown cannot be followed immediately by another one.
    state.last_clean = Some(Instant::now());

    match outcome {
        Ok(output) => {
            // Reset error streak on any successful execution
            state.clean_errors = 0;
            display::print_clean_summary(&output);
            if output.overall_after.memory_load_percent >= thresh {
                state.cooldown_multiplier =
                    (state.cooldown_multiplier * 2).min(MAX_COOLDOWN_MULTIPLIER);
                println!(
                    "  {} Load is still above the threshold; next auto-clean in {}s at the earliest",
                    "⏳".yellow(),
                    cooldown.saturating_mul(state.cooldown_multiplier).as_secs()
                );
            } else {
                state.cooldown_multiplier = 1;
            }
        }
        Err(e) => record_error(&mut state.clean_errors, &e)?,
    }

    Ok(())
}
