//! A simulated [`MemorySystem`] for engine tests.
//!
//! Models physical memory as four page pools (in use, modified, standby,
//! free) whose sum is constant. Each operation moves pages between the pools
//! the way Windows does, and every call is recorded so tests can assert on
//! what the engine did, in which order.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Duration;

use anyhow::{Result, bail};

use super::system::MemorySystem;
use crate::memory::{MemoryListInfo, MemorySnapshot, QuickMemoryReading};
use crate::platform::nt::{MemoryListCommand, NtStatus};
use crate::platform::process::{ProcessEntry, TrimOutcome};

/// Page size used by the model.
pub const PAGE: u64 = 4096;

/// Pages per GiB.
pub const GIB_PAGES: u64 = 1024 * 1024 * 1024 / PAGE;

/// The process ID the fake reports for the engine itself.
pub const OWN_PID: u32 = 1000;

/// One operating-system call made by the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    /// `memory_command`
    Command(MemoryListCommand),
    /// `flush_file_cache`
    FlushFileCache,
    /// `flush_registry`
    FlushRegistry,
    /// `combine_pages`
    Combine,
    /// `trim_process`
    Trim(u32),
}

/// Mutable model state.
#[derive(Debug, Clone)]
pub struct Model {
    /// Pages in process working sets and other in-use memory.
    pub in_use: u64,
    /// Dirty pages awaiting write-back.
    pub modified: u64,
    /// Cached pages that count as available.
    pub standby: u64,
    /// Zeroed and free pages.
    pub free: u64,
    /// Pages released from working sets by emptying them.
    pub trimmable: u64,
    /// Pages the file cache flush moves to standby.
    pub file_cache: u64,
    /// Whether the kernel page-list query works (needs the privilege).
    pub lists_available: bool,
    /// Pages that land back on standby right after each purge (in-flight
    /// write-back or cache refill), consumed front to back; empty = none.
    pub refill_after_purge: VecDeque<u64>,
    /// Command that fails, and the status it fails with.
    pub failing_command: Option<(MemoryListCommand, NtStatus)>,
    /// Whether the failing command still moves its pages before reporting
    /// the error (a partial success), rather than doing nothing.
    pub failure_still_applies: bool,
    /// Running processes.
    pub processes: Vec<ProcessEntry>,
    /// PIDs that cannot be trimmed (protected processes).
    pub protected: Vec<u32>,
    /// PIDs whose process has exited since it was listed.
    pub exited: Vec<u32>,
    /// Bytes each process gives back when trimmed (0 if not listed).
    pub trim_freed: Vec<(u32, u64)>,
    /// Offsets in bytes added to the available memory of successive settle
    /// readings, consumed front to back (empty = memory is still). Lets a
    /// test make memory keep moving so the settle loop has to wait.
    pub reading_offsets: VecDeque<u64>,
    /// Win32 error the file cache flush fails with, if any.
    pub failing_file_cache: Option<u32>,
    /// Status the registry flush fails with, if any.
    pub failing_registry: Option<NtStatus>,
    /// Status page combining fails with, if any.
    pub failing_combine: Option<NtStatus>,
}

impl Default for Model {
    /// 16 GiB machine: 8 GiB in use (2 GiB trimmable), 1 GiB modified,
    /// 4 GiB standby, 3 GiB free, 512 MiB of file cache.
    fn default() -> Self {
        Self {
            in_use: 8 * GIB_PAGES,
            modified: GIB_PAGES,
            standby: 4 * GIB_PAGES,
            free: 3 * GIB_PAGES,
            trimmable: 2 * GIB_PAGES,
            file_cache: GIB_PAGES / 2,
            lists_available: true,
            refill_after_purge: VecDeque::new(),
            failing_command: None,
            failure_still_applies: false,
            processes: vec![
                entry(OWN_PID, "magicx-ram-cleaner.exe"),
                entry(2000, "chrome.exe"),
                entry(3000, "notepad.exe"),
                entry(4000, "protected.exe"),
            ],
            protected: vec![4000],
            exited: Vec::new(),
            trim_freed: Vec::new(),
            reading_offsets: VecDeque::new(),
            failing_file_cache: None,
            failing_registry: None,
            failing_combine: None,
        }
    }
}

/// Build a process entry.
pub fn entry(pid: u32, name: &str) -> ProcessEntry {
    ProcessEntry {
        pid,
        parent_pid: 1,
        name: name.to_owned(),
    }
}

/// The simulated memory system.
#[derive(Debug, Default)]
pub struct FakeSystem {
    /// Current model state.
    pub model: RefCell<Model>,
    /// Every call made, in order.
    pub calls: RefCell<Vec<Call>>,
}

impl FakeSystem {
    /// A fake with the given initial model.
    pub fn new(model: Model) -> Self {
        Self {
            model: RefCell::new(model),
            calls: RefCell::default(),
        }
    }

    /// The calls made so far.
    pub fn calls(&self) -> Vec<Call> {
        self.calls.borrow().clone()
    }

    fn total_pages(m: &Model) -> u64 {
        m.in_use + m.modified + m.standby + m.free
    }
}

/// Move up to `pages` from `from` to `to`, returning how many moved.
fn shift(from: &mut u64, to: &mut u64, pages: u64) -> u64 {
    let moved = pages.min(*from);
    *from -= moved;
    *to += moved;
    moved
}

impl MemorySystem for FakeSystem {
    fn snapshot(&self) -> Result<MemorySnapshot> {
        let m = self.model.borrow();
        let total = Self::total_pages(&m) * PAGE;
        let available = (m.standby + m.free) * PAGE;
        let lists = m.lists_available.then(|| MemoryListInfo {
            zeroed_pages: m.free,
            free_pages: 0,
            modified_pages: m.modified,
            modified_no_write_pages: 0,
            bad_pages: 0,
            standby_pages: [m.standby, 0, 0, 0, 0, 0, 0, 0],
            repurposed_pages: [0; 8],
            modified_pagefile_pages: m.modified,
        });
        Ok(MemorySnapshot {
            memory_load_percent: ((total - available) * 100 / total) as u32,
            total_physical: total,
            available_physical: available,
            used_physical: total - available,
            total_page_file: 0,
            available_page_file: 0,
            total_virtual: 0,
            available_virtual: 0,
            commit_total_pages: 0,
            commit_limit_pages: 0,
            commit_peak_pages: 0,
            physical_available_pages: m.standby + m.free,
            physical_total_pages: Self::total_pages(&m),
            kernel_paged_pages: 0,
            kernel_nonpaged_pages: 0,
            page_size: PAGE,
            handle_count: 0,
            process_count: m.processes.len() as u32,
            thread_count: 0,
            lists,
        })
    }

    fn quick_reading(&self) -> Result<QuickMemoryReading> {
        let mut m = self.model.borrow_mut();
        let offset = m.reading_offsets.pop_front().unwrap_or(0);
        Ok(QuickMemoryReading {
            total_physical: Self::total_pages(&m) * PAGE,
            available_physical: (m.standby + m.free) * PAGE + offset,
        })
    }

    fn memory_command(&self, command: MemoryListCommand) -> Result<(), NtStatus> {
        self.calls.borrow_mut().push(Call::Command(command));
        let mut m = self.model.borrow_mut();
        let failure = m
            .failing_command
            .filter(|(failing, _)| *failing == command)
            .map(|(_, status)| status);
        if let Some(status) = failure
            && !m.failure_still_applies
        {
            return Err(status);
        }
        let m = &mut *m;
        match command {
            MemoryListCommand::EmptyWorkingSets => {
                // Half of the trimmed pages are dirty, half clean.
                let trimmed = m.trimmable.min(m.in_use);
                m.trimmable -= trimmed;
                m.in_use -= trimmed;
                m.modified += trimmed / 2;
                m.standby += trimmed - trimmed / 2;
            }
            MemoryListCommand::FlushModifiedList => {
                let modified = m.modified;
                shift(&mut m.modified, &mut m.standby, modified);
            }
            MemoryListCommand::PurgeStandbyList
            | MemoryListCommand::PurgeLowPriorityStandbyList => {
                let standby = m.standby;
                shift(&mut m.standby, &mut m.free, standby);
                let refill = m.refill_after_purge.pop_front().unwrap_or(0);
                shift(&mut m.free, &mut m.standby, refill);
            }
        }
        failure.map_or(Ok(()), Err)
    }

    fn flush_file_cache(&self) -> Result<(), u32> {
        self.calls.borrow_mut().push(Call::FlushFileCache);
        let mut m = self.model.borrow_mut();
        if let Some(error) = m.failing_file_cache {
            return Err(error);
        }
        let m = &mut *m;
        let cache = m.file_cache;
        m.file_cache = 0;
        shift(&mut m.in_use, &mut m.standby, cache);
        Ok(())
    }

    fn flush_registry(&self) -> Result<(), NtStatus> {
        self.calls.borrow_mut().push(Call::FlushRegistry);
        self.model.borrow().failing_registry.map_or(Ok(()), Err)
    }

    fn combine_pages(&self) -> Result<usize, NtStatus> {
        self.calls.borrow_mut().push(Call::Combine);
        let mut m = self.model.borrow_mut();
        if let Some(status) = m.failing_combine {
            return Err(status);
        }
        let m = &mut *m;
        Ok(shift(&mut m.in_use, &mut m.free, 1000) as usize)
    }

    fn processes(&self) -> Result<Vec<ProcessEntry>> {
        let m = self.model.borrow();
        if m.processes.is_empty() {
            bail!("no processes");
        }
        Ok(m.processes.clone())
    }

    fn trim_process(&self, pid: u32, _started: Option<u64>) -> TrimOutcome {
        self.calls.borrow_mut().push(Call::Trim(pid));
        let m = self.model.borrow();
        if m.exited.contains(&pid) {
            TrimOutcome::Exited
        } else if m.protected.contains(&pid) {
            TrimOutcome::Denied
        } else {
            let freed_bytes = m
                .trim_freed
                .iter()
                .find(|&&(p, _)| p == pid)
                .map_or(0, |&(_, bytes)| bytes);
            TrimOutcome::Trimmed { freed_bytes }
        }
    }

    fn own_pid(&self) -> u32 {
        OWN_PID
    }

    fn sleep(&self, _duration: Duration) {}
}
