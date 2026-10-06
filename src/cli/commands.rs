//! Command dispatch: runs each subcommand and reports its result.

use anyhow::{Context, Result};
use colored::Colorize;

use super::args::{Commands, ContextMenuAction};
use super::{Outcome, display, monitor, notification};
use crate::engine::{self, CleanLevel, CleanResult, SmartCleanResult};
use crate::integration::context_menu;
use crate::memory::{self, MemorySnapshot};

/// How a command should report: to the terminal (optionally quiet) or as a
/// notification summary.
#[derive(Debug, Clone, Copy)]
pub(super) struct Reporting {
    /// Suppress the banner and non-essential output, and force verbose off.
    pub quiet: bool,
    /// Produce a notification summary instead of terminal output.
    pub notify: bool,
}

impl Reporting {
    /// Whether progress should be printed for a command run with `--verbose`.
    const fn verbose(self, requested: bool) -> bool {
        requested && !self.quiet
    }
}

/// A cleaner for the live system that prints its progress when `verbose`.
fn cli_cleaner(verbose: bool) -> engine::Cleaner<'static> {
    engine::Cleaner::new(&engine::WindowsMemory, display::progress_printer(verbose))
}

/// Report a single operation's result and turn it into an [`Outcome`].
fn single_result(result: &CleanResult, reporting: Reporting) -> Outcome {
    if reporting.notify {
        Outcome {
            had_failure: !result.success,
            notification: notification::operation_summary(result),
        }
    } else {
        display::print_single_result(result);
        Outcome::from_failure(!result.success)
    }
}

/// Run a parsed command.
pub(super) fn dispatch(command: &Commands, reporting: Reporting) -> Result<Outcome> {
    match command {
        Commands::Clean {
            level,
            verbose,
            report,
            dry_run,
            exclude,
        } => {
            let options = CleanOptions {
                level: (*level).into(),
                verbose: reporting.verbose(*verbose),
                report: report.as_deref(),
                dry_run: *dry_run,
                exclude,
            };
            clean(&options, reporting)
        }

        Commands::Status {
            detailed,
            json,
            top,
        } => {
            if reporting.notify {
                let snapshot = MemorySnapshot::capture()?;
                return Ok(Outcome {
                    had_failure: false,
                    notification: notification::status_summary(&snapshot),
                });
            }
            status(*detailed, *json, *top)?;
            Ok(Outcome::default())
        }

        Commands::PurgeStandby {
            low_priority,
            verbose,
        } => {
            let mut cleaner = cli_cleaner(reporting.verbose(*verbose));
            let result = if *low_priority {
                cleaner.purge_standby_low_priority()?
            } else {
                cleaner.purge_standby()?
            };
            Ok(single_result(&result, reporting))
        }

        Commands::FlushModified { verbose } => {
            let result = cli_cleaner(reporting.verbose(*verbose)).flush_modified()?;
            Ok(single_result(&result, reporting))
        }

        Commands::EmptyWorkingsets {
            per_process,
            exclude,
            verbose,
        } => {
            let mut cleaner = cli_cleaner(reporting.verbose(*verbose));
            let result = if *per_process || !exclude.is_empty() {
                cleaner.empty_working_sets_per_process(exclude)?
            } else {
                cleaner.empty_working_sets()?
            };
            Ok(single_result(&result, reporting))
        }

        Commands::FlushCache { verbose } => {
            let result = cli_cleaner(reporting.verbose(*verbose)).flush_file_cache()?;
            Ok(single_result(&result, reporting))
        }

        Commands::FlushRegistry { verbose } => {
            let result = cli_cleaner(reporting.verbose(*verbose)).flush_registry_cache()?;
            Ok(single_result(&result, reporting))
        }

        Commands::Combine { verbose } => {
            let result = cli_cleaner(reporting.verbose(*verbose)).combine_memory()?;
            Ok(single_result(&result, reporting))
        }

        Commands::Monitor {
            interval,
            threshold,
            level,
            cooldown,
            verbose,
        } => {
            let had_failure = monitor::run_monitor(
                *interval,
                *threshold,
                (*level).into(),
                *cooldown,
                reporting.verbose(*verbose),
            )?;
            Ok(Outcome::from_failure(had_failure))
        }

        Commands::ContextMenu { action } => context_menu_command(*action),
    }
}

/// Run the `context-menu` command.
fn context_menu_command(action: ContextMenuAction) -> Result<Outcome> {
    match action {
        ContextMenuAction::Install => {
            context_menu::install(&context_menu::current_exe_path()?)?;
            display::print_context_menu_installed(context_menu::entry_labels());
        }
        ContextMenuAction::Uninstall => {
            display::print_context_menu_removed(context_menu::uninstall()?);
        }
    }
    Ok(Outcome::default())
}

/// Options of the `clean` command.
struct CleanOptions<'a> {
    /// Cleaning level.
    level: CleanLevel,
    /// Print per-operation progress.
    verbose: bool,
    /// Path of a JSON report to write.
    report: Option<&'a str>,
    /// Only print the plan.
    dry_run: bool,
    /// Process names to protect from working-set trimming.
    exclude: &'a [String],
}

/// Run the `clean` command.
fn clean(options: &CleanOptions<'_>, reporting: Reporting) -> Result<Outcome> {
    let level = options.level;
    if options.dry_run {
        let plan = engine::dry_run_plan(level, !options.exclude.is_empty());
        if reporting.notify {
            return Ok(Outcome {
                had_failure: false,
                notification: format!("Dry run: {} operation(s) planned", plan.len()),
            });
        }
        display::print_dry_run(level, &plan);
        return Ok(Outcome::default());
    }
    if !options.exclude.is_empty() && !level.empties_working_sets() && !reporting.quiet {
        eprintln!(
            "{} --exclude has no effect at level {level}: only aggressive and nuclear empty process working sets",
            "warning:".yellow(),
        );
    }
    if !reporting.quiet && !reporting.notify {
        display::print_clean_start(level);
    }

    let output = cli_cleaner(options.verbose).smart_clean(level, options.exclude)?;

    if !reporting.notify {
        display::print_clean_summary(&output);
    }
    if let Some(path) = options.report {
        write_report(path, &output, reporting.quiet || reporting.notify)?;
    }
    Ok(Outcome {
        had_failure: output.failed_count() > 0,
        notification: if reporting.notify {
            notification::clean_summary(&output)
        } else {
            String::new()
        },
    })
}

/// Run the `status` command: capture and display memory information.
fn status(detailed: bool, json: bool, top: Option<usize>) -> Result<()> {
    let snapshot = MemorySnapshot::capture()?;
    let list_info = if detailed {
        warn_on_error(
            memory::MemoryListInfo::query(),
            "Could not query memory list details",
        )
    } else {
        None
    };
    let file_cache = if detailed {
        warn_on_error(
            memory::FileCacheSnapshot::capture(),
            "Could not query file cache info",
        )
    } else {
        None
    };
    let top_processes = top.and_then(|count| {
        warn_on_error(
            memory::query_top_processes(count),
            "Could not query process memory info",
        )
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
        if let Some(procs) = &top_processes {
            display::print_top_processes(procs);
        }
    }
    Ok(())
}

/// Print a warning for a failed optional query and continue without it.
fn warn_on_error<T>(result: Result<T>, what: &str) -> Option<T> {
    result
        .map_err(|e| eprintln!("{} {what}: {e}", "warning:".yellow()))
        .ok()
}

/// Write a cleaning report to a JSON file.
fn write_report(path: &str, output: &SmartCleanResult, quiet: bool) -> Result<()> {
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
