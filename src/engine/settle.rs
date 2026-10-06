//! Waiting for the kernel to finish reclaiming pages after an operation.

use std::time::Duration;

use anyhow::Result;

use super::Cleaner;
use super::progress::Progress;
use crate::memory::MemorySnapshot;

/// How thoroughly to wait for kernel memory settling.
///
/// `Full` is used for standalone operations and wherever write-back finishes
/// asynchronously (modified flush, standby purge). `Quick` is used for
/// intermediate operations whose effect is synchronous, to avoid spending
/// seconds per operation waiting for sub-megabyte variations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SettleMode {
    /// 3 consecutive stable reads, up to 20 polls (2 s max).
    Full,
    /// 1 stable read, up to 8 polls (0.8 s max).
    Quick,
}

/// Interval between settle polls.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Absolute floor for the "no longer changing" threshold.
const MIN_JITTER_BYTES: u64 = 4 * 1024 * 1024;

impl Cleaner<'_> {
    /// Wait for the kernel to finish processing memory operations.
    ///
    /// After `NtSetSystemInformation` returns, the kernel continues reclaiming
    /// pages asynchronously. This polls available physical memory until it
    /// stops changing between consecutive reads or a timeout is reached, then
    /// captures a full snapshot.
    pub(super) fn wait_for_settle(&mut self, mode: SettleMode) -> Result<MemorySnapshot> {
        self.settle(mode, true)
    }

    /// [`wait_for_settle`](Self::wait_for_settle) without reporting
    /// progress, for internal waits that are not an operation of their own.
    pub(super) fn wait_for_settle_silently(&mut self, mode: SettleMode) -> Result<MemorySnapshot> {
        self.settle(mode, false)
    }

    /// Settle polling; reports [`Progress`] only when `report` is set.
    fn settle(&mut self, mode: SettleMode, report: bool) -> Result<MemorySnapshot> {
        let (max_polls, stable_reads): (u32, u32) = match mode {
            SettleMode::Full => (20, 3),
            SettleMode::Quick => (8, 1),
        };

        let first = self.sys.quick_reading()?;

        // Scale the jitter threshold to total RAM: 0.01% of physical memory,
        // with a 4 MB floor. On a 16 GB system this is ~1.6 MB; on 128 GB ~13 MB.
        let jitter_threshold = (first.total_physical / 10_000).max(MIN_JITTER_BYTES);

        let mut prev_available = first.available_physical;
        let mut stable_count: u32 = 0;

        for poll in 1..=max_polls {
            self.sys.sleep(POLL_INTERVAL);
            let current = self.sys.quick_reading()?;

            // Kernel page transitions produce jitter, so "settled" means the
            // reading moved by less than the threshold since the last poll.
            if current.available_physical.abs_diff(prev_available) < jitter_threshold {
                stable_count += 1;
                if stable_count >= stable_reads {
                    if report {
                        self.report(Progress::Settled {
                            after_ms: elapsed_ms(poll),
                        });
                    }
                    // Only do the expensive full capture once settled
                    return self.sys.snapshot();
                }
            } else {
                stable_count = 0;
            }
            prev_available = current.available_physical;
        }

        if report {
            self.report(Progress::SettleTimedOut {
                after_ms: elapsed_ms(max_polls),
            });
        }
        self.sys.snapshot()
    }
}

/// Milliseconds spent after `polls` polls.
fn elapsed_ms(polls: u32) -> u64 {
    u64::from(polls) * POLL_INTERVAL.as_millis() as u64
}
