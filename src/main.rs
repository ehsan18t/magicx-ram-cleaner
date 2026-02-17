//! # MagicX RAM Cleaner
//!
//! The most powerful Windows RAM cleaner CLI tool.
//! Surpasses EmptyStandbyList with granular control over every memory subsystem.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    MagicX RAM Cleaner                       │
//! ├─────────────┬──────────────┬──────────────┬────────────────┤
//! │   CLI       │  Cleaner     │  Monitor     │  Display       │
//! │  (clap)     │  Engine      │  Loop        │  Formatting    │
//! ├─────────────┴──────────────┴──────────────┴────────────────┤
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
//! ## Why MagicX is better than EmptyStandbyList
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

mod cleaner;
mod display;
mod monitor;
mod ntapi;
mod privilege;
mod stats;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;

use cleaner::CleanLevel;

/// MagicX RAM Cleaner — The most powerful Windows RAM cleaner.
///
/// Surpasses EmptyStandbyList with granular control, smart cleaning levels,
/// detailed diagnostics, and monitoring with auto-clean.
///
/// REQUIRES: Run as Administrator (right-click → Run as administrator).
#[derive(Parser)]
#[command(
    name = "magicx-ram-cleaner",
    version,
    about = "MagicX RAM Cleaner — The world's most powerful Windows RAM cleaner CLI",
    long_about = "\
MagicX RAM Cleaner is a command-line tool that provides unmatched control over \
Windows memory management. It goes beyond EmptyStandbyList by offering:\n\
\n\
  • Smart cleaning with 4 levels: gentle, moderate, aggressive, nuclear\n\
  • Individual control over every memory operation\n\
  • Detailed memory diagnostics with per-priority standby breakdown\n\
  • Continuous monitoring with automatic cleaning at configurable thresholds\n\
  • File system cache management\n\
  • Memory page combining/deduplication\n\
  • Optimal operation ordering for maximum RAM recovery\n\
  • Before/after reporting showing exactly how much RAM was freed\n\
\n\
REQUIRES: Must be run as Administrator.\n\
\n\
QUICK START:\n\
  magicx-ram-cleaner clean                    # Smart clean (aggressive)\n\
  magicx-ram-cleaner clean --level gentle     # Minimal impact clean\n\
  magicx-ram-cleaner status                   # Show memory usage\n\
  magicx-ram-cleaner monitor --threshold 80   # Auto-clean at 80% usage\n\
",
    after_help = "\
EXAMPLES:\n\
  # Quick aggressive clean (default — recommended for most users)\n\
  magicx-ram-cleaner clean\n\
\n\
  # Gentle clean — minimal impact, good for gaming\n\
  magicx-ram-cleaner clean --level gentle\n\
\n\
  # Nuclear clean — maximum RAM recovery\n\
  magicx-ram-cleaner clean --level nuclear -v\n\
\n\
  # Just purge the standby list (like EmptyStandbyList)\n\
  magicx-ram-cleaner purge-standby\n\
\n\
  # Only purge low-priority standby (safest)\n\
  magicx-ram-cleaner purge-standby --low-priority\n\
\n\
  # Flush modified pages to disk first, then purge standby\n\
  magicx-ram-cleaner flush-modified\n\
  magicx-ram-cleaner purge-standby\n\
\n\
  # Show detailed memory status including standby list breakdown\n\
  magicx-ram-cleaner status --detailed\n\
\n\
  # Monitor memory every 5 seconds, auto-clean at 85% usage\n\
  magicx-ram-cleaner monitor --interval 5 --threshold 85\n\
\n\
  # Export memory status as JSON\n\
  magicx-ram-cleaner status --json\n\
\n\
  # Trim file system cache (frees cached file data from RAM)\n\
  magicx-ram-cleaner flush-cache\n\
\n\
  # Empty all process working sets (kernel-level)\n\
  magicx-ram-cleaner empty-workingsets\n\
\n\
  # Empty working sets per-process (shows details)\n\
  magicx-ram-cleaner empty-workingsets --per-process\n\
\n\
  # Run memory page combining (Windows 10+ only)\n\
  magicx-ram-cleaner combine\n\
"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
    Status {
        /// Show detailed memory list information (standby priorities, modified pages, etc.).
        /// Requires SeProfileSingleProcessPrivilege.
        #[arg(short, long)]
        detailed: bool,

        /// Output as JSON for scripting/automation.
        #[arg(short, long)]
        json: bool,
    },

    /// Purge standby list — equivalent to EmptyStandbyList but better.
    ///
    /// Removes cached pages from the standby list, making them available
    /// for new allocations. By default purges ALL priorities.
    ///
    /// Tip: Run `flush-modified` first for maximum effect.
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
    /// RAM used to cache recently-read files. Requires SeIncreaseQuotaPrivilege.
    ///
    /// This is unique to MagicX — EmptyStandbyList can't do this.
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
    /// Unique to MagicX.
    Combine {
        /// Show detailed progress.
        #[arg(short, long)]
        verbose: bool,
    },

    /// Monitor memory usage continuously with optional auto-clean.
    ///
    /// Watches memory usage at regular intervals and optionally triggers
    /// automatic cleaning when usage exceeds a threshold.
    Monitor {
        /// Check interval in seconds [default: 5]
        #[arg(short, long, default_value = "5")]
        interval: u64,

        /// Auto-clean when memory load exceeds this percentage (0-100).
        /// Omit to only monitor without cleaning.
        #[arg(short, long)]
        threshold: Option<u32>,

        /// Cleaning level for auto-clean [default: aggressive]
        #[arg(short, long, value_enum, default_value = "aggressive")]
        level: CleanLevel,

        /// Show detailed output during auto-clean.
        #[arg(short, long)]
        verbose: bool,
    },
}

fn main() -> Result<()> {
    // Enable ANSI virtual terminal processing on Windows consoles
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{
            ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE,
            SetConsoleMode,
        };
        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut mode: u32 = 0;
            if GetConsoleMode(handle, &mut mode) != 0 {
                let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
            }
        }
    }

    let cli = Cli::parse();

    // Suppress banner when JSON output is requested so stdout stays machine-parseable
    let is_json = matches!(cli.command, Commands::Status { json: true, .. });
    if !is_json {
        print_banner();
    }

    // Check for admin privileges
    check_admin()?;

    // Enable required privileges
    privilege::enable_all_privileges().context(
        "Failed to enable privileges. Make sure you're running as Administrator.\n\
         Right-click the terminal/exe → 'Run as administrator'",
    )?;

    match cli.command {
        Commands::Clean { level, verbose } => {
            cleaner::smart_clean(level, verbose)?;
        }

        Commands::Status { detailed, json } => {
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
        }

        Commands::PurgeStandby {
            low_priority,
            verbose,
        } => {
            let result = if low_priority {
                cleaner::purge_standby_low_priority(verbose)?
            } else {
                cleaner::purge_standby_all(verbose)?
            };
            print_single_result(&result);
        }

        Commands::FlushModified { verbose } => {
            let result = cleaner::flush_modified_list(verbose)?;
            print_single_result(&result);
        }

        Commands::EmptyWorkingsets {
            per_process,
            verbose,
        } => {
            let result = if per_process {
                cleaner::empty_working_sets_per_process(verbose, &[])?
            } else {
                cleaner::empty_working_sets_kernel(verbose)?
            };
            print_single_result(&result);
        }

        Commands::FlushCache { verbose } => {
            let result = cleaner::flush_file_cache(verbose)?;
            print_single_result(&result);
        }

        Commands::Combine { verbose } => {
            let result = cleaner::combine_memory(verbose)?;
            print_single_result(&result);
        }

        Commands::Monitor {
            interval,
            threshold,
            level,
            verbose,
        } => {
            if let Some(t) = threshold {
                if t > 100 {
                    anyhow::bail!("Threshold must be 0-100, got {}", t);
                }
            }
            monitor::run_monitor(interval, threshold, level, verbose)?;
        }
    }

    Ok(())
}

fn print_banner() {
    println!(
        "{}",
        r#"
  __  __             _       __  __
 |  \/  | __ _  __ _(_) ___ \ \/ /
 | |\/| |/ _` |/ _` | |/ __| \  /
 | |  | | (_| | (_| | | (__  /  \
 |_|  |_|\__,_|\__, |_|\___/_/\_\
               |___/
"#
        .cyan()
        .bold()
    );
    println!(
        "  {} {}\n",
        "RAM Cleaner".white().bold(),
        format!("v{}", env!("CARGO_PKG_VERSION")).dimmed()
    );
}

/// Check if we're actually running with elevated (Administrator) privileges.
///
/// Uses `CheckTokenMembership` with the built-in Administrators group SID
/// to verify true elevation, not just token access.
fn check_admin() -> Result<()> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
    use windows_sys::Win32::Security::CheckTokenMembership;

    // Well-known SID string for BUILTIN\Administrators: S-1-5-32-544
    let sid_str: Vec<u16> = "S-1-5-32-544\0".encode_utf16().collect();
    let mut sid: *mut std::ffi::c_void = std::ptr::null_mut();

    let ok = unsafe { ConvertStringSidToSidW(sid_str.as_ptr(), &mut sid) };
    if ok == 0 {
        anyhow::bail!(
            "Cannot verify admin status. Please run as Administrator.\n\
             Right-click Command Prompt or PowerShell → 'Run as administrator'"
        );
    }

    let mut is_member: i32 = 0;
    let check_ok = unsafe { CheckTokenMembership(std::ptr::null_mut(), sid, &mut is_member) };
    unsafe { LocalFree(sid) };

    if check_ok == 0 || is_member == 0 {
        anyhow::bail!(
            "Not running as Administrator.\n\
             Right-click Command Prompt or PowerShell → 'Run as administrator'"
        );
    }

    Ok(())
}

/// Print the result of a single cleaning operation.
/// Uses the before/after data stored in CleanResult (measured with kernel settle).
fn print_single_result(result: &cleaner::CleanResult) {
    println!();
    let status = if result.success {
        "OK".green().bold()
    } else {
        "FAIL".red().bold()
    };

    println!("  [{}] {}", status, result.operation.bold());
    println!("  {}", result.message.dimmed());

    if result.freed_bytes > 0 {
        println!(
            "  Freed: {}",
            stats::format_bytes(result.freed_bytes as u64)
                .green()
                .bold()
        );
    }
    println!(
        "  Available: {} -> {}",
        stats::format_bytes(result.available_before).yellow(),
        stats::format_bytes(result.available_after).green()
    );
    println!(
        "  Load: {}% -> {}%\n",
        result.load_before, result.load_after
    );
}
