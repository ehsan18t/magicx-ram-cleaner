//! # `MagicX` RAM Cleaner — Display & Formatting
//!
//! Beautiful terminal output for memory status and diagnostics.

use crate::cleaner::{CleanLevel, CleanResult};
use crate::stats::{MemoryListInfo, MemorySnapshot, format_bytes};
use colored::{ColoredString, Colorize};

/// Colour a memory load percentage: red (>85%), yellow (>60%), green (≤60%).
fn coloured_load(percent: u32) -> ColoredString {
    let text = format!("{percent}%");
    if percent > 85 {
        text.red().bold()
    } else if percent > 60 {
        text.yellow()
    } else {
        text.green()
    }
}

/// Print a comprehensive memory status report.
pub fn print_status(snapshot: &MemorySnapshot, list_info: Option<&MemoryListInfo>) {
    let page_size = snapshot.page_size;

    println!();
    println!(
        "{}",
        "╔══════════════════════════════════════════════════════════════╗"
            .cyan()
            .bold()
    );
    println!(
        "{}",
        "║             MagicX RAM Cleaner — System Status             ║"
            .cyan()
            .bold()
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════════════════════╝"
            .cyan()
            .bold()
    );
    println!();

    print_physical_memory(snapshot);

    if let Some(info) = list_info {
        print_memory_lists(info, page_size);
    }

    print_commit_and_pagefile(snapshot, page_size);
    print_kernel_and_system(snapshot, page_size);
    println!();
}

/// Print the physical memory section.
fn print_physical_memory(snapshot: &MemorySnapshot) {
    println!("{}", "─── Physical Memory ────────────────────".dimmed());
    println!(
        "  Memory Load:    {}",
        coloured_load(snapshot.memory_load_percent)
    );
    println!(
        "  Total:          {}",
        format_bytes(snapshot.total_physical).white().bold()
    );
    println!(
        "  Used:           {}",
        format_bytes(snapshot.used_physical).red()
    );
    println!(
        "  Available:      {}",
        format_bytes(snapshot.available_physical).green()
    );
}

/// Print memory page lists and standby priority breakdown.
fn print_memory_lists(info: &MemoryListInfo, page_size: u64) {
    println!();
    println!("{}", "─── Memory Page Lists ──────────────────".dimmed());
    println!(
        "  Zeroed:         {}  ({} pages)",
        format_bytes(info.zeroed_pages * page_size).dimmed(),
        info.zeroed_pages
    );
    println!(
        "  Free:           {}  ({} pages)",
        format_bytes(info.free_pages * page_size).green(),
        info.free_pages
    );
    println!(
        "  Modified:       {}  ({} pages)",
        format_bytes(info.modified_pages * page_size).yellow(),
        info.modified_pages
    );
    println!(
        "  Mod-NoWrite:    {}  ({} pages)",
        format_bytes(info.modified_no_write_pages * page_size).dimmed(),
        info.modified_no_write_pages
    );
    if info.bad_pages > 0 {
        println!(
            "  Bad:            {}  ({} pages)",
            format_bytes(info.bad_pages * page_size).red().bold(),
            info.bad_pages
        );
    }

    println!();
    println!("{}", "─── Standby List (by priority) ─────────".dimmed());
    let total_standby = info.total_standby_pages();
    let total_standby_bytes = total_standby * page_size;
    println!(
        "  Total Standby:  {}  ({} pages)",
        format_bytes(total_standby_bytes).yellow().bold(),
        total_standby
    );
    for (i, &count) in info.standby_pages.iter().enumerate() {
        if count > 0 {
            let ratio = if total_standby > 0 {
                count as f64 / total_standby as f64
            } else {
                0.0
            };
            let bar = "█".repeat((ratio * 30.0) as usize);
            let pct = ratio * 100.0;
            let priority_label = match i {
                0 => "Lowest  ",
                7 => "Highest ",
                _ => "        ",
            };
            println!(
                "  Priority {}: {:>12}  {:>5.1}%  {} {}",
                i,
                format_bytes(count * page_size),
                pct,
                bar.cyan(),
                priority_label.dimmed()
            );
        }
    }
}

/// Print commit charge and page file sections.
fn print_commit_and_pagefile(snapshot: &MemorySnapshot, page_size: u64) {
    println!();
    println!("{}", "─── Commit Charge ──────────────────────".dimmed());
    println!(
        "  Current:        {}  ({} pages)",
        format_bytes(snapshot.commit_total_pages * page_size).white(),
        snapshot.commit_total_pages
    );
    println!(
        "  Limit:          {}  ({} pages)",
        format_bytes(snapshot.commit_limit_pages * page_size).dimmed(),
        snapshot.commit_limit_pages
    );
    println!(
        "  Peak:           {}  ({} pages)",
        format_bytes(snapshot.commit_peak_pages * page_size).yellow(),
        snapshot.commit_peak_pages
    );
    println!("  Usage:          {:.1}%", snapshot.commit_percent());

    println!();
    println!("{}", "─── Page File ──────────────────────────".dimmed());
    println!(
        "  Total:          {}",
        format_bytes(snapshot.total_page_file)
    );
    println!(
        "  Available:      {}",
        format_bytes(snapshot.available_page_file).green()
    );
    println!(
        "  Used:           {}",
        format_bytes(
            snapshot
                .total_page_file
                .saturating_sub(snapshot.available_page_file)
        )
        .red()
    );
}

/// Print kernel memory pools and system counters.
fn print_kernel_and_system(snapshot: &MemorySnapshot, page_size: u64) {
    println!();
    println!("{}", "─── Kernel Memory Pools ────────────────".dimmed());
    println!(
        "  Paged Pool:     {}",
        format_bytes(snapshot.kernel_paged_pages * page_size)
    );
    println!(
        "  Non-Paged Pool: {}",
        format_bytes(snapshot.kernel_nonpaged_pages * page_size)
    );

    println!();
    println!("{}", "─── System Counters ────────────────────".dimmed());
    println!("  Processes:      {}", snapshot.process_count);
    println!("  Threads:        {}", snapshot.thread_count);
    println!("  Handles:        {}", snapshot.handle_count);
    println!("  Page Size:      {} bytes", snapshot.page_size);
}

/// Print a compact one-line memory summary (for monitoring mode).
pub fn print_compact_status(snapshot: &MemorySnapshot) {
    let load_str = coloured_load(snapshot.memory_load_percent);

    let now = local_now();
    println!(
        "[{}] Load: {} | Used: {} | Avail: {} | Commit: {:.0}%",
        now.dimmed(),
        load_str,
        format_bytes(snapshot.used_physical).red(),
        format_bytes(snapshot.available_physical).green(),
        snapshot.commit_percent(),
    );
}

/// Get local date and time as `YYYY-MM-DD HH:MM:SS` string using Win32 `GetLocalTime`.
fn local_now() -> String {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: SYSTEMTIME is a plain data struct; zeroed is a valid initial state.
    // GetLocalTime writes to the provided pointer and cannot fail.
    unsafe {
        let mut st: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&raw mut st);
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
        )
    }
}

// ─── Application-level Display Functions ─────────────────────────────────────

/// Print the `MagicX` ASCII art banner and version.
pub fn print_banner() {
    println!(
        "{}",
        r"
  __  __             _       __  __
 |  \/  | __ _  __ _(_) ___ \ \/ /
 | |\/| |/ _` |/ _` | |/ __| \  /
 | |  | | (_| | (_| | | (__  /  \
 |_|  |_|\__,_|\__, |_|\___/_/\_\
               |___/
"
        .cyan()
        .bold()
    );
    println!(
        "  {} {}\n",
        "RAM Cleaner".white().bold(),
        format!("v{}", env!("CARGO_PKG_VERSION")).dimmed()
    );
}

/// Print the "Starting X clean..." header before a smart clean run.
pub fn print_clean_start(level: CleanLevel) {
    println!(
        "\n{} Starting {} clean...\n",
        "⚡".yellow(),
        level.to_string().bold()
    );
}

/// Print the result of a single cleaning operation.
///
/// Uses the before/after data stored in [`CleanResult`] (measured with kernel settle).
pub fn print_single_result(result: &CleanResult) {
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
            format_bytes(result.freed_bytes as u64).green().bold()
        );
    }
    println!(
        "  Available: {} -> {}",
        format_bytes(result.available_before).yellow(),
        format_bytes(result.available_after).green()
    );
    println!(
        "  Load: {}% -> {}%\n",
        result.load_before, result.load_after
    );
}

/// Print a formatted summary of all cleaning results with before/after comparison.
pub fn print_clean_summary(
    results: &[CleanResult],
    before: &MemorySnapshot,
    after: &MemorySnapshot,
    total_freed: i64,
) {
    println!("{}", "─── Cleaning Summary ───────────────────".dimmed());
    println!();

    for r in results {
        let status = if r.success {
            "✓".green().bold().to_string()
        } else {
            "✗".red().bold().to_string()
        };
        let freed_str = match r.freed_bytes.cmp(&0) {
            std::cmp::Ordering::Greater => format!("+{}", format_bytes(r.freed_bytes as u64))
                .green()
                .to_string(),
            std::cmp::Ordering::Less => format!("-{}", format_bytes(r.freed_bytes.unsigned_abs()))
                .yellow()
                .to_string(),
            std::cmp::Ordering::Equal => "0 B".dimmed().to_string(),
        };

        println!(
            "  {} {:<35} {:>12}   {}",
            status,
            r.operation,
            freed_str,
            r.message.dimmed()
        );
    }

    println!();
    println!("{}", "─── Memory Before/After ────────────────".dimmed());
    println!(
        "  {} Used:      {} → {}",
        "▸".cyan(),
        format_bytes(before.used_physical).red(),
        format_bytes(after.used_physical).green()
    );
    println!(
        "  {} Available: {} → {}",
        "▸".cyan(),
        format_bytes(before.available_physical).yellow(),
        format_bytes(after.available_physical).green()
    );
    println!(
        "  {} Load:      {}% → {}%",
        "▸".cyan(),
        before.memory_load_percent.to_string().red(),
        after.memory_load_percent.to_string().green()
    );

    if total_freed > 0 {
        println!(
            "\n  {} Total freed: {}",
            "★".yellow().bold(),
            format_bytes(total_freed as u64).green().bold()
        );
    } else {
        println!(
            "\n  {} Net change: {} (already clean or pages re-faulted)",
            "•".dimmed(),
            format_bytes(total_freed.unsigned_abs()).yellow()
        );
    }
    println!();
}
