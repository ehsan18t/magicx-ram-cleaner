//! # `MagicX` RAM Cleaner - Display & Formatting
//!
//! Terminal output helpers for memory status and cleaning diagnostics.

use crate::cleaner::{CleanLevel, CleanResult};
use crate::stats::{
    FileCacheSnapshot, MemoryListInfo, MemorySnapshot, ProcessMemoryInfo, format_bytes,
};
use crate::strings;
use colored::{ColoredString, Colorize};

/// Total display width for section dividers (matches the status box width).
const SECTION_WIDTH: usize = 40;

/// Build a `"─── Title ───..."` section divider from a title string.
///
/// The divider is always [`SECTION_WIDTH`] characters wide.  The title is
/// placed after a 4-char prefix (`"─── "`) and followed by enough `─`
/// characters to pad to the total width.
fn section_divider(title: &str) -> String {
    let prefix = "─── ";
    let body = format!("{prefix}{title} ");
    let padding = SECTION_WIDTH.saturating_sub(body.chars().count());
    format!("{body}{}", "─".repeat(padding))
}

/// Build a centred box header line (`║ ... ║`) for the status report.
///
/// The inner content is centred within a 62-character field (matching the
/// 64-char box border: `╔` + 62×`═` + `╗`).
fn box_header_line(title: &str) -> String {
    const INNER_WIDTH: usize = 62;
    let pad_total = INNER_WIDTH.saturating_sub(title.len());
    let pad_left = pad_total / 2;
    let pad_right = pad_total - pad_left;
    format!(
        "║{}{}{}║",
        " ".repeat(pad_left),
        title,
        " ".repeat(pad_right)
    )
}

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

/// Colour a commit charge percentage: red (>90%), yellow (>70%), green (≤70%).
fn coloured_commit(percent: f64) -> ColoredString {
    let text = format!("{percent:.1}%");
    if percent > 90.0 {
        text.red().bold()
    } else if percent > 70.0 {
        text.yellow()
    } else {
        text.green()
    }
}

/// Print a comprehensive memory status report to the terminal.
///
/// Renders a boxed header followed by sections for physical memory, page
/// lists (if available), file system cache (if available), commit charge,
/// page file, kernel memory pools, and system counters.
pub fn print_status(
    snapshot: &MemorySnapshot,
    list_info: Option<&MemoryListInfo>,
    file_cache: Option<&FileCacheSnapshot>,
) {
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
        box_header_line(strings::cli::STATUS_HEADER).cyan().bold()
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

    if let Some(fc) = file_cache {
        print_file_cache(fc);
    }

    print_commit_and_pagefile(snapshot, page_size);
    print_kernel_and_system(snapshot, page_size);
    println!();
}

/// Print the physical memory section.
fn print_physical_memory(snapshot: &MemorySnapshot) {
    println!(
        "{}",
        section_divider(strings::cli::SECTION_PHYSICAL).dimmed()
    );
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
    println!(
        "{}",
        section_divider(strings::cli::SECTION_PAGE_LISTS).dimmed()
    );
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
    println!(
        "{}",
        section_divider(strings::cli::SECTION_STANDBY).dimmed()
    );
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

/// Print file system cache information.
fn print_file_cache(fc: &FileCacheSnapshot) {
    println!();
    println!(
        "{}",
        section_divider(strings::cli::SECTION_FILE_CACHE).dimmed()
    );
    println!(
        "  Current:        {}",
        format_bytes(fc.current_size).white().bold()
    );
    println!("  Peak:           {}", format_bytes(fc.peak_size).yellow());
    if fc.minimum_working_set > 0 {
        println!(
            "  Min Limit:      {}",
            format_bytes(fc.minimum_working_set).dimmed()
        );
    }
    if fc.maximum_working_set > 0 {
        println!(
            "  Max Limit:      {}",
            format_bytes(fc.maximum_working_set).dimmed()
        );
    }
}

/// Print commit charge and page file sections.
fn print_commit_and_pagefile(snapshot: &MemorySnapshot, page_size: u64) {
    println!();
    println!("{}", section_divider(strings::cli::SECTION_COMMIT).dimmed());
    println!(
        "  Current:        {}  ({} pages)",
        format_bytes(snapshot.commit_total_pages * page_size)
            .white()
            .bold(),
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
    println!(
        "  Usage:          {}",
        coloured_commit(snapshot.commit_percent())
    );

    println!();
    println!(
        "{}",
        section_divider(strings::cli::SECTION_PAGE_FILE).dimmed()
    );
    println!(
        "  Total:          {}",
        format_bytes(snapshot.total_page_file).white()
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
    println!("{}", section_divider(strings::cli::SECTION_KERNEL).dimmed());
    println!(
        "  Paged Pool:     {}",
        format_bytes(snapshot.kernel_paged_pages * page_size).white()
    );
    println!(
        "  Non-Paged Pool: {}",
        format_bytes(snapshot.kernel_nonpaged_pages * page_size).white()
    );

    println!();
    println!("{}", section_divider(strings::cli::SECTION_SYSTEM).dimmed());
    println!(
        "  Processes:      {}",
        snapshot.process_count.to_string().dimmed()
    );
    println!(
        "  Threads:        {}",
        snapshot.thread_count.to_string().dimmed()
    );
    println!(
        "  Handles:        {}",
        snapshot.handle_count.to_string().dimmed()
    );
    println!(
        "  Page Size:      {} bytes",
        snapshot.page_size.to_string().dimmed()
    );
}

/// Print a compact one-line memory summary prefixed with the current timestamp.
///
/// Used by the monitoring loop to show periodic updates without flooding the
/// terminal.  Format: `[YYYY-MM-DD HH:MM:SS] Load: … | Used: … | Avail: …`.
pub fn print_compact_status(snapshot: &MemorySnapshot) {
    let load_str = coloured_load(snapshot.memory_load_percent);

    let now = local_now();
    println!(
        "[{}] Load: {} | Used: {} | Avail: {} | Commit: {}",
        now.dimmed(),
        load_str,
        format_bytes(snapshot.used_physical).red(),
        format_bytes(snapshot.available_physical).green(),
        coloured_commit(snapshot.commit_percent()),
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

/// Print the `MagicX` ASCII art banner and version tag.
///
/// Renders a cyan bold ASCII-art logo followed by the crate version read
/// from `CARGO_PKG_VERSION` at compile time.
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

/// Print the "Starting X clean…" header before a smart clean run.
///
/// Displays the [`CleanLevel`] name in bold so the user knows which
/// cleaning tier is about to execute.
pub fn print_clean_start(level: CleanLevel) {
    println!(
        "\n{} Starting {} Clean...\n",
        "⚡".yellow().bold(),
        level.title_case_name().bold()
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
        "  Available: {} → {}",
        format_bytes(result.available_before).yellow(),
        format_bytes(result.available_after).green()
    );
    println!(
        "  Load: {} → {}  (took {:.2}s)\n",
        coloured_load(result.load_before),
        coloured_load(result.load_after),
        result.elapsed_secs
    );
}

/// Print a ranked table of the top processes by private working set (physical RAM) usage.
///
/// The "Memory" column shows private working set - the portion of physical RAM
/// that belongs exclusively to the process, matching Task Manager's default
/// "Memory" column. "Working Set" includes shared pages (DLLs, mapped files).
pub fn print_top_processes(processes: &[ProcessMemoryInfo]) {
    if processes.is_empty() {
        return;
    }

    println!();
    println!(
        "{}",
        section_divider(strings::cli::SECTION_TOP_PROCESSES).dimmed()
    );
    let header = format!(
        "  {:<6} {:<30} {:>12} {:>12} {:>12}",
        "PID", "Process", "Memory", "Working Set", "Peak WS"
    );
    println!("{}", header.cyan().bold());
    println!("  {}", "─".repeat(76).dimmed());

    for p in processes {
        let mem = format!("{:>12}", format_bytes(p.private_working_set));
        let ws = format!("{:>12}", format_bytes(p.working_set));
        let peak = format!("{:>12}", format_bytes(p.peak_working_set));
        println!(
            "  {:<6} {:<30} {} {} {}",
            p.pid,
            truncate_name(&p.name, 30),
            mem.white().bold(),
            ws.dimmed(),
            peak.dimmed(),
        );
    }
    println!();
}

/// Truncate a string to `max_len` characters, adding an ellipsis if needed.
///
/// Uses [`char_indices`](str::char_indices) so the slice never lands inside a
/// multi-byte UTF-8 sequence.
fn truncate_name(name: &str, max_len: usize) -> String {
    if name.len() <= max_len {
        return name.to_string();
    }
    // Find the last char boundary that fits within (max_len -1) bytes,
    // leaving room for the '…' character.
    let boundary = name
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|&i| i < max_len)
        .last()
        .unwrap_or(0);
    format!("{}…", &name[..boundary])
}

/// Print a dry-run preview listing the operations that *would* be performed.
///
/// Shows the [`CleanLevel`] name, a numbered list of operations, and a
/// footer hint telling the user to remove the `--dry-run` flag to execute.
pub fn print_dry_run(level: CleanLevel, operations: &[&str]) {
    println!(
        "\n{} Dry run  -{} level ({} operations):\n",
        "🔍".dimmed(),
        level.title_case_name().bold(),
        operations.len()
    );
    for (i, op) in operations.iter().enumerate() {
        println!("  {}. {}", (i + 1).to_string().cyan(), op.white().bold());
    }
    println!("\n  {}", strings::cli::DRY_RUN_FOOTER.yellow());
    println!();
}

/// Print a formatted summary of all cleaning results with before/after comparison.
///
/// For each [`CleanResult`], shows a pass/fail status, freed bytes, and
/// elapsed time.  Then prints an overall before/after memory delta and
/// total freed amount.
pub fn print_clean_summary(
    results: &[CleanResult],
    before: &MemorySnapshot,
    after: &MemorySnapshot,
    total_freed: i64,
    total_elapsed_secs: f64,
) {
    println!(
        "{}",
        section_divider(strings::cli::SECTION_CLEAN_SUMMARY).dimmed()
    );
    println!();

    for r in results {
        let status = if r.success {
            "✓".green().bold().to_string()
        } else {
            "✗".red().bold().to_string()
        };
        let freed_str = match r.freed_bytes.cmp(&0) {
            std::cmp::Ordering::Greater => {
                format!("{:>12}", format!("+{}", format_bytes(r.freed_bytes as u64)))
                    .green()
                    .to_string()
            }
            std::cmp::Ordering::Less => format!(
                "{:>12}",
                format!("-{}", format_bytes(r.freed_bytes.unsigned_abs()))
            )
            .yellow()
            .to_string(),
            std::cmp::Ordering::Equal => format!("{:>12}", "0 B").dimmed().to_string(),
        };

        let elapsed_str = format!("{:>6}", format!("{:.2}s", r.elapsed_secs))
            .dimmed()
            .to_string();

        println!(
            "  {} {:<35} {}  {}  {}",
            status,
            r.operation,
            freed_str,
            elapsed_str,
            r.message.dimmed()
        );
    }

    println!();
    println!(
        "{}",
        section_divider(strings::cli::SECTION_BEFORE_AFTER).dimmed()
    );
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
        "  {} Load:      {} → {}",
        "▸".cyan(),
        coloured_load(before.memory_load_percent),
        coloured_load(after.memory_load_percent)
    );

    match total_freed.cmp(&0) {
        std::cmp::Ordering::Greater => {
            println!(
                "\n  {} Total freed: {}  (took {:.2}s)",
                "★".yellow().bold(),
                format_bytes(total_freed as u64).green().bold(),
                total_elapsed_secs
            );
        }
        std::cmp::Ordering::Equal => {
            println!(
                "\n  {} Net change: 0 B (already clean, took {:.2}s)",
                "•".dimmed(),
                total_elapsed_secs
            );
        }
        std::cmp::Ordering::Less => {
            println!(
                "\n  {} Net change: -{} (pages re-faulted during clean, took {:.2}s)",
                "•".dimmed(),
                format_bytes(total_freed.unsigned_abs()).yellow(),
                total_elapsed_secs
            );
        }
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_name_short_string() {
        assert_eq!(truncate_name("notepad.exe", 30), "notepad.exe");
    }

    #[test]
    fn truncate_name_exact_length() {
        let name = "a".repeat(30);
        assert_eq!(truncate_name(&name, 30), name);
    }

    #[test]
    fn truncate_name_long_string() {
        let name = "a".repeat(40);
        let result = truncate_name(&name, 30);
        assert!(
            result.len() <= 32, // 29 ASCII chars + '…' (3 bytes UTF-8)
            "truncated name too long: {} bytes",
            result.len()
        );
        assert!(result.ends_with('…'));
    }

    #[test]
    fn truncate_name_empty_string() {
        assert_eq!(truncate_name("", 30), "");
    }

    #[test]
    fn truncate_name_single_char() {
        assert_eq!(truncate_name("x", 30), "x");
    }

    #[test]
    fn truncate_name_max_len_one() {
        // Edge case: max_len = 1 with a long string
        let result = truncate_name("hello", 1);
        assert!(result.ends_with('…'), "should have ellipsis");
    }

    #[test]
    fn coloured_load_boundaries() {
        // Test the colour threshold boundaries
        let low = coloured_load(60);
        assert!(format!("{low}").contains("60%"));

        let mid = coloured_load(61);
        assert!(format!("{mid}").contains("61%"));

        let high = coloured_load(86);
        assert!(format!("{high}").contains("86%"));
    }

    #[test]
    fn section_divider_width() {
        let div = section_divider("Physical Memory");
        assert_eq!(
            div.chars().count(),
            SECTION_WIDTH,
            "divider should be {SECTION_WIDTH} chars wide, got {}",
            div.chars().count()
        );
    }

    #[test]
    fn section_divider_contains_title() {
        let div = section_divider("Commit Charge");
        assert!(
            div.contains("Commit Charge"),
            "divider should contain the title"
        );
    }

    #[test]
    fn section_divider_long_title_does_not_panic() {
        // Title longer than SECTION_WIDTH should not panic (padding saturates to 0)
        let long = "A".repeat(SECTION_WIDTH + 10);
        let div = section_divider(&long);
        assert!(div.contains(&long), "long title should still appear");
    }

    #[test]
    fn box_header_line_centred() {
        let line = box_header_line("Test");
        assert!(line.starts_with('║'), "line should start with ║");
        assert!(line.ends_with('║'), "line should end with ║");
        // Inner content: 62 chars between the borders
        let inner = &line[3..line.len() - 3]; // ║ is 3 bytes UTF-8
        assert_eq!(inner.len(), 62, "inner content should be 62 bytes");
    }

    #[test]
    fn box_header_line_uses_status_header_constant() {
        let line = box_header_line(crate::strings::cli::STATUS_HEADER);
        assert!(
            line.contains("MagicX RAM Cleaner - System Status"),
            "box header should contain the correct title with space after dash"
        );
    }
}
