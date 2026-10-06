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
// SUBSYSTEM:WINDOWS - no console window is created at startup.
// GUI launches are flash-free. For CLI usage, `console::setup_cli_console()`
// attaches to the parent terminal (or allocates a fresh console) on demand.
#![windows_subsystem = "windows"]

//! `MagicX` RAM Cleaner - binary entry point.
//!
//! Thin entry point: CLI parsing, command dispatch, and exit code mapping.
//! All domain logic lives in the library modules (see [`lib.rs`](../magicx_ram_cleaner/index.html)).

use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{ColorChoice, CommandFactory, FromArgMatches};
use colored::Colorize;

use magicx_ram_cleaner::cli::{Cli, Commands, ContextMenuAction};
use magicx_ram_cleaner::{
    cleaner, console, context_menu, display, gui, monitor, privilege, stats, strings,
};

/// Entry point - returns [`ExitCode`] instead of calling `std::process::exit()`.
///
/// Exit codes:
/// - `0` - all operations succeeded
/// - `1` - one or more cleaning operations failed (already reported)
/// - `2` - fatal error or invalid arguments (printed to stderr)
fn main() -> ExitCode {
    // Detect --notify BEFORE anything else.
    let notify = has_arg("--notify");

    // Detect whether we're launching the GUI (no subcommand, no --help,
    // no --version). With SUBSYSTEM:WINDOWS no console exists by default,
    // so GUI launches are completely flash-free.
    let gui_launch = !notify && is_gui_launch();

    // ── Console setup ────────────────────────────────────────────────
    // SUBSYSTEM:WINDOWS means NO console exists at startup.
    // For CLI mode: attach to the parent terminal (if launched from
    // cmd/powershell), keep redirected handles, or allocate a fresh one.
    // For GUI / notify modes we skip entirely - no console needed.
    let standalone =
        !gui_launch && !notify && console::setup_cli_console() == console::ConsoleMode::Standalone;

    // Detect --no-color / NO_COLOR BEFORE anything else so that all output
    // (including clap help text and the banner) respects the preference.
    let no_color = detect_no_color();
    if no_color || notify {
        colored::control::set_override(false);
    } else {
        console::enable_ansi_colors();
    }

    let code = match parse_cli(no_color) {
        Ok(cli) => run_and_report(&cli, notify),
        Err(e) => report_parse_error(&e, notify),
    };

    // If launched by double-clicking the .exe, pause so the user can read the
    // output (including any error above) before the console window closes.
    if standalone {
        console::pause_before_exit();
    }

    code
}

/// Run the parsed command and report its outcome on the console or, in
/// notify mode, as a balloon. Returns the process exit code.
fn run_and_report(cli: &Cli, notify: bool) -> ExitCode {
    let result = run(cli, notify);

    if notify {
        let (title, body) = match &result {
            Ok((false, msg)) => (strings::notification::TITLE, msg.clone()),
            Ok((true, msg)) => (strings::notification::TITLE_WARNING, msg.clone()),
            Err(e) => (strings::notification::TITLE_ERROR, format!("{e:#}")),
        };
        drop(console::show_balloon_notification(title, &body));
    } else if let Err(e) = &result {
        eprintln!("{} {e:?}", "Error:".red().bold());
    }

    match result {
        Ok((false, _)) => ExitCode::SUCCESS,
        Ok((true, _)) => ExitCode::FAILURE,
        Err(_) => ExitCode::from(2),
    }
}

/// Report a clap parse result that did not produce a [`Cli`]: real argument
/// errors as well as `--help` / `--version` output.
fn report_parse_error(error: &clap::Error, notify: bool) -> ExitCode {
    if notify {
        if error.use_stderr() {
            drop(console::show_balloon_notification(
                strings::notification::TITLE_ERROR,
                &error.to_string(),
            ));
        }
    } else {
        drop(error.print());
    }
    ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(2))
}

/// Core application logic.
///
/// Returns `Ok((had_failure, message))` where `message` is a short summary
/// for notification mode and `had_failure` indicates whether any operation
/// reported a failure.
///
/// `notify` indicates balloon-notification mode (no console attached).
fn run(cli: &Cli, notify: bool) -> Result<(bool, String)> {
    let quiet = cli.quiet || notify;

    let Some(ref command) = cli.command else {
        if notify {
            bail!("--notify requires a command such as `clean` or `status`");
        }
        // No subcommand → launch the graphical interface.
        gui::run_gui()?;
        return Ok((false, String::new()));
    };

    if notify
        && matches!(
            command,
            Commands::Monitor { .. } | Commands::ContextMenu { .. }
        )
    {
        bail!("--notify cannot be used with `monitor` or `context-menu`");
    }

    // Suppress banner when JSON output is requested so stdout stays machine-parseable
    let is_json = matches!(command, Commands::Status { json: true, .. });
    if !quiet && !is_json {
        display::print_banner();
    }

    // A dry run only prints the plan, so it needs no privileges.
    let is_dry_run = matches!(command, Commands::Clean { dry_run: true, .. });
    if !is_dry_run {
        // Check for admin privileges and enable required security tokens
        privilege::check_admin()?;
        privilege::enable_all_privileges().context(
            "Failed to enable privileges. Make sure you're running as Administrator.\n\
             Right-click the terminal/exe → 'Run as administrator'",
        )?;
    }

    dispatch_command(command, quiet, notify)
}

/// Pre-scan `argv` and environment for colour suppression requests.
///
/// This runs BEFORE clap parsing so that clap's `--help` rendering also
/// respects the preference.
///
/// Checks two sources (matching the `--no-color` doc comment contract):
/// - `--no-color` flag anywhere in `argv`
/// - `NO_COLOR` environment variable (any value, per <https://no-color.org/>)
fn detect_no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some() || has_arg("--no-color")
}

/// Parse CLI arguments with colour support applied.
///
/// When `no_color` is `true`, sets [`ColorChoice::Never`] on the clap
/// [`Command`](clap::Command) so it strips all ANSI escape codes from
/// help text (`long_about`, `after_help`, etc.) before rendering.
///
/// Uses `try_get_matches` so the caller decides how to show errors and
/// help text (and can pause a standalone console afterwards), instead of
/// clap exiting the process directly.
fn parse_cli(no_color: bool) -> Result<Cli, clap::Error> {
    let mut cmd = Cli::command();
    if no_color {
        cmd = cmd.color(ColorChoice::Never);
    }
    let matches = cmd.try_get_matches()?;
    Cli::from_arg_matches(&matches)
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
        if notify {
            return Ok((
                false,
                format!("Dry run: {} operation(s) planned", plan.len()),
            ));
        }
        display::print_dry_run(level, &plan);
        return Ok((false, String::new()));
    }
    if !exclude.is_empty() && level < cleaner::CleanLevel::Aggressive && !quiet {
        eprintln!(
            "{} --exclude has no effect at level {level}: only aggressive and nuclear empty process working sets",
            "warning:".yellow(),
        );
    }
    let ev = verbose && !quiet;
    if !quiet && !notify {
        display::print_clean_start(level);
    }
    let output = cleaner::smart_clean(level, ev, exclude)?;
    if !notify {
        display::print_clean_summary(&output);
    }
    if let Some(path) = report {
        write_report(path, &output, quiet || notify)?;
    }
    let had_failure = output.failed_count() > 0;
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

/// Handle the `status` subcommand - capture and display memory information.
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

    let top_processes = top.and_then(|count| match stats::query_top_processes(count) {
        Ok(procs) => Some(procs),
        Err(e) => {
            eprintln!(
                "{} Could not query process memory info: {}",
                "warning:".yellow(),
                e
            );
            None
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
fn write_report(path: &str, output: &cleaner::SmartCleanResult, quiet: bool) -> Result<()> {
    let json = serde_json::to_string_pretty(output).context("Failed to serialize report")?;
    std::fs::write(path, &json).with_context(|| format!("Failed to write report to '{path}'"))?;
    if !quiet {
        println!(
            "  {} Report written to {}",
            "📄".dimmed(),
            path.cyan().bold()
        );
    }
    Ok(())
}

// ─── Early flag detection ────────────────────────────────────────────────────

/// Pre-scan `argv` for a given flag before clap parsing.
///
/// Used for flags like `--notify` that need to take effect (e.g. hiding
/// the console) before clap even runs. Uses `args_os` so a non-UTF-16
/// argument cannot panic the process.
fn has_arg(flag: &str) -> bool {
    std::env::args_os().skip(1).any(|a| a == flag)
}

/// Detect whether the process was launched with no subcommand - i.e. the user
/// double-clicked the `.exe` or ran it with no arguments, which means we should
/// open the GUI.
///
/// Returns `true` when the only argv entries are the exe path itself, plus
/// optional global flags that do not change GUI behaviour (`--no-color`,
/// `-q` / `--quiet`). Any other argument (subcommand name, `--help`,
/// `--version`, `--notify`) means this is a CLI launch.
fn is_gui_launch() -> bool {
    std::env::args_os()
        .skip(1)
        .all(|a| a == "--no-color" || a == "-q" || a == "--quiet")
}

// ─── Notification message formatting ─────────────────────────────────────────

/// Format a notification body for a [`SmartCleanResult`](cleaner::SmartCleanResult).
fn format_clean_notification(output: &cleaner::SmartCleanResult) -> String {
    let freed = stats::format_signed_bytes(output.reclaimed_bytes());
    let before_load = output.overall_before.memory_load_percent;
    let after_load = output.overall_after.memory_load_percent;
    let ops = output.results.len();
    let ok = ops - output.failed_count();
    format!(
        "Freed {freed}\n{ok}/{ops} operations succeeded\nRAM usage: {before_load}% → {after_load}%"
    )
}

/// Format a notification body for a single [`CleanResult`](cleaner::CleanResult).
fn format_single_notification(result: &cleaner::CleanResult) -> String {
    let status = if result.success { "OK" } else { "FAILED" };
    let freed = stats::format_signed_bytes(result.reclaimed_bytes());
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
