//! # Application launcher
//!
//! Decides between the GUI and the CLI, prepares the console accordingly,
//! parses arguments, runs the command and maps the outcome to an exit code
//! (or a balloon notification in `--notify` mode). This is the only module
//! that depends on both [`crate::cli`] and [`crate::gui`].
//!
//! Exit codes:
//! - `0` - all operations succeeded
//! - `1` - one or more cleaning operations failed (already reported)
//! - `2` - fatal error or invalid arguments (printed to stderr)

use std::ffi::OsString;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{ColorChoice, CommandFactory, FromArgMatches};
use colored::Colorize;

use crate::cli::{self, Outcome, args::Cli};
use crate::gui;
use crate::platform::{console, loader, notify};
use crate::strings;

/// Run the application with the process's arguments.
#[must_use]
pub fn run() -> ExitCode {
    loader::restrict_dll_search_to_system32();
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let notify = has_arg(&args, "--notify");
    let gui_launch = !notify && is_gui_launch(&args);

    // ── Console setup ────────────────────────────────────────────────
    // GUI / notify modes need no console: drop one that older Windows
    // created for us, and if the GUI was started from a terminal, hand it
    // to a detached copy so the shell is not blocked until the window closes.
    // CLI mode: share the parent terminal, keep redirected handles, or
    // allocate a fresh console.
    if gui_launch || notify {
        console::release_private_console();
        if gui_launch && console::shares_parent_console() && console::relaunch_detached() {
            return ExitCode::SUCCESS;
        }
    }
    let standalone =
        !gui_launch && !notify && console::setup_cli_console() == console::ConsoleMode::Standalone;

    // Apply --no-color / NO_COLOR before parsing so clap's help text and the
    // banner respect it too.
    let no_color = std::env::var_os("NO_COLOR").is_some() || has_arg(&args, "--no-color");
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

/// Run the parsed command line and report its outcome on the console or, in
/// notify mode, as a balloon. Returns the process exit code.
fn run_and_report(cli: &Cli, notify: bool) -> ExitCode {
    let result = execute(cli, notify);

    if notify {
        let (title, body) = match &result {
            Ok(outcome) if outcome.had_failure => (
                strings::notification::TITLE_WARNING,
                outcome.notification.clone(),
            ),
            Ok(outcome) => (strings::notification::TITLE, outcome.notification.clone()),
            Err(e) => (strings::notification::TITLE_ERROR, format!("{e:#}")),
        };
        drop(notify::show_balloon_notification(title, &body));
    } else if let Err(e) = &result {
        eprintln!("{} {e:?}", "Error:".red().bold());
    }

    match result {
        Ok(outcome) if outcome.had_failure => ExitCode::FAILURE,
        Ok(_) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(2),
    }
}

/// Run a CLI command, or the GUI when no command was given.
fn execute(cli: &Cli, notify: bool) -> Result<Outcome> {
    let Some(command) = &cli.command else {
        if notify {
            bail!("--notify requires a command such as `clean` or `status`");
        }
        gui::run_gui()?;
        return Ok(Outcome::default());
    };
    cli::run(command, cli.quiet, notify)
}

/// Report a clap parse result that did not produce a [`Cli`]: real argument
/// errors as well as `--help` / `--version` output.
fn report_parse_error(error: &clap::Error, notify: bool) -> ExitCode {
    if notify {
        if error.use_stderr() {
            drop(notify::show_balloon_notification(
                strings::notification::TITLE_ERROR,
                &error.to_string(),
            ));
        }
    } else {
        drop(error.print());
    }
    ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(2))
}

/// Parse CLI arguments with colour support applied.
///
/// When `no_color` is `true`, sets [`ColorChoice::Never`] so clap strips all
/// ANSI escape codes from help text. Uses `try_get_matches` so the caller
/// decides how to show errors and help text (and can pause a standalone
/// console afterwards), instead of clap exiting the process directly.
fn parse_cli(no_color: bool) -> Result<Cli, clap::Error> {
    let mut cmd = Cli::command();
    if no_color {
        cmd = cmd.color(ColorChoice::Never);
    }
    let matches = cmd.try_get_matches()?;
    Cli::from_arg_matches(&matches)
}

/// Whether `flag` appears among `args` (the arguments after the exe path).
///
/// Used for flags that must take effect before clap runs, such as
/// `--notify`. Works on `OsString`s so non-UTF-16 arguments cannot panic.
fn has_arg(args: &[OsString], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// Whether `args` (the arguments after the exe path) mean "open the GUI".
///
/// True when there are no arguments, or only global flags that do not
/// change GUI behaviour (`--no-color`, `-q` / `--quiet`). Any other argument
/// (subcommand, `--help`, `--version`, `--notify`) means a CLI launch.
fn is_gui_launch(args: &[OsString]) -> bool {
    args.iter()
        .all(|a| a == "--no-color" || a == "-q" || a == "--quiet")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn no_arguments_or_cosmetic_flags_open_the_gui() {
        assert!(is_gui_launch(&args(&[])));
        assert!(is_gui_launch(&args(&["--no-color", "-q"])));
        assert!(is_gui_launch(&args(&["--quiet"])));
    }

    #[test]
    fn commands_and_meta_flags_run_the_cli() {
        for list in [
            &["clean"][..],
            &["--help"],
            &["-V"],
            &["--notify"],
            &["-q", "status"],
        ] {
            assert!(!is_gui_launch(&args(list)), "{list:?}");
        }
    }

    #[test]
    fn has_arg_matches_whole_arguments_only() {
        let a = args(&["clean", "--notify"]);
        assert!(has_arg(&a, "--notify"));
        assert!(!has_arg(&a, "--no"));
    }
}
