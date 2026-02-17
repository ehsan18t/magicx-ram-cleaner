//! # `MagicX` RAM Cleaner — Display & Formatting
//!
//! Beautiful terminal output for memory status and diagnostics.

use crate::stats::{MemoryListInfo, MemorySnapshot, format_bytes};
use colored::Colorize;

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
    let load_color = if snapshot.memory_load_percent > 85 {
        format!("{}%", snapshot.memory_load_percent).red().bold()
    } else if snapshot.memory_load_percent > 60 {
        format!("{}%", snapshot.memory_load_percent).yellow()
    } else {
        format!("{}%", snapshot.memory_load_percent).green()
    };
    println!("  Memory Load:    {load_color}");
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
            let bar_len = if total_standby > 0 {
                ((count as f64 / total_standby as f64) * 30.0) as usize
            } else {
                0
            };
            let bar = "█".repeat(bar_len);
            let pct = if total_standby > 0 {
                (count as f64 / total_standby as f64) * 100.0
            } else {
                0.0
            };
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
    let load_str = if snapshot.memory_load_percent > 85 {
        format!("{}%", snapshot.memory_load_percent)
            .red()
            .bold()
            .to_string()
    } else if snapshot.memory_load_percent > 60 {
        format!("{}%", snapshot.memory_load_percent)
            .yellow()
            .to_string()
    } else {
        format!("{}%", snapshot.memory_load_percent)
            .green()
            .to_string()
    };

    let now = local_now();
    println!(
        "[{}] Load: {} | Used: {} | Avail: {} | Commit: {:.0}%",
        now.dimmed(),
        load_str,
        format_bytes(snapshot.used_physical),
        format_bytes(snapshot.available_physical).green(),
        snapshot.commit_percent(),
    );
}

/// Get local time as HH:MM:SS string using Win32 `GetLocalTime`.
fn local_now() -> String {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: SYSTEMTIME is a plain data struct; zeroed is a valid initial state.
    // GetLocalTime writes to the provided pointer and cannot fail.
    unsafe {
        let mut st: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&raw mut st);
        format!("{:02}:{:02}:{:02}", st.wHour, st.wMinute, st.wSecond)
    }
}
