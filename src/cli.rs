//! # `MagicX` RAM Cleaner — CLI Definitions
//!
//! Clap-based command-line interface: parser struct, subcommands,
//! styling, and help text constants. Separated from `main.rs` so that
//! CLI surface area can evolve independently of application wiring.

use clap::builder::styling::{AnsiColor, Styles};
use clap::{Parser, Subcommand};

use crate::cleaner::CleanLevel;

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
MagicX RAM Cleaner \x1b[90m—\x1b[0m The world's most powerful Windows RAM cleaner CLI.

\
Provides unmatched control over Windows memory management, surpassing \
EmptyStandbyList with granular control over every memory subsystem.

\
\x1b[1;36mFEATURES:\x1b[0m
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
\x1b[1;36mCLEANING LEVELS:\x1b[0m
  \
\x1b[32mgentle\x1b[0m      Safe — purge low-priority standby only \x1b[90m(good for gaming)\x1b[0m
  \
\x1b[33mmoderate\x1b[0m    Balanced — working sets + low-priority standby
  \
\x1b[1;33maggressive\x1b[0m  Full clean: cache + working sets + modified + standby \x1b[1;33m[DEFAULT]\x1b[0m
  \
\x1b[35mnuclear\x1b[0m     Maximum — everything + memory combining + 2nd pass

\
\x1b[1;36mQUICK START:\x1b[0m
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
\x1b[1;36mKEY CONCEPTS:\x1b[0m
  \
\x1b[1mStandby List\x1b[0m     Cached pages in RAM — freed first when memory is needed
  \
\x1b[1mWorking Sets\x1b[0m     Pages actively mapped by each running process
  \
\x1b[1mModified Pages\x1b[0m   Dirty pages not yet written to disk or pagefile
  \
\x1b[1mFile Cache\x1b[0m       RAM used by Windows to cache recent file I/O
  \
\x1b[1mPage Combining\x1b[0m   Deduplicating identical pages via copy-on-write

\
\x1b[1;36mEXIT CODES:\x1b[0m
  \
\x1b[32m0\x1b[0m  All operations completed successfully
  \
\x1b[33m1\x1b[0m  One or more operations failed
  \
\x1b[31m2\x1b[0m  Invalid arguments or missing administrator privileges

\
\x1b[1;36mLEARN MORE:\x1b[0m
  \
Repository:  \x1b[36mhttps://github.com/ehsan18t/magicx-ram-cleaner\x1b[0m
  \
Run \x1b[32mmagicx-ram-cleaner <command> --help\x1b[0m for detailed command information.";

/// `MagicX` RAM Cleaner — The most powerful Windows RAM cleaner.
///
/// Surpasses `EmptyStandbyList` with granular control, smart cleaning levels,
/// detailed diagnostics, and monitoring with auto-clean.
///
/// REQUIRES: Run as Administrator (right-click → Run as administrator).
#[derive(Parser)]
#[command(
    name = "magicx-ram-cleaner",
    version,
    styles = STYLES,
    about = "MagicX RAM Cleaner — The world's most powerful Windows RAM cleaner CLI",
    long_about = LONG_ABOUT,
    after_help = AFTER_HELP_SHORT,
    after_long_help = AFTER_HELP_LONG,
)]
pub struct Cli {
    /// The subcommand to execute.
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Disable coloured terminal output.
    ///
    /// Useful for piping output to files or non-ANSI terminals.
    /// Also respects the `NO_COLOR` environment variable.
    #[arg(long, global = true)]
    pub no_color: bool,
}

/// Available CLI subcommands.
#[derive(Subcommand)]
pub enum Commands {
    /// Smart clean — the recommended way to free RAM.
    ///
    /// Runs multiple memory operations in optimal order based on the
    /// selected aggressiveness level. Shows before/after stats.
    ///
    /// Levels:
    ///   gentle     — Purge low-priority standby only (safe for gaming)
    ///   moderate   — Empty working sets + purge low-priority standby
    ///   aggressive — Full clean: cache + working sets + modified + standby (DEFAULT)
    ///   nuclear    — Everything + memory combining + second pass
    #[command(verbatim_doc_comment)]
    Clean {
        /// Cleaning aggressiveness level [default: aggressive]
        #[arg(short, long, value_enum, default_value = "aggressive")]
        level: CleanLevel,

        /// Show detailed progress of each operation.
        #[arg(short, long)]
        verbose: bool,
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
    },

    /// Purge standby list — equivalent to `EmptyStandbyList` but better.
    ///
    /// Removes cached pages from the standby list, making them available
    /// for new allocations. By default purges ALL priorities.
    ///
    /// Tip: Run `flush-modified` first, then `purge-standby` for maximum effect.
    #[command(verbatim_doc_comment)]
    PurgeStandby {
        /// Only purge low-priority (priority 0) standby pages.
        /// Safer — preserves frequently-accessed cached data.
        #[arg(long)]
        low_priority: bool,

        /// Show detailed progress.
        #[arg(short, long)]
        verbose: bool,
    },

    /// Flush modified page list — write dirty pages to disk.
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

    /// Empty process working sets — trim all processes.
    ///
    /// Forces all processes to release their working set pages.
    /// By default uses the kernel-level command which is faster and
    /// more thorough than per-process trimming.
    #[command(verbatim_doc_comment)]
    EmptyWorkingsets {
        /// Use per-process trimming instead of kernel-level.
        /// Slower but shows individual process results.
        #[arg(long)]
        per_process: bool,

        /// Show detailed progress.
        #[arg(short, long)]
        verbose: bool,
    },

    /// Flush file system cache — release cached file data.
    ///
    /// Tells Windows to release its file system cache, freeing the
    /// RAM used to cache recently-read files. Requires `SeIncreaseQuotaPrivilege`.
    ///
    /// This is unique to `MagicX` — `EmptyStandbyList` cannot do this.
    #[command(verbatim_doc_comment)]
    FlushCache {
        /// Show detailed progress.
        #[arg(short, long)]
        verbose: bool,
    },

    /// Memory combining — deduplicate identical pages.
    ///
    /// Scans physical memory for identical pages and combines them using
    /// copy-on-write, freeing duplicate pages. Windows 10+ only.
    ///
    /// This can take several seconds on systems with lots of RAM.
    /// Unique to `MagicX` — `EmptyStandbyList` cannot do this.
    #[command(verbatim_doc_comment)]
    Combine {
        /// Show detailed progress.
        #[arg(short, long)]
        verbose: bool,
    },

    /// Monitor memory usage continuously with optional auto-clean.
    ///
    /// Watches memory usage at regular intervals and optionally triggers
    /// automatic cleaning when usage exceeds a threshold.
    ///
    /// Press Ctrl+C to stop monitoring.
    #[command(verbatim_doc_comment)]
    Monitor {
        /// Check interval in seconds (minimum 1) [default: 5]
        #[arg(short, long, default_value = "5", value_parser = clap::value_parser!(u64).range(1..))]
        interval: u64,

        /// Auto-clean when memory load exceeds this percentage (1-100).
        /// Omit to only monitor without cleaning.
        #[arg(short, long, value_parser = clap::value_parser!(u32).range(1..=100))]
        threshold: Option<u32>,

        /// Cleaning level for auto-clean [default: aggressive]
        #[arg(short, long, value_enum, default_value = "aggressive")]
        level: CleanLevel,

        /// Show detailed output during auto-clean.
        #[arg(short, long)]
        verbose: bool,
    },
}
