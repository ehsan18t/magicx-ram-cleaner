// ─── Compiler-enforced quality gates ─────────────────────────────────────────
// These cannot be overridden by individual modules. Any violation = build failure.
#![deny(
    // Correctness
    unused_must_use,         // ignoring Result/Option is a bug
    unreachable_patterns,    // dead match arms = confusion
    // Safety — unsafe is denied globally; modules that need FFI get #[allow(unsafe_code)]
    unsafe_code,
    unsafe_op_in_unsafe_fn,  // unsafe blocks inside unsafe fn must be explicit
    // Quality
    unused_imports,          // dead imports = sloppy code
    unused_variables,        // unused vars = incomplete work
    dead_code,               // dead code = maintenance burden
    // Documentation
    rustdoc::broken_intra_doc_links,
)]

//! # `MagicX` RAM Cleaner
//!
//! The most powerful Windows RAM cleaner CLI tool.
//! Surpasses `EmptyStandbyList` with granular control over every memory subsystem.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    MagicX RAM Cleaner                       │
//! ├──────────┬──────────────┬──────────────┬───────────────────┤
//! │   CLI    │  Cleaner     │  Monitor     │  Display          │
//! │ (cli.rs) │  Engine      │  Loop        │  Formatting       │
//! ├──────────┤              │              │                   │
//! │ Console  │              │              │                   │
//! │ (ANSI)   │              │              │                   │
//! ├──────────┴──────────────┴──────────────┴───────────────────┤
//! │              NT Native API Bindings (ntapi)                │
//! │     NtSetSystemInformation / NtQuerySystemInformation      │
//! ├────────────────────────────────────────────────────────────┤
//! │              Win32 API  (windows-sys)                       │
//! │  GlobalMemoryStatusEx, SetSystemFileCacheSize,             │
//! │  K32EmptyWorkingSet, SetProcessWorkingSetSizeEx            │
//! ├────────────────────────────────────────────────────────────┤
//! │              Privilege Manager                              │
//! │  SeProfileSingleProcessPrivilege, SeIncreaseQuotaPrivilege │
//! └────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Why `MagicX` is better than `EmptyStandbyList`
//!
//! | Feature | EmptyStandbyList | MagicX |
//! |---|---|---|
//! | Standby list purge | ✓ | ✓ |
//! | Low-priority only purge | ✓ | ✓ |
//! | Working set empty | ✓ | ✓ (kernel-level AND per-process) |
//! | Modified list flush | ✓ | ✓ |
//! | File cache flush | ✗ | ✓ |
//! | Memory combining/dedup | ✗ | ✓ |
//! | Smart multi-step cleaning | ✗ | ✓ (4 levels) |
//! | Before/after reporting | ✗ | ✓ |
//! | Detailed memory list stats | ✗ | ✓ (per-priority breakdown) |
//! | Monitoring with auto-clean | ✗ | ✓ |
//! | JSON output | ✗ | ✓ |
//! | Optimal operation ordering | ✗ | ✓ |
//! | Second-pass cleaning | ✗ | ✓ |

// Modules that legitimately need unsafe get a scoped allow
#[allow(unsafe_code)]
mod cleaner;
mod cli;
#[allow(unsafe_code)]
mod console;
#[allow(unsafe_code)]
mod display;
#[allow(unsafe_code)]
mod monitor;
#[allow(unsafe_code)]
mod ntapi;
#[allow(unsafe_code)]
mod privilege;
#[allow(unsafe_code)]
mod stats;

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{ColorChoice, CommandFactory, FromArgMatches};
use colored::Colorize;

use cli::{Cli, Commands};

/// Entry point — returns [`ExitCode`] instead of calling `std::process::exit()`.
///
/// Exit codes:
/// - `0` — all operations succeeded
/// - `1` — one or more cleaning operations failed (already reported)
/// - `2` — fatal error (printed to stderr)
fn main() -> ExitCode {
    // Detect --no-color / NO_COLOR BEFORE anything else so that all output
    // (including clap help text and the banner) respects the preference.
    let no_color = detect_no_color();
    if no_color {
        colored::control::set_override(false);
    } else {
        console::enable_ansi_colors();
    }

    let standalone = console::is_standalone_console();

    let result = run(no_color);

    // If launched by double-clicking the .exe, pause so the user can read the
    // output before the console window closes.
    if standalone {
        console::pause_before_exit();
    }

    match result {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{} {e:?}", "Error:".red().bold());
            ExitCode::from(2)
        }
    }
}

/// Core application logic.
///
/// Returns `Ok(true)` if any cleaning operation reported a failure,
/// `Ok(false)` on full success, or `Err` on fatal errors.
///
/// `no_color` is pre-computed by [`detect_no_color`] before this function
/// is called, so that clap help text rendering also strips ANSI codes.
fn run(no_color: bool) -> Result<bool> {
    let cli = parse_cli(no_color)?;

    let quiet = cli.quiet;

    let Some(ref command) = cli.command else {
        if !quiet {
            display::print_banner();
        }
        print_no_command_help()?;
        return Ok(false);
    };

    // Suppress banner when JSON output is requested so stdout stays machine-parseable
    let is_json = matches!(command, Commands::Status { json: true, .. });
    if !quiet && !is_json {
        display::print_banner();
    }

    // Check for admin privileges and enable required security tokens
    privilege::check_admin()?;
    privilege::enable_all_privileges().context(
        "Failed to enable privileges. Make sure you're running as Administrator.\n\
         Right-click the terminal/exe → 'Run as administrator'",
    )?;

    dispatch_command(command, quiet)
}

/// Show a friendly help guide when no subcommand is given.
fn print_no_command_help() -> Result<()> {
    println!(
        "  {}\n",
        "No command specified. Showing usage guide.".yellow()
    );
    Cli::command()
        .print_long_help()
        .context("Failed to print help")?;
    println!();
    Ok(())
}

/// Pre-scan `argv` and environment for colour suppression requests.
///
/// This runs BEFORE [`Cli::parse`] so that clap's `--help` rendering also
/// respects the preference. Clap processes `--help` internally and exits
/// during parsing, so checking `cli.no_color` after `parse()` would be
/// too late for help text.
///
/// Checks two sources (matching the `--no-color` doc comment contract):
/// - `--no-color` flag anywhere in `argv`
/// - `NO_COLOR` environment variable (any value, per <https://no-color.org/>)
fn detect_no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some() || std::env::args().any(|a| a == "--no-color")
}

/// Parse CLI arguments with colour support applied.
///
/// When `no_color` is `true`, sets [`ColorChoice::Never`] on the clap
/// [`Command`](clap::Command) so it strips all ANSI escape codes from
/// help text (`long_about`, `after_help`, etc.) before rendering.
fn parse_cli(no_color: bool) -> Result<Cli> {
    let mut cmd = Cli::command();
    if no_color {
        cmd = cmd.color(ColorChoice::Never);
    }
    let matches = cmd.get_matches();
    Ok(Cli::from_arg_matches(&matches)?)
}

/// Print a single operation's result and return `true` if it failed.
fn report_single(result: &cleaner::CleanResult) -> bool {
    display::print_single_result(result);
    !result.success
}

/// Dispatch the parsed CLI command. Returns `true` if any operation failed.
///
/// When `quiet` is `true`, the banner is already suppressed and verbose progress
/// is forced off. Only results, errors, and machine-readable data are printed.
fn dispatch_command(command: &Commands, quiet: bool) -> Result<bool> {
    let mut had_failure = false;

    match command {
        Commands::Clean {
            level,
            verbose,
            report,
            dry_run,
            exclude,
        } => {
            if *dry_run {
                let plan = cleaner::dry_run_plan(*level, !exclude.is_empty());
                display::print_dry_run(*level, &plan);
            } else {
                let effective_verbose = *verbose && !quiet;
                if !quiet {
                    display::print_clean_start(*level);
                }
                let output = cleaner::smart_clean(*level, effective_verbose, exclude)?;
                display::print_clean_summary(
                    &output.results,
                    &output.overall_before,
                    &output.overall_after,
                    output.total_freed,
                    output.total_elapsed_secs,
                );
                if let Some(path) = report {
                    write_report(path, &output)?;
                }
                had_failure = output.results.iter().any(|r| !r.success);
            }
        }

        Commands::Status {
            detailed,
            json,
            top,
        } => {
            dispatch_status(*detailed, *json, *top)?;
        }

        Commands::PurgeStandby {
            low_priority,
            verbose,
        } => {
            let effective_verbose = *verbose && !quiet;
            let result = if *low_priority {
                cleaner::purge_standby_low_priority(effective_verbose)?
            } else {
                cleaner::purge_standby_all(effective_verbose)?
            };
            had_failure = report_single(&result);
        }

        Commands::FlushModified { verbose } => {
            let effective_verbose = *verbose && !quiet;
            had_failure = report_single(&cleaner::flush_modified_list(effective_verbose)?);
        }

        Commands::EmptyWorkingsets {
            per_process,
            exclude,
            verbose,
        } => {
            let effective_verbose = *verbose && !quiet;
            // --exclude implies per-process mode (kernel-level cannot exclude)
            let result = if *per_process || !exclude.is_empty() {
                cleaner::empty_working_sets_per_process(effective_verbose, exclude)?
            } else {
                cleaner::empty_working_sets_kernel(effective_verbose)?
            };
            had_failure = report_single(&result);
        }

        Commands::FlushCache { verbose } => {
            let effective_verbose = *verbose && !quiet;
            had_failure = report_single(&cleaner::flush_file_cache(effective_verbose)?);
        }

        Commands::FlushRegistry { verbose } => {
            let effective_verbose = *verbose && !quiet;
            had_failure = report_single(&cleaner::flush_registry_cache(effective_verbose)?);
        }

        Commands::Combine { verbose } => {
            let effective_verbose = *verbose && !quiet;
            had_failure = report_single(&cleaner::combine_memory(effective_verbose)?);
        }

        Commands::Monitor {
            interval,
            threshold,
            level,
            cooldown,
            verbose,
        } => {
            let effective_verbose = *verbose && !quiet;
            monitor::run_monitor(*interval, *threshold, *level, *cooldown, effective_verbose)?;
        }
    }

    Ok(had_failure)
}

/// Handle the `status` subcommand — capture and display memory information.
fn dispatch_status(detailed: bool, json: bool, top: Option<usize>) -> Result<()> {
    let snapshot = stats::MemorySnapshot::capture()?;
    let list_info = if detailed {
        match stats::MemoryListInfo::query() {
            Ok(info) => Some(info),
            Err(e) => {
                eprintln!(
                    "{} Could not query memory list details: {}",
                    "warning:".yellow(),
                    e
                );
                None
            }
        }
    } else {
        None
    };

    let file_cache = if detailed {
        match stats::FileCacheSnapshot::capture() {
            Ok(fc) => Some(fc),
            Err(e) => {
                eprintln!(
                    "{} Could not query file cache info: {}",
                    "warning:".yellow(),
                    e
                );
                None
            }
        }
    } else {
        None
    };

    let top_processes = top.and_then(|count| {
        let count = if count == 0 { 10 } else { count };
        match stats::query_top_processes(count) {
            Ok(procs) => Some(procs),
            Err(e) => {
                eprintln!(
                    "{} Could not query process memory info: {}",
                    "warning:".yellow(),
                    e
                );
                None
            }
        }
    });

    if json {
        let output = serde_json::json!({
            "snapshot": snapshot,
            "memory_lists": list_info,
            "file_cache": file_cache,
            "top_processes": top_processes,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        display::print_status(&snapshot, list_info.as_ref(), file_cache.as_ref());
        if let Some(ref procs) = top_processes {
            display::print_top_processes(procs);
        }
    }
    Ok(())
}

/// Write a cleaning report to a JSON file.
fn write_report(path: &str, output: &cleaner::SmartCleanResult) -> Result<()> {
    let json = serde_json::to_string_pretty(output).context("Failed to serialize report")?;
    std::fs::write(path, &json).with_context(|| format!("Failed to write report to '{path}'"))?;
    println!(
        "  {} Report written to {}",
        "📄".dimmed(),
        path.cyan().bold()
    );
    Ok(())
}
