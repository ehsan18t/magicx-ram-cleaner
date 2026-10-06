//! The last few minutes of memory readings, for the Monitor chart.
//!
//! A sample is recorded whenever the stats thread already reads memory
//! (window visible or auto-clean on), so the history costs no extra work.
//! Time when nothing was read shows as a gap: consecutive samples further
//! apart than [`GAP`] start a new run.

use std::time::{Duration, Instant};

use crate::memory::MemorySnapshot;

/// How much history is kept and charted.
pub const WINDOW: Duration = Duration::from_mins(10);

/// Samples further apart than this are drawn as separate runs.
pub const GAP: Duration = Duration::from_millis(2500);

/// One reading: memory load and, when known, the memory-list shares.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// When it was read.
    pub at: Instant,
    /// Memory load, 0.0 to 1.0.
    pub load: f32,
    /// Shares of In use, Modified, Standby and Free, when the page lists
    /// could be read.
    pub lists: Option<[f32; 4]>,
}

impl Sample {
    /// A sample of `snap`, taken at `at`.
    #[must_use]
    pub fn of(snap: &MemorySnapshot, at: Instant) -> Self {
        let lists = snap.composition().map(|c| {
            let total = c.total().max(1) as f32;
            [
                c.in_use as f32 / total,
                c.modified as f32 / total,
                c.standby as f32 / total,
                c.free as f32 / total,
            ]
        });
        Self {
            at,
            load: snap.memory_load_percent as f32 / 100.0,
            lists,
        }
    }
}

/// Samples from the last [`WINDOW`], oldest first.
#[derive(Debug, Clone, Default)]
pub struct History {
    /// The samples, oldest first.
    samples: Vec<Sample>,
}

impl History {
    /// Add a sample and forget those older than [`WINDOW`] before it.
    pub fn push(&mut self, sample: Sample) {
        self.samples.push(sample);
        let cutoff = sample.at.checked_sub(WINDOW);
        let stale = cutoff.map_or(0, |cutoff| self.samples.partition_point(|s| s.at < cutoff));
        self.samples.drain(..stale);
    }

    /// All samples, oldest first.
    #[must_use]
    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }

    /// Unbroken runs of samples, split wherever recording paused.
    pub fn runs(&self) -> impl Iterator<Item = &[Sample]> {
        self.samples
            .chunk_by(|a, b| b.at.saturating_duration_since(a.at) <= GAP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(base: Instant, secs: u64) -> Sample {
        Sample {
            at: base + Duration::from_secs(secs),
            load: 0.5,
            lists: None,
        }
    }

    #[test]
    fn samples_older_than_the_window_are_dropped() {
        let base = Instant::now();
        let mut history = History::default();
        history.push(sample(base, 0));
        history.push(sample(base, 1));
        history.push(sample(base, WINDOW.as_secs() + 1));
        assert_eq!(history.samples().len(), 2);
        assert_eq!(history.samples()[0].at, base + Duration::from_secs(1));
    }

    #[test]
    fn a_pause_in_recording_splits_the_runs() {
        let base = Instant::now();
        let mut history = History::default();
        for secs in [0, 1, 2, 30, 31] {
            history.push(sample(base, secs));
        }
        let lengths: Vec<usize> = history.runs().map(<[Sample]>::len).collect();
        assert_eq!(lengths, [3, 2]);
    }
}
