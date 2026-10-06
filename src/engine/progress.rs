//! Progress events the engine reports while it cleans.
//!
//! The engine never prints. Callers that show progress (the CLI's
//! `--verbose`) turn these events into output; others ignore them.

/// One step of a cleaning run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// An operation is starting. `label` describes it, e.g.
    /// "Purging all standby pages...".
    Started {
        /// What the operation is doing.
        label: &'static str,
    },
    /// Memory stopped changing `after_ms` milliseconds after the operation.
    Settled {
        /// Time until memory settled.
        after_ms: u64,
    },
    /// Memory was still changing when the settle timeout expired; the latest
    /// reading is used.
    SettleTimedOut {
        /// The settle timeout that expired.
        after_ms: u64,
    },
    /// A process was not trimmed because its name matched an exclusion.
    Excluded {
        /// Executable name, e.g. `chrome.exe`.
        name: String,
        /// Process ID.
        pid: u32,
    },
    /// The second flush and purge pass of a Nuclear clean is starting.
    SecondPass,
    /// A leftover sweep pass is starting.
    Sweep {
        /// Pass number, starting at 1.
        pass: u32,
        /// Standby plus pagefile-backed modified memory still present.
        leftover_bytes: u64,
    },
}
