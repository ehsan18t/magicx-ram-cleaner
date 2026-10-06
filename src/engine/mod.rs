//! # Cleaning engine
//!
//! Decides *what* to do to free memory and measures the effect; the
//! operating system work itself goes through a [`MemorySystem`](crate::engine::MemorySystem)
//! ([`WindowsMemory`](crate::engine::WindowsMemory) in production).
//!
//! A [`Cleaner`](crate::engine::Cleaner) runs either single operations (purge standby, flush the
//! modified list, empty working sets, ...) or a whole [`CleanLevel`](crate::engine::CleanLevel) via
//! [`Cleaner::smart_clean`](crate::engine::Cleaner::smart_clean). Every operation captures a snapshot before it
//! runs, waits for the kernel to settle afterwards, and returns a
//! [`CleanResult`](crate::engine::CleanResult) with the change in both *available* and *free* memory.
//! The engine never prints; it reports [`Progress`](crate::engine::Progress) events instead.

pub mod auto_clean;
mod level;
mod operations;
mod progress;
mod report;
mod settle;
mod smart;
mod system;
mod trim;

#[cfg(test)]
mod fake;
#[cfg(test)]
mod tests;

pub use self::level::{CleanLevel, ReclaimEstimate};
pub use self::progress::Progress;
pub use self::report::{CleanResult, SmartCleanResult};
pub use self::smart::dry_run_plan;
pub use self::system::{MemorySystem, WindowsMemory};
pub use self::trim::{TrimReport, TrimTarget, trim_processes};

/// Runs cleaning operations against a [`MemorySystem`], reporting
/// [`Progress`] to a caller-supplied callback.
pub struct Cleaner<'a> {
    /// The memory system operated on.
    sys: &'a dyn MemorySystem,
    /// Receives progress events.
    on_progress: Box<dyn FnMut(Progress) + 'a>,
}

impl<'a> Cleaner<'a> {
    /// Create a cleaner for `sys` that reports progress to `on_progress`.
    pub fn new(sys: &'a dyn MemorySystem, on_progress: impl FnMut(Progress) + 'a) -> Self {
        Self {
            sys,
            on_progress: Box::new(on_progress),
        }
    }

    /// Create a cleaner for `sys` that ignores progress.
    pub fn silent(sys: &'a dyn MemorySystem) -> Self {
        Self::new(sys, |_| {})
    }

    /// Forward a progress event to the callback.
    fn report(&mut self, progress: Progress) {
        (self.on_progress)(progress);
    }
}
