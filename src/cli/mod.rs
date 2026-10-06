//! # Command-line interface
//!
//! Argument definitions ([`args`]), command dispatch, terminal output, the
//! continuous monitor, and the summaries shown as balloon notifications when
//! a command runs with `--notify` (the context-menu entries).

pub mod args;
mod commands;
pub mod display;
mod monitor;
mod notification;

use anyhow::{Context, Result, bail};

use self::args::Commands;
use self::commands::Reporting;
use crate::platform::privilege;

/// What a command produced, for the exit code and the `--notify` balloon.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// At least one operation reported failure (exit code 1).
    pub had_failure: bool,
    /// Summary shown in the balloon in `--notify` mode; empty otherwise.
    pub notification: String,
}

impl Outcome {
    /// An outcome without a notification summary.
    const fn from_failure(had_failure: bool) -> Self {
        Self {
            had_failure,
            notification: String::new(),
        }
    }
}

/// Run a CLI command.
///
/// `quiet` suppresses the banner and non-essential output; `notify` means
/// the result is shown as a balloon (no console) and implies `quiet`.
///
/// # Errors
///
/// Fails if the command cannot be used with `--notify`, administrator
/// rights or privileges are missing (everything except a dry run needs
/// them), or the command itself fails.
pub fn run(command: &Commands, quiet: bool, notify: bool) -> Result<Outcome> {
    let reporting = Reporting {
        quiet: quiet || notify,
        notify,
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
    if !reporting.quiet && !is_json {
        display::print_banner();
    }

    // A dry run only prints the plan, so it needs no privileges.
    let is_dry_run = matches!(command, Commands::Clean { dry_run: true, .. });
    if !is_dry_run {
        privilege::check_admin()?;
        privilege::enable_all_privileges().context(
            "Failed to enable privileges. Make sure you're running as Administrator.\n\
             Right-click the terminal/exe → 'Run as administrator'",
        )?;
    }

    commands::dispatch(command, reporting)
}
