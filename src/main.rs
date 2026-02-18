// ─── Compiler-enforced quality gates ─────────────────────────────────────────
#![deny(
    unused_must_use,
    unreachable_patterns,
    unsafe_code,
    unused_imports,
    unused_variables,
    dead_code,
    rustdoc::broken_intra_doc_links
)]

//! `MagicX` RAM Cleaner — binary entry point.
//!
//! Thin entry point: CLI parsing, command dispatch, and exit code mapping.
//! All domain logic lives in the library modules (see [`lib.rs`](../magicx_ram_cleaner/index.html)).

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{ColorChoice, CommandFactory, FromArgMatches};
use colored::Colorize;

use magicx_ram_cleaner::cli::{Cli, Commands, ContextMenuAction};
use magicx_ram_cleaner::{cleaner, console, context_menu, display, monitor, privilege, stats};

/// Entry point — returns [`ExitCode`] instead of calling `std::process::exit()`.
///
/// Exit codes:
/// - `0` — all operations succeeded
/// - `1` — one or more cleaning operations failed (already reported)
/// - `2` — fatal error (printed to stderr)
fn main() -> ExitCode {
    // Detect --notify BEFORE anything else so we can hide the console
    // window immediately, before any visible output appears.
    let notify = detect_flag("--notify");

    if notify {
        // Detach from the console so no terminal window is visible.
        // After this call stdout/stderr are invalid — all user feedback
        // goes through the balloon notification at the end.
        console::hide_console_window();
    }

    // Detect --no-color / NO_COLOR BEFORE anything else so that all output
    // (including clap help text and the banner) respects the preference.
    let no_color = detect_no_color();
    if no_color || notify {
        colored::control::set_override(false);
    } else {
        console::enable_ansi_colors();
    }

    let standalone = !notify && console::is_standalone_console();

    let result = run(no_color, notify);

    // In notify mode, show a balloon notification with the outcome.
    if notify {
        let (title, body) = match &result {
            Ok((false, msg)) => ("MagicX RAM Cleaner", msg.as_str()),
            Ok((true, msg)) => ("MagicX RAM Cleaner — Warning", msg.as_str()),
            Err(e) => {
                // Format the error into a static-lifetime-friendly string
                // (we'll use a local binding so the borrow lives long enough).
                drop(console::show_balloon_notification(
                    "MagicX RAM Cleaner \u{2014} Error",
                    &format!("{e:#}"),
                ));
                return ExitCode::from(2);
            }
        };
        drop(console::show_balloon_notification(title, body));
    }

    // If launched by double-clicking the .exe, pause so the user can read the
    // output before the console window closes.
    if standalone {
        console::pause_before_exit();
    }

    match result {
        Ok((false, _)) => ExitCode::SUCCESS,
        Ok((true, _)) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{} {e:?}", "Error:".red().bold());
            ExitCode::from(2)
        }
    }
}

/// Core application logic.
///
/// Returns `Ok((had_failure, message))` where `message` is a short summary
/// for notification mode and `had_failure` indicates whether any operation
/// reported a failure.
///
/// `no_color` is pre-computed by [`detect_no_color`] before this function
/// is called, so that clap help text rendering also strips ANSI codes.
/// `notify` indicates balloon-notification mode (console already detached).
fn run(no_color: bool, notify: bool) -> Result<(bool, String)> {
    let cli = parse_cli(no_color)?;

    let quiet = cli.quiet || notify;

    let Some(ref command) = cli.command else {
        if !quiet {
            display::print_banner();
        }
        print_no_command_help()?;
        return Ok((false, String::new()));
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

    dispatch_command(command, quiet, notify)
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

/// Handle a single [`CleanResult`](cleaner::CleanResult) in both normal and
/// notification modes. Returns `(had_failure, notification_message)`.
fn handle_single_result(result: &cleaner::CleanResult, notify: bool) -> (bool, String) {
    if notify {
        (!result.success, format_single_notification(result))
    } else {
        (report_single(result), String::new())
    }
}

/// Dispatch the `clean` subcommand logic.
// Four independent semantic flags (verbose, quiet, notify, dry_run) that do not
// naturally group into a two-variant enum. Collapsing them would hurt readability.
#[allow(clippy::fn_params_excessive_bools)]
fn dispatch_clean(
    level: cleaner::CleanLevel,
    verbose: bool,
    quiet: bool,
    notify: bool,
    report: Option<&str>,
    dry_run: bool,
    exclude: &[String],
) -> Result<(bool, String)> {
    if dry_run {
        let plan = cleaner::dry_run_plan(level, !exclude.is_empty());
        display::print_dry_run(level, &plan);
        return Ok((false, String::new()));
    }
    let ev = verbose && !quiet;
    if !quiet && !notify {
        display::print_clean_start(level);
    }
    let output = cleaner::smart_clean(level, ev, exclude)?;
    if !notify {
        display::print_clean_summary(
            &output.results,
            &output.overall_before,
            &output.overall_after,
            output.total_freed,
            output.total_elapsed_secs,
        );
    }
    if let Some(path) = report {
        write_report(path, &output)?;
    }
    let had_failure = output.results.iter().any(|r| !r.success);
    let msg = if notify {
        format_clean_notification(&output)
    } else {
        String::new()
    };
    Ok((had_failure, msg))
}

/// Dispatch the parsed CLI command. Returns `(had_failure, message)`.
///
/// When `quiet` is `true`, the banner is already suppressed and verbose progress
/// is forced off. Only results, errors, and machine-readable data are printed.
/// When `notify` is `true`, a short summary string is returned for the balloon.
fn dispatch_command(command: &Commands, quiet: bool, notify: bool) -> Result<(bool, String)> {
    let (mut had_failure, mut notify_msg) = (false, String::new());

    match command {
        Commands::Clean {
            level,
            verbose,
            report,
            dry_run,
            exclude,
        } => {
            return dispatch_clean(
                *level,
                *verbose,
                quiet,
                notify,
                report.as_deref(),
                *dry_run,
                exclude,
            );
        }

        Commands::Status {
            detailed,
            json,
            top,
        } => {
            if notify {
                let snapshot = stats::MemorySnapshot::capture()?;
                notify_msg = format_status_notification(&snapshot);
            } else {
                dispatch_status(*detailed, *json, *top)?;
            }
        }

        Commands::PurgeStandby {
            low_priority,
            verbose,
        } => {
            let ev = *verbose && !quiet;
            let r = if *low_priority {
                cleaner::purge_standby_low_priority(ev)?
            } else {
                cleaner::purge_standby_all(ev)?
            };
            (had_failure, notify_msg) = handle_single_result(&r, notify);
        }

        Commands::FlushModified { verbose } => {
            let r = cleaner::flush_modified_list(*verbose && !quiet)?;
            (had_failure, notify_msg) = handle_single_result(&r, notify);
        }

        Commands::EmptyWorkingsets {
            per_process,
            exclude,
            verbose,
        } => {
            let ev = *verbose && !quiet;
            let r = if *per_process || !exclude.is_empty() {
                cleaner::empty_working_sets_per_process(ev, exclude)?
            } else {
                cleaner::empty_working_sets_kernel(ev)?
            };
            (had_failure, notify_msg) = handle_single_result(&r, notify);
        }

        Commands::FlushCache { verbose } => {
            let r = cleaner::flush_file_cache(*verbose && !quiet)?;
            (had_failure, notify_msg) = handle_single_result(&r, notify);
        }

        Commands::FlushRegistry { verbose } => {
            let r = cleaner::flush_registry_cache(*verbose && !quiet)?;
            (had_failure, notify_msg) = handle_single_result(&r, notify);
        }

        Commands::Combine { verbose } => {
            let r = cleaner::combine_memory(*verbose && !quiet)?;
            (had_failure, notify_msg) = handle_single_result(&r, notify);
        }

        Commands::Monitor {
            interval,
            threshold,
            level,
            cooldown,
            verbose,
        } => {
            monitor::run_monitor(*interval, *threshold, *level, *cooldown, *verbose && !quiet)?;
        }

        Commands::ContextMenu { action } => match action {
            ContextMenuAction::Install => {
                let exe = context_menu::current_exe_path()?;
                context_menu::install(&exe)?;
            }
            ContextMenuAction::Uninstall => context_menu::uninstall()?,
        },
    }

    Ok((had_failure, notify_msg))
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

// ─── Early flag detection ────────────────────────────────────────────────────

/// Pre-scan `argv` for a given flag before clap parsing.
///
/// Used for flags like `--notify` that need to take effect (e.g. hiding
/// the console) before clap even runs.
fn detect_flag(flag: &str) -> bool {
    std::env::args().any(|a| a == flag)
}

// ─── Notification message formatting ─────────────────────────────────────────

/// Format a notification body for a [`SmartCleanResult`](cleaner::SmartCleanResult).
fn format_clean_notification(output: &cleaner::SmartCleanResult) -> String {
    let freed = if output.total_freed >= 0 {
        stats::format_bytes(output.total_freed as u64)
    } else {
        format!(
            "-{}",
            stats::format_bytes(output.total_freed.unsigned_abs())
        )
    };
    let before_load = output.overall_before.memory_load_percent;
    let after_load = output.overall_after.memory_load_percent;
    let ops = output.results.len();
    let ok = output.results.iter().filter(|r| r.success).count();
    format!(
        "Freed {freed}\n{ok}/{ops} operations succeeded\nRAM usage: {before_load}% → {after_load}%"
    )
}

/// Format a notification body for a single [`CleanResult`](cleaner::CleanResult).
fn format_single_notification(result: &cleaner::CleanResult) -> String {
    let status = if result.success { "OK" } else { "FAILED" };
    let freed = if result.freed_bytes >= 0 {
        stats::format_bytes(result.freed_bytes as u64)
    } else {
        format!(
            "-{}",
            stats::format_bytes(result.freed_bytes.unsigned_abs())
        )
    };
    format!(
        "{}: {status}\nFreed {freed}\nRAM usage: {}% → {}%",
        result.operation, result.load_before, result.load_after
    )
}

/// Format a notification body for a memory status snapshot.
fn format_status_notification(snapshot: &stats::MemorySnapshot) -> String {
    let used = snapshot
        .total_physical
        .saturating_sub(snapshot.available_physical);
    format!(
        "RAM: {} / {} ({}% used)\nAvailable: {}",
        stats::format_bytes(used),
        stats::format_bytes(snapshot.total_physical),
        snapshot.memory_load_percent,
        stats::format_bytes(snapshot.available_physical),
    )
}
