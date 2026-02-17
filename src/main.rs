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
use clap::{CommandFactory, Parser};
use colored::Colorize;

use cli::{Cli, Commands};

/// Entry point — returns [`ExitCode`] instead of calling `std::process::exit()`.
///
/// Exit codes:
/// - `0` — all operations succeeded
/// - `1` — one or more cleaning operations failed (already reported)
/// - `2` — fatal error (printed to stderr)
fn main() -> ExitCode {
    console::enable_ansi_colors();
    let standalone = console::is_standalone_console();

    let result = run();

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
fn run() -> Result<bool> {
    let cli = Cli::parse();

    let Some(ref command) = cli.command else {
        display::print_banner();
        print_no_command_help()?;
        return Ok(false);
    };

    // Suppress banner when JSON output is requested so stdout stays machine-parseable
    let is_json = matches!(command, Commands::Status { json: true, .. });
    if !is_json {
        display::print_banner();
    }

    // Check for admin privileges and enable required security tokens
    privilege::check_admin()?;
    privilege::enable_all_privileges().context(
        "Failed to enable privileges. Make sure you're running as Administrator.\n\
         Right-click the terminal/exe → 'Run as administrator'",
    )?;

    dispatch_command(command)
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

/// Print a single operation's result and return `true` if it failed.
fn report_single(result: &cleaner::CleanResult) -> bool {
    display::print_single_result(result);
    !result.success
}

/// Dispatch the parsed CLI command. Returns `true` if any operation failed.
fn dispatch_command(command: &Commands) -> Result<bool> {
    let mut had_failure = false;

    match command {
        Commands::Clean { level, verbose } => {
            display::print_clean_start(*level);
            let output = cleaner::smart_clean(*level, *verbose)?;
            display::print_clean_summary(
                &output.results,
                &output.overall_before,
                &output.overall_after,
                output.total_freed,
            );
            had_failure = output.results.iter().any(|r| !r.success);
        }

        Commands::Status { detailed, json } => {
            dispatch_status(*detailed, *json)?;
        }

        Commands::PurgeStandby {
            low_priority,
            verbose,
        } => {
            let result = if *low_priority {
                cleaner::purge_standby_low_priority(*verbose)?
            } else {
                cleaner::purge_standby_all(*verbose)?
            };
            had_failure = report_single(&result);
        }

        Commands::FlushModified { verbose } => {
            had_failure = report_single(&cleaner::flush_modified_list(*verbose)?);
        }

        Commands::EmptyWorkingsets {
            per_process,
            verbose,
        } => {
            let result = if *per_process {
                cleaner::empty_working_sets_per_process(*verbose, &[])?
            } else {
                cleaner::empty_working_sets_kernel(*verbose)?
            };
            had_failure = report_single(&result);
        }

        Commands::FlushCache { verbose } => {
            had_failure = report_single(&cleaner::flush_file_cache(*verbose)?);
        }

        Commands::Combine { verbose } => {
            had_failure = report_single(&cleaner::combine_memory(*verbose)?);
        }

        Commands::Monitor {
            interval,
            threshold,
            level,
            verbose,
        } => {
            if let Some(t) = threshold
                && *t > 100
            {
                anyhow::bail!("Threshold must be 0-100, got {t}");
            }
            monitor::run_monitor(*interval, *threshold, *level, *verbose)?;
        }
    }

    Ok(had_failure)
}

/// Handle the `status` subcommand — capture and display memory information.
fn dispatch_status(detailed: bool, json: bool) -> Result<()> {
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

    if json {
        let output = serde_json::json!({
            "snapshot": snapshot,
            "memory_lists": list_info,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        display::print_status(&snapshot, list_info.as_ref());
    }
    Ok(())
}
