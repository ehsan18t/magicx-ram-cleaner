//! Individual cleaning operations.

use anyhow::Result;

use super::Cleaner;
use super::progress::Progress;
use super::report::CleanResult;
use super::settle::SettleMode;
use crate::platform::nt::{self, MemoryListCommand, NtStatus};

/// Display strings for a memory-list command:
/// `(operation_name, success_message, progress_label)`.
const fn command_labels(command: MemoryListCommand) -> (&'static str, &'static str, &'static str) {
    match command {
        MemoryListCommand::CaptureAccessedBits | MemoryListCommand::CaptureAndResetAccessedBits => {
            (
                "Capture Accessed Bits",
                "PTE accessed bits captured",
                "Capturing PTE accessed bits...",
            )
        }
        MemoryListCommand::EmptyWorkingSets => (
            "Empty Working Sets (Kernel)",
            "All process working sets emptied via kernel",
            "Emptying working sets (kernel-level)...",
        ),
        MemoryListCommand::FlushModifiedList => (
            "Flush Modified List",
            "Modified pages flushed to disk",
            "Flushing modified page list...",
        ),
        MemoryListCommand::PurgeLowPriorityStandbyList => (
            "Purge Low-Priority Standby",
            "Low-priority standby pages purged",
            "Purging low-priority standby pages...",
        ),
        MemoryListCommand::PurgeStandbyList => (
            "Purge All Standby",
            "All standby pages purged",
            "Purging all standby pages...",
        ),
    }
}

/// Failure message for an `NTSTATUS` returned by `NtSetSystemInformation`.
pub(super) fn ntstatus_failure(call: &str, status: NtStatus) -> String {
    format!(
        "{call} failed: 0x{:08X}: {}",
        status as u32,
        nt::ntstatus_message(status)
    )
}

/// Normalise an exclusion name: lowercase, without a trailing `.exe`.
fn normalise_exclusion(name: &str) -> String {
    let lower = name.to_lowercase();
    lower.strip_suffix(".exe").unwrap_or(&lower).to_owned()
}

/// Whether a process name matches any entry of a normalised exclusion list.
///
/// Comparison is case-insensitive and matches with or without `.exe`.
fn is_excluded(exe_name: &str, normalised_excludes: &[String]) -> bool {
    let lower = exe_name.to_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    normalised_excludes.iter().any(|ex| ex == stem)
}

impl Cleaner<'_> {
    /// Run a memory-list command with before/after measurement.
    pub(super) fn memory_list_op(
        &mut self,
        command: MemoryListCommand,
        settle: SettleMode,
    ) -> Result<CleanResult> {
        let (name, success_msg, label) = command_labels(command);
        self.report(Progress::Started { label });

        let before = self.sys.snapshot()?;
        let start = std::time::Instant::now();

        match self.sys.memory_command(command) {
            Ok(()) => {
                let after = self.wait_for_settle(settle)?;
                Ok(CleanResult::success(
                    name,
                    success_msg,
                    &before,
                    &after,
                    start.elapsed(),
                ))
            }
            Err(status) => Ok(CleanResult::failure(
                name,
                ntstatus_failure("NtSetSystemInformation", status),
                &before,
            )),
        }
    }

    /// **Purge ALL standby pages** (priorities 0-7).
    ///
    /// Removes all cached pages from RAM; programs re-read data from disk
    /// afterwards, causing temporary I/O spikes.
    pub fn purge_standby(&mut self) -> Result<CleanResult> {
        self.memory_list_op(MemoryListCommand::PurgeStandbyList, SettleMode::Full)
    }

    /// **Purge low-priority (priority 0) standby pages only.**
    ///
    /// Frees pages Windows would reclaim first anyway, with minimal impact on
    /// cache performance.
    pub fn purge_standby_low_priority(&mut self) -> Result<CleanResult> {
        self.memory_list_op(
            MemoryListCommand::PurgeLowPriorityStandbyList,
            SettleMode::Full,
        )
    }

    /// **Flush the modified page list**: write dirty pages to disk/pagefile
    /// so they move to the standby list, where a purge can free them.
    pub fn flush_modified(&mut self) -> Result<CleanResult> {
        self.memory_list_op(MemoryListCommand::FlushModifiedList, SettleMode::Full)
    }

    /// **Empty all process working sets (kernel-level).**
    ///
    /// One kernel call that reaches every process, including protected and
    /// system processes that cannot be opened from user mode.
    pub fn empty_working_sets(&mut self) -> Result<CleanResult> {
        self.memory_list_op(MemoryListCommand::EmptyWorkingSets, SettleMode::Full)
    }

    /// **Empty working sets per process**, skipping excluded names.
    ///
    /// Processes whose executable name (case-insensitive, `.exe` optional)
    /// matches an entry of `exclude_names` are skipped, as is this process.
    /// Protected/system processes cannot be opened and count as skipped.
    pub fn empty_working_sets_per_process(
        &mut self,
        exclude_names: &[String],
    ) -> Result<CleanResult> {
        self.per_process_trim(exclude_names, SettleMode::Full)
    }

    /// Per-process trimming with a configurable settle mode.
    pub(super) fn per_process_trim(
        &mut self,
        exclude_names: &[String],
        settle: SettleMode,
    ) -> Result<CleanResult> {
        self.report(Progress::Started {
            label: "Emptying working sets per-process...",
        });

        let before = self.sys.snapshot()?;
        let start = std::time::Instant::now();
        let own_pid = self.sys.own_pid();
        let exclusions: Vec<String> = exclude_names
            .iter()
            .map(|n| normalise_exclusion(n))
            .collect();

        let (mut trimmed, mut skipped, mut excluded) = (0u32, 0u32, 0u32);
        for entry in self.sys.processes()? {
            if entry.pid == own_pid {
                continue;
            }
            if is_excluded(&entry.name, &exclusions) {
                excluded += 1;
                self.report(Progress::Excluded {
                    name: entry.name,
                    pid: entry.pid,
                });
            } else if self.sys.trim_process(entry.pid) {
                trimmed += 1;
            } else {
                skipped += 1;
            }
        }

        let after = self.wait_for_settle(settle)?;

        let mut message =
            format!("Trimmed {trimmed} processes, {skipped} skipped (protected/system)");
        if excluded > 0 {
            use std::fmt::Write;
            // Writing to a String cannot fail.
            let _ = write!(message, ", {excluded} excluded by name");
        }

        Ok(CleanResult::success(
            "Empty Working Sets (Per-Process)",
            message,
            &before,
            &after,
            start.elapsed(),
        ))
    }

    /// **Trim the system file cache** (requires `SeIncreaseQuotaPrivilege`).
    ///
    /// Targets the file cache directly, often the biggest source of standby
    /// pages.
    pub fn flush_file_cache(&mut self) -> Result<CleanResult> {
        self.file_cache_op(SettleMode::Full)
    }

    /// File cache trim with a configurable settle mode.
    pub(super) fn file_cache_op(&mut self, settle: SettleMode) -> Result<CleanResult> {
        self.report(Progress::Started {
            label: "Flushing file system cache...",
        });

        let before = self.sys.snapshot()?;
        let start = std::time::Instant::now();

        if let Err(err) = self.sys.flush_file_cache() {
            return Ok(CleanResult::failure(
                "Flush File Cache",
                format!(
                    "SetSystemFileCacheSize failed (error {err}). Need SeIncreaseQuotaPrivilege."
                ),
                &before,
            ));
        }

        let after = self.wait_for_settle(settle)?;
        Ok(CleanResult::success(
            "Flush File Cache",
            "File system cache flushed successfully",
            &before,
            &after,
            start.elapsed(),
        ))
    }

    /// **Write the registry cache to disk**, freeing the modified memory held
    /// by dirty hive pages.
    pub fn flush_registry_cache(&mut self) -> Result<CleanResult> {
        self.registry_op(SettleMode::Full)
    }

    /// Registry flush with a configurable settle mode.
    pub(super) fn registry_op(&mut self, settle: SettleMode) -> Result<CleanResult> {
        self.report(Progress::Started {
            label: "Flushing registry cache to disk...",
        });

        let before = self.sys.snapshot()?;
        let start = std::time::Instant::now();

        match self.sys.flush_registry() {
            Ok(()) => {
                let after = self.wait_for_settle(settle)?;
                Ok(CleanResult::success(
                    "Flush Registry Cache",
                    "Registry hive cache flushed to disk",
                    &before,
                    &after,
                    start.elapsed(),
                ))
            }
            Err(status) => Ok(CleanResult::failure(
                "Flush Registry Cache",
                ntstatus_failure(
                    "NtSetSystemInformation(SystemRegistryReconciliationInformation)",
                    status,
                ),
                &before,
            )),
        }
    }

    /// **Combine identical memory pages** (copy-on-write deduplication,
    /// Windows 10+). Scans all memory, so it can take several seconds.
    pub fn combine_memory(&mut self) -> Result<CleanResult> {
        self.combine_op(SettleMode::Full)
    }

    /// Page combining with a configurable settle mode.
    pub(super) fn combine_op(&mut self, settle: SettleMode) -> Result<CleanResult> {
        self.report(Progress::Started {
            label: "Running memory page combining...",
        });

        let before = self.sys.snapshot()?;
        let start = std::time::Instant::now();

        match self.sys.combine_pages() {
            Ok(pages_combined) => {
                let after = self.wait_for_settle(settle)?;
                Ok(CleanResult::success(
                    "Memory Combining",
                    format!("Pages combined: {pages_combined}"),
                    &before,
                    &after,
                    start.elapsed(),
                ))
            }
            Err(status) => Ok(CleanResult::failure(
                "Memory Combining",
                ntstatus_failure(
                    "NtSetSystemInformation(SystemCombinePhysicalMemoryInformation)",
                    status,
                ),
                &before,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalised(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| normalise_exclusion(n)).collect()
    }

    #[test]
    fn is_excluded_case_insensitive() {
        let excludes = normalised(&["chrome", "firefox"]);
        assert!(is_excluded("chrome.exe", &excludes));
        assert!(is_excluded("Chrome.EXE", &excludes));
        assert!(is_excluded("FIREFOX.exe", &excludes));
        assert!(!is_excluded("notepad.exe", &excludes));
    }

    #[test]
    fn is_excluded_empty_list() {
        assert!(
            !is_excluded("anything.exe", &[]),
            "nothing should be excluded with an empty list"
        );
    }

    #[test]
    fn is_excluded_without_exe_suffix() {
        assert!(
            is_excluded("notepad", &normalised(&["notepad"])),
            "should match process name without .exe suffix"
        );
    }

    #[test]
    fn is_excluded_with_exe_suffix_in_list() {
        let excludes = normalised(&["chrome.exe"]);
        assert!(is_excluded("chrome.exe", &excludes));
        assert!(is_excluded("Chrome.EXE", &excludes));
    }

    #[test]
    fn ntstatus_failure_names_call_and_status() {
        let msg = ntstatus_failure("NtSetSystemInformation", 0xC000_0022_u32 as i32);
        assert!(msg.starts_with("NtSetSystemInformation failed: 0xC0000022: "));
        assert!(msg.contains("ACCESS_DENIED"));
    }
}
