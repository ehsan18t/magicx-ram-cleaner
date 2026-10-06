//! The cleaning operations by name: one place for what each one is called,
//! shared by the executors, their results, progress and the dry-run plan.

use crate::platform::nt::MemoryListCommand;

/// One kind of cleaning operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// Trim the system file cache.
    FlushFileCache,
    /// Write dirty registry hive pages to disk.
    FlushRegistry,
    /// Empty every process's working set with one kernel call.
    EmptyWorkingSetsKernel,
    /// Empty working sets process by process, skipping excluded names.
    EmptyWorkingSetsPerProcess,
    /// Write the modified page list to disk.
    FlushModified,
    /// Purge every standby page.
    PurgeStandby,
    /// Purge only priority-0 standby pages.
    PurgeLowPriorityStandby,
    /// Combine identical physical pages.
    CombinePages,
    /// Re-run flush and purge while leftovers remain (only if needed).
    LeftoverSweep,
}

impl Operation {
    /// The operation for a memory-list command.
    #[must_use]
    pub const fn for_command(command: MemoryListCommand) -> Self {
        match command {
            MemoryListCommand::EmptyWorkingSets => Self::EmptyWorkingSetsKernel,
            MemoryListCommand::FlushModifiedList => Self::FlushModified,
            MemoryListCommand::PurgeStandbyList => Self::PurgeStandby,
            MemoryListCommand::PurgeLowPriorityStandbyList => Self::PurgeLowPriorityStandby,
        }
    }

    /// Display name, as shown in results and the dry-run plan.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::FlushFileCache => "Flush File Cache",
            Self::FlushRegistry => "Flush Registry Cache",
            Self::EmptyWorkingSetsKernel => "Empty Working Sets (Kernel)",
            Self::EmptyWorkingSetsPerProcess => "Empty Working Sets (Per-Process)",
            Self::FlushModified => "Flush Modified List",
            Self::PurgeStandby => "Purge All Standby",
            Self::PurgeLowPriorityStandby => "Purge Low-Priority Standby",
            Self::CombinePages => "Memory Combining",
            Self::LeftoverSweep => "Leftover Sweep",
        }
    }

    /// What the operation is doing, shown while it runs.
    #[must_use]
    pub const fn progress_label(self) -> &'static str {
        match self {
            Self::FlushFileCache => "Flushing file system cache...",
            Self::FlushRegistry => "Flushing registry cache to disk...",
            Self::EmptyWorkingSetsKernel => "Emptying working sets (kernel-level)...",
            Self::EmptyWorkingSetsPerProcess => "Emptying working sets per-process...",
            Self::FlushModified => "Flushing modified page list...",
            Self::PurgeStandby => "Purging all standby pages...",
            Self::PurgeLowPriorityStandby => "Purging low-priority standby pages...",
            Self::CombinePages => "Running memory page combining...",
            Self::LeftoverSweep => "Sweeping leftover pages...",
        }
    }

    /// Whether the operation runs only when needed (the leftover sweep), so
    /// a progress count must not wait for it.
    #[must_use]
    pub const fn is_optional(self) -> bool {
        matches!(self, Self::LeftoverSweep)
    }
}

/// One step of a level's plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedStep {
    /// The operation the step runs.
    pub operation: Operation,
    /// Whether this is the Nuclear level's second flush and purge.
    pub second_pass: bool,
}

impl PlannedStep {
    /// The step's label: the operation name, marked for a second pass and
    /// for the optional sweep. Matches the executed result's name.
    #[must_use]
    pub fn label(self) -> String {
        let name = self.operation.name();
        if self.second_pass {
            format!("{name}{SECOND_PASS_SUFFIX}")
        } else if self.operation.is_optional() {
            format!("{name} (only if needed)")
        } else {
            name.to_owned()
        }
    }
}

/// Appended to the names of the Nuclear level's second-pass operations.
pub const SECOND_PASS_SUFFIX: &str = " (2nd pass)";
