//! The engine's view of the operating system.

use std::time::Duration;

use anyhow::Result;

use crate::memory::{MemorySnapshot, QuickMemoryReading};
use crate::platform::memory;
use crate::platform::nt::{self, MemoryListCommand, NtStatus};
use crate::platform::process::{self, ProcessEntry};

/// Everything the cleaning engine needs from the operating system.
///
/// [`WindowsMemory`] is the real implementation. The trait exists so the
/// engine's decisions (operation order, settling, leftover sweeps,
/// measurement) can be exercised against a simulated memory system.
pub trait MemorySystem {
    /// Capture a full memory snapshot, including the page-list breakdown
    /// when available.
    fn snapshot(&self) -> Result<MemorySnapshot>;

    /// Capture the cheap physical-memory reading used for settle polling.
    fn quick_reading(&self) -> Result<QuickMemoryReading>;

    /// Run a `SystemMemoryListInformation` command (purge, flush, empty).
    fn memory_command(&self, command: MemoryListCommand) -> Result<(), NtStatus>;

    /// Trim the system file cache; returns the Win32 error code on failure.
    fn flush_file_cache(&self) -> Result<(), u32>;

    /// Write dirty registry hive pages to disk.
    fn flush_registry(&self) -> Result<(), NtStatus>;

    /// Combine identical physical pages; returns the number combined.
    fn combine_pages(&self) -> Result<usize, NtStatus>;

    /// List running processes (without System Idle and System).
    fn processes(&self) -> Result<Vec<ProcessEntry>>;

    /// Empty one process's working set; `false` if it cannot be trimmed.
    fn trim_process(&self, pid: u32) -> bool;

    /// The ID of the process running the engine (never trimmed).
    fn own_pid(&self) -> u32;

    /// Wait between settle polls.
    fn sleep(&self, duration: Duration);
}

/// The live Windows memory system.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsMemory;

impl MemorySystem for WindowsMemory {
    fn snapshot(&self) -> Result<MemorySnapshot> {
        MemorySnapshot::capture()
    }

    fn quick_reading(&self) -> Result<QuickMemoryReading> {
        QuickMemoryReading::capture()
    }

    fn memory_command(&self, command: MemoryListCommand) -> Result<(), NtStatus> {
        nt::execute_memory_command(command)
    }

    fn flush_file_cache(&self) -> Result<(), u32> {
        memory::flush_system_file_cache()
    }

    fn flush_registry(&self) -> Result<(), NtStatus> {
        nt::execute_registry_flush()
    }

    fn combine_pages(&self) -> Result<usize, NtStatus> {
        nt::execute_combine_memory()
    }

    fn processes(&self) -> Result<Vec<ProcessEntry>> {
        process::processes()
    }

    fn trim_process(&self, pid: u32) -> bool {
        process::empty_working_set(pid)
    }

    fn own_pid(&self) -> u32 {
        std::process::id()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}
