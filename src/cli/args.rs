//! # `MagicX` RAM Cleaner - CLI Definitions
//!
//! Clap-based command-line interface: parser struct, subcommands,
//! styling, and help text constants. Separated from `main.rs` so that
//! CLI surface area can evolve independently of application wiring.

use clap::builder::styling::{AnsiColor, Styles};
use clap::{Parser, Subcommand};

use clap::ValueEnum;

use crate::engine::CleanLevel;
use crate::strings;

// ─── Clap styling ────────────────────────────────────────────────────────────

/// Clap terminal colour theme.
pub const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Cyan.on_default().bold())
    .literal(AnsiColor::Green.on_default().bold())
    .placeholder(AnsiColor::Yellow.on_default())
    .usage(AnsiColor::Cyan.on_default().bold())
    .valid(AnsiColor::Green.on_default());

// ─── Help text constants ─────────────────────────────────────────────────────

/// Long description shown for `magicx-ram-cleaner --help`.
pub const LONG_ABOUT: &str = "\
MagicX RAM Cleaner \x1b[90m-\x1b[0m frees Windows RAM from the command line.

\
Purges the standby list, flushes modified pages, trims working sets, and \
flushes the file and registry caches, individually or as one smart clean.

\
\x1b[1;36mFEATURES:\x1b[0m
  \
\x1b[1;32m★\x1b[0m Built-in GUI - double-click the exe or run without arguments
  \
\x1b[1;32m★\x1b[0m Smart cleaning with 4 aggressiveness levels (gentle → nuclear)
  \
\x1b[1;32m★\x1b[0m Individual control over each memory operation
  \
\x1b[1;32m★\x1b[0m Detailed diagnostics with per-priority standby breakdown
  \
\x1b[1;32m★\x1b[0m Continuous monitoring with auto-clean at configurable thresholds
  \
\x1b[1;32m★\x1b[0m File system cache management
  \
\x1b[1;32m★\x1b[0m Memory page combining / deduplication (Windows 10+)
  \
\x1b[1;32m★\x1b[0m Before/after reporting showing exact RAM freed
  \
\x1b[1;32m★\x1b[0m Optimal operation ordering for maximum recovery
  \
\x1b[1;32m★\x1b[0m Windows context menu integration (Desktop & folder right-click)

\
\x1b[1;36mCLEANING LEVELS:\x1b[0m
  \
\x1b[32mgentle\x1b[0m      Safe - purge ALL standby pages \x1b[90m(no process impact)
  \
\x1b[33mmoderate\x1b[0m    Balanced - flush modified pages + purge ALL standby
  \
\x1b[1;33maggressive\x1b[0m  Full clean: cache + working sets + modified + standby \x1b[1;33m[DEFAULT]\x1b[0m
  \
\x1b[1;31mnuclear\x1b[0m     Maximum - everything + memory combining + 2nd pass

\
\x1b[1;36mQUICK START:\x1b[0m
  \
\x1b[32mmagicx-ram-cleaner\x1b[0m                             \x1b[90m# Launch GUI (no arguments)\x1b[0m
  \
\x1b[32mmagicx-ram-cleaner clean\x1b[0m                    \x1b[90m# Smart clean (aggressive)\x1b[0m
  \
\x1b[32mmagicx-ram-cleaner clean --level gentle\x1b[0m     \x1b[90m# Minimal impact clean\x1b[0m
  \
\x1b[32mmagicx-ram-cleaner status\x1b[0m                   \x1b[90m# Show memory usage\x1b[0m
  \
\x1b[32mmagicx-ram-cleaner monitor --threshold 80\x1b[0m   \x1b[90m# Auto-clean at 80%\x1b[0m

\
\x1b[1;33mREQUIREMENTS:\x1b[0m
  \
Must be run as \x1b[1;33mAdministrator\x1b[0m (right-click → Run as administrator).
  \
Windows 10/11 or Windows Server 2016+ required.";

/// Short after-help shown for `magicx-ram-cleaner -h`.
pub const AFTER_HELP_SHORT: &str = "\
\x1b[90mRun\x1b[0m \x1b[32mmagicx-ram-cleaner --help\x1b[0m \x1b[90mfor full documentation and examples.\x1b[0m";

/// Long after-help shown for `magicx-ram-cleaner --help`.
pub const AFTER_HELP_LONG: &str = "\
\x1b[1;36mEXAMPLES:\x1b[0m

  \
\x1b[36mBasic Cleaning:\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner clean\x1b[0m                        \x1b[90m# Aggressive clean (default)\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner clean --level gentle\x1b[0m         \x1b[90m# Safe, minimal impact\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner clean --level nuclear -v\x1b[0m     \x1b[90m# Maximum recovery, verbose\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner clean --exclude chrome\x1b[0m      \x1b[90m# Protect Chrome from trimming\x1b[0m

  \
\x1b[36mIndividual Operations:\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner purge-standby\x1b[0m                \x1b[90m# Like EmptyStandbyList\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner purge-standby --low-priority\x1b[0m \x1b[90m# Safest purge\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner flush-modified\x1b[0m               \x1b[90m# Write dirty pages to disk\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner flush-cache\x1b[0m                  \x1b[90m# Release file cache\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner flush-registry\x1b[0m               \x1b[90m# Flush registry hive cache\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner empty-workingsets\x1b[0m            \x1b[90m# Trim all process memory\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner combine\x1b[0m                      \x1b[90m# Deduplicate pages (Win10+)\x1b[0m

  \
\x1b[36mDiagnostics:\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner status\x1b[0m                       \x1b[90m# Memory overview\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner status --detailed\x1b[0m            \x1b[90m# Full standby breakdown\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner status --top 10\x1b[0m              \x1b[90m# Top 10 RAM-hungry processes\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner status --json\x1b[0m                \x1b[90m# Machine-readable output\x1b[0m

  \
\x1b[36mMonitoring:\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner monitor\x1b[0m                      \x1b[90m# Watch memory usage live\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner monitor -t 85 -i 5\x1b[0m           \x1b[90m# Auto-clean at 85%, every 5s\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner monitor -t 80 -l nuclear\x1b[0m     \x1b[90m# Nuclear clean at 80%\x1b[0m

  \
\x1b[36mAdvanced Workflows:\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner flush-modified\x1b[0m               \x1b[90m# Step 1: flush dirty pages\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner purge-standby\x1b[0m                \x1b[90m# Step 2: purge standby list\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner empty-workingsets --per-process\x1b[0m \x1b[90m# Per-process details\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner empty-workingsets --exclude chrome --exclude firefox\x1b[0m \x1b[90m# Protect browsers\x1b[0m

  \
\x1b[36mContext Menu:\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner context-menu install\x1b[0m              \x1b[90m# Add to right-click menu\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner context-menu uninstall\x1b[0m             \x1b[90m# Remove from right-click menu\x1b[0m

  \
\x1b[36mGUI Mode:\x1b[0m
    \
\x1b[32mmagicx-ram-cleaner\x1b[0m                                     \x1b[90m# Launch the graphical interface\x1b[0m
    \x1b[90mDouble-click the exe to open the GUI with dashboard, charts, and settings.\x1b[0m

\
\x1b[1;36mKEY CONCEPTS:\x1b[0m
  \
\x1b[1mStandby List\x1b[0m     Cached pages in RAM - freed first when memory is needed
  \
\x1b[1mWorking Sets\x1b[0m     Pages actively mapped by each running process
  \
\x1b[1mModified Pages\x1b[0m   Dirty pages not yet written to disk or pagefile
  \
\x1b[1mFile Cache\x1b[0m       RAM used by Windows to cache recent file I/O
  \
\x1b[1mRegistry Cache\x1b[0m   RAM used to cache modified registry hive pages
  \
\x1b[1mPage Combining\x1b[0m   Deduplicating identical pages via copy-on-write

\
\x1b[1;36mEXIT CODES:\x1b[0m
  \
\x1b[32m0\x1b[0m  All operations completed successfully
  \
\x1b[33m1\x1b[0m  One or more operations failed
  \
\x1b[31m2\x1b[0m  Fatal error, invalid arguments or missing administrator privileges

\
\x1b[1;36mLEARN MORE:\x1b[0m
  \
Repository:  \x1b[36mhttps://github.com/ehsan18t/magicx-ram-cleaner\x1b[0m
  \
Run \x1b[32mmagicx-ram-cleaner <command> --help\x1b[0m for detailed command information.";

/// `MagicX` RAM Cleaner command line.
///
/// Individual memory operations, smart cleaning levels, detailed
/// diagnostics, and monitoring with auto-clean.
///
/// REQUIRES: Run as Administrator (right-click → Run as administrator).
#[derive(Parser)]
#[command(
    name = "magicx-ram-cleaner",
    version,
    styles = STYLES,
    about = "MagicX RAM Cleaner: frees Windows RAM by purging standby, flushing modified pages and trimming working sets",
    long_about = LONG_ABOUT,
    after_help = AFTER_HELP_SHORT,
    after_long_help = AFTER_HELP_LONG,
)]
pub struct Cli {
    /// The subcommand to execute.
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Disable coloured terminal output (including help text).
    ///
    /// Strips all ANSI escape codes from both runtime output and clap
    /// help text rendering. Useful for piping output to files or
    /// non-ANSI terminals.
    ///
    /// Also respects the `NO_COLOR` environment variable
    /// (any value, per <https://no-color.org/>).
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Suppress banner and non-essential output.
    ///
    /// Only results, errors, and machine-readable data are printed.
    /// Implies `--verbose false` for cleaning operations.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Run silently and show a balloon notification with results.
    ///
    /// Skips console attachment (no terminal window appears), suppresses
    /// all terminal output, and displays a brief Windows balloon
    /// notification when the operation finishes. The notification
    /// auto-dismisses and is not saved in the Action Center. Used
    /// internally by context menu entries.
    #[arg(long, global = true, hide = true)]
    pub notify: bool,
}

/// Available CLI subcommands.
#[derive(Subcommand)]
pub enum Commands {
    /// Smart clean - the recommended way to free RAM.
    ///
    /// Runs multiple memory operations in optimal order based on the
    /// selected aggressiveness level (see --level). Shows before/after stats.
    #[command(verbatim_doc_comment)]
    Clean {
        /// Cleaning aggressiveness level.
        #[arg(short, long, value_enum, default_value = "aggressive")]
        level: LevelArg,

        /// Show detailed progress of each operation.
        #[arg(short, long)]
        verbose: bool,

        /// Write cleaning results to a JSON report file.
        #[arg(long, value_name = "FILE", conflicts_with = "dry_run")]
        report: Option<String>,

        /// Preview what operations would run without executing them.
        #[arg(long)]
        dry_run: bool,

        /// Exclude processes by name during working set emptying (case-insensitive, .exe optional).
        ///
        /// Can be specified multiple times: --exclude chrome --exclude firefox
        ///
        /// When set, working set operations use per-process trimming instead of kernel-level.
        /// Only affects the aggressive and nuclear levels (the ones that empty working sets).
        #[arg(long, value_name = "NAME")]
        exclude: Vec<String>,
    },

    /// Show detailed memory usage status.
    ///
    /// Displays physical memory, page lists, standby priorities,
    /// commit charge, kernel pools, and system counters.
    #[command(verbatim_doc_comment)]
    Status {
        /// Show detailed memory list information (standby priorities, modified pages, etc.).
        /// Requires `SeProfileSingleProcessPrivilege`.
        #[arg(short, long)]
        detailed: bool,

        /// Output as JSON for scripting/automation.
        #[arg(short, long)]
        json: bool,

        /// Show top N processes by memory (working set) usage.
        #[arg(
            long,
            value_name = "N",
            value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..=1000)
        )]
        top: Option<usize>,
    },

    /// Purge standby list - equivalent to `EmptyStandbyList` but better.
    ///
    /// Removes cached pages from the standby list, making them available
    /// for new allocations. By default purges ALL priorities.
    ///
    /// Tip: Run `flush-modified` first, then `purge-standby` for maximum effect.
    #[command(verbatim_doc_comment)]
    PurgeStandby {
        /// Only purge low-priority (priority 0) standby pages.
        /// Safer - preserves frequently-accessed cached data.
        #[arg(long)]
        low_priority: bool,

        /// Show detailed progress.
        #[arg(short, long)]
        verbose: bool,
    },

    /// Flush modified page list - write dirty pages to disk.
    ///
    /// Forces all modified (dirty) pages to be written to disk/pagefile.
    /// After flushing, these pages move to the standby list where they
    /// can then be purged. Best used before `purge-standby`.
    #[command(verbatim_doc_comment)]
    FlushModified {
        /// Show detailed progress.
        #[arg(short, long)]
        verbose: bool,
    },

    /// Empty process working sets - trim all processes.
    ///
    /// Forces all processes to release their working set pages.
    /// By default uses the kernel-level command which is faster and
    /// more thorough than per-process trimming.
    ///
    /// Use --exclude to protect specific processes (implies --per-process).
    #[command(verbatim_doc_comment)]
    EmptyWorkingsets {
        /// Use per-process trimming instead of kernel-level.
        /// Slower but shows individual process results.
        #[arg(long)]
        per_process: bool,

        /// Exclude processes by name (case-insensitive, .exe suffix optional).
        /// Can be specified multiple times: --exclude chrome --exclude firefox
        /// Implies --per-process since kernel-level trim cannot exclude.
        #[arg(long, value_name = "NAME")]
        exclude: Vec<String>,

        /// Show detailed progress.
        #[arg(short, long)]
        verbose: bool,
    },

    /// Flush file system cache - release cached file data.
    ///
    /// Tells Windows to release its file system cache, freeing the
    /// RAM used to cache recently-read files. Requires `SeIncreaseQuotaPrivilege`.
    #[command(verbatim_doc_comment)]
    FlushCache {
        /// Show detailed progress.
        #[arg(short, long)]
        verbose: bool,
    },

    /// Flush registry cache - write dirty hive pages to disk.
    ///
    /// Forces Windows to write all cached registry modifications to disk,
    /// freeing the RAM used for registry hive caching. Included automatically
    /// in aggressive and nuclear cleaning levels.
    #[command(verbatim_doc_comment)]
    FlushRegistry {
        /// Show detailed progress.
        #[arg(short, long)]
        verbose: bool,
    },

    /// Memory combining - deduplicate identical pages.
    ///
    /// Scans physical memory for identical pages and combines them using
    /// copy-on-write, freeing duplicate pages. Windows 10+ only.
    ///
    /// This can take several seconds on systems with lots of RAM.
    #[command(verbatim_doc_comment)]
    Combine {
        /// Show detailed progress.
        #[arg(short, long)]
        verbose: bool,
    },

    /// Monitor memory usage continuously with optional auto-clean.
    ///
    /// Watches memory usage at regular intervals and optionally triggers
    /// automatic cleaning when usage reaches a threshold.
    ///
    /// Press Ctrl+C to stop monitoring.
    #[command(verbatim_doc_comment)]
    Monitor {
        /// Check interval in seconds (1 to 86400).
        #[arg(short, long, default_value = "5", value_parser = clap::value_parser!(u64).range(1..=86_400))]
        interval: u64,

        /// Auto-clean when memory load reaches this percentage (1-100).
        /// Omit to only monitor without cleaning.
        #[arg(short, long, value_parser = clap::value_parser!(u32).range(1..=100))]
        threshold: Option<u32>,

        /// Cleaning level for auto-clean.
        #[arg(short, long, value_enum, default_value = "aggressive")]
        level: LevelArg,

        /// Cooldown in seconds after auto-clean before cleaning again [default: 2×interval]
        #[arg(short, long)]
        cooldown: Option<u64>,

        /// Show detailed output during auto-clean.
        #[arg(short, long)]
        verbose: bool,
    },

    /// Manage Windows right-click context menu integration.
    ///
    /// Install or uninstall "`MagicX` RAM Cleaner" entries in the
    /// Desktop and folder right-click context menus for quick access
    /// to cleaning operations without opening a terminal.
    ///
    /// Requires Administrator privileges (writes to `HKEY_CLASSES_ROOT`).
    #[command(verbatim_doc_comment)]
    ContextMenu {
        /// The action to perform.
        #[command(subcommand)]
        action: ContextMenuAction,
    },
}

/// Actions for the `context-menu` subcommand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
pub enum ContextMenuAction {
    /// Install context menu entries (creates registry keys under HKLM\Software\Classes).
    ///
    /// Adds a "`MagicX` RAM Cleaner" cascading submenu to the Desktop
    /// and folder background right-click menus with quick access to
    /// Quick Clean, Standard Clean, Deep Clean, Purge Standby List,
    /// and Memory Status.
    ///
    /// Existing entries are replaced cleanly (delete + recreate).
    /// Icons embedded in the executable are used for each entry.
    #[command(verbatim_doc_comment)]
    Install,

    /// Uninstall context menu entries (removes registry keys).
    ///
    /// Removes the "`MagicX` RAM Cleaner" submenu and all its entries
    /// from the Desktop and folder right-click menus.
    ///
    /// Idempotent - succeeds even if not currently installed.
    #[command(verbatim_doc_comment)]
    Uninstall,
}

/// Cleaning level as accepted on the command line.
///
/// Mirrors [`CleanLevel`]; kept separate so the engine does not depend on
/// the argument parser. The `--help` text is the shared level description
/// the GUI shows too, so the two cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LevelArg {
    /// The Gentle level.
    #[value(help = strings::levels::GENTLE_DESC)]
    Gentle,
    /// The Moderate level.
    #[value(help = strings::levels::MODERATE_DESC)]
    Moderate,
    /// The Aggressive level.
    #[value(help = strings::levels::AGGRESSIVE_DESC)]
    Aggressive,
    /// The Nuclear level.
    #[value(help = strings::levels::NUCLEAR_DESC)]
    Nuclear,
}

impl From<LevelArg> for CleanLevel {
    fn from(level: LevelArg) -> Self {
        match level {
            LevelArg::Gentle => Self::Gentle,
            LevelArg::Moderate => Self::Moderate,
            LevelArg::Aggressive => Self::Aggressive,
            LevelArg::Nuclear => Self::Nuclear,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn level_names_match_the_engine_display_names() {
        for arg in LevelArg::value_variants() {
            let name = arg.to_possible_value().map(|v| v.get_name().to_owned());
            assert_eq!(name, Some(CleanLevel::from(*arg).to_string()));
        }
    }
}
