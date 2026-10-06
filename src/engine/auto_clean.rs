//! When to clean automatically: the threshold, cooldown and backoff rules
//! shared by the CLI monitor and the GUI's auto-clean.

use std::time::{Duration, Instant};

use super::SmartCleanResult;

/// Upper bound for the cooldown backoff multiplier.
pub const MAX_BACKOFF: u32 = 8;

/// What the monitor should do at a given memory load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Load is below the threshold; nothing to do.
    BelowThreshold,
    /// Load is high, but the (backed-off) cooldown since the last clean has
    /// not elapsed yet.
    CoolingDown,
    /// Load is high and the cooldown has elapsed: clean now.
    Clean,
}

/// Auto-clean policy.
///
/// - Cleans when memory load reaches the threshold.
/// - Waits at least the cooldown after a clean *finishes* before the next,
///   so a clean longer than the cooldown cannot be followed by another one
///   straight away. This holds for cleans the user starts too (see
///   [`AutoCleanPolicy::note_manual_clean`]).
/// - Doubles the cooldown (up to [`MAX_BACKOFF`] times) after a clean that
///   leaves load at or above the threshold, which stops futile back-to-back
///   cleans when the load is held up by memory cleaning cannot reclaim. The
///   backoff resets once load drops below the threshold.
#[derive(Debug, Clone)]
pub struct AutoCleanPolicy {
    /// Load percentage that triggers a clean.
    threshold: u32,
    /// Base cooldown after a clean.
    cooldown: Duration,
    /// Current cooldown multiplier (1 = no backoff).
    backoff: u32,
    /// When the last clean finished.
    last_clean: Option<Instant>,
}

impl AutoCleanPolicy {
    /// A policy that cleans at `threshold` percent load with `cooldown`
    /// between cleans.
    #[must_use]
    pub const fn new(threshold: u32, cooldown: Duration) -> Self {
        Self {
            threshold,
            cooldown,
            backoff: 1,
            last_clean: None,
        }
    }

    /// Update the threshold and cooldown, keeping the timing state.
    pub const fn set_limits(&mut self, threshold: u32, cooldown: Duration) {
        self.threshold = threshold;
        self.cooldown = cooldown;
    }

    /// The load percentage that triggers a clean.
    #[must_use]
    pub const fn threshold(&self) -> u32 {
        self.threshold
    }

    /// Drop any backoff (e.g. when monitoring restarts). The cooldown since
    /// the last clean still applies, so toggling monitoring off and on cannot
    /// trigger back-to-back cleans.
    pub const fn reset_backoff(&mut self) {
        self.backoff = 1;
    }

    /// The cooldown currently in force: the base cooldown times the backoff.
    #[must_use]
    pub const fn effective_cooldown(&self) -> Duration {
        self.cooldown.saturating_mul(self.backoff)
    }

    /// Decide what to do at memory load `load` (percent) at time `now`.
    pub fn decide(&mut self, load: u32, now: Instant) -> Decision {
        if load < self.threshold {
            self.backoff = 1;
            return Decision::BelowThreshold;
        }
        let cooling = self
            .last_clean
            .is_some_and(|last| now.saturating_duration_since(last) < self.effective_cooldown());
        if cooling {
            Decision::CoolingDown
        } else {
            Decision::Clean
        }
    }

    /// Record that a clean finished at `now`, leaving memory load at
    /// `load_after` percent (`None` if unknown, e.g. the clean failed).
    ///
    /// Returns `true` when load is still at or above the threshold, in which
    /// case the cooldown has been backed off. An unknown load keeps the
    /// current backoff: a failed clean says nothing about whether cleaning
    /// can bring the load down.
    pub fn record_clean(&mut self, now: Instant, load_after: Option<u32>) -> bool {
        self.last_clean = Some(now);
        let Some(load) = load_after else {
            return false;
        };
        let still_high = load >= self.threshold;
        self.backoff = if still_high {
            (self.backoff * 2).min(MAX_BACKOFF)
        } else {
            1
        };
        still_high
    }

    /// Record a clean the user started (not the monitor) that finished at
    /// `now`. The cooldown starts, so an auto-clean cannot follow it straight
    /// away, but the backoff is left alone: the user's clean says nothing
    /// about whether auto-cleaning is futile.
    pub const fn note_manual_clean(&mut self, now: Instant) {
        self.last_clean = Some(now);
    }
}

/// The memory load a clean left behind, for [`AutoCleanPolicy::record_clean`].
///
/// `None` when the clean failed, so a failure never backs the cooldown off.
/// Both front ends use this, so they treat failures the same way.
pub fn load_after<E>(outcome: &Result<SmartCleanResult, E>) -> Option<u32> {
    outcome
        .as_ref()
        .ok()
        .map(|result| result.overall_after.memory_load_percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COOLDOWN: Duration = Duration::from_secs(30);

    fn policy() -> AutoCleanPolicy {
        AutoCleanPolicy::new(80, COOLDOWN)
    }

    #[test]
    fn cleans_at_threshold_but_not_below() {
        let mut p = policy();
        let now = Instant::now();
        assert_eq!(p.decide(79, now), Decision::BelowThreshold);
        assert_eq!(p.decide(80, now), Decision::Clean);
    }

    #[test]
    fn cooldown_runs_from_the_end_of_the_clean() {
        let mut p = policy();
        let start = Instant::now();
        let finished = start + Duration::from_secs(45); // longer than the cooldown
        p.record_clean(finished, Some(50));
        assert_eq!(
            p.decide(90, finished + Duration::from_secs(29)),
            Decision::CoolingDown
        );
        assert_eq!(p.decide(90, finished + COOLDOWN), Decision::Clean);
    }

    #[test]
    fn futile_cleans_back_off_up_to_the_cap() {
        let mut p = policy();
        let mut now = Instant::now();
        let mut expected = [2, 4, 8, 8].into_iter();
        for _ in 0..4 {
            assert!(p.record_clean(now, Some(85)));
            let backoff = expected.next().unwrap_or(MAX_BACKOFF);
            assert_eq!(p.effective_cooldown(), COOLDOWN * backoff);
            now += p.effective_cooldown();
        }
    }

    #[test]
    fn backoff_resets_once_load_drops() {
        let mut p = policy();
        let now = Instant::now();
        p.record_clean(now, Some(85));
        assert_eq!(p.effective_cooldown(), COOLDOWN * 2);
        assert_eq!(p.decide(60, now), Decision::BelowThreshold);
        assert_eq!(p.effective_cooldown(), COOLDOWN);
    }

    #[test]
    fn successful_clean_does_not_back_off() {
        let mut p = policy();
        assert!(!p.record_clean(Instant::now(), Some(70)));
        assert_eq!(p.effective_cooldown(), COOLDOWN);
    }

    #[test]
    fn failed_clean_keeps_the_current_backoff() {
        let mut p = policy();
        let now = Instant::now();
        p.record_clean(now, Some(85));
        p.record_clean(now, Some(85));
        assert_eq!(p.effective_cooldown(), COOLDOWN * 4);
        assert!(!p.record_clean(now, None));
        assert_eq!(p.effective_cooldown(), COOLDOWN * 4);
    }

    #[test]
    fn a_manual_clean_starts_the_cooldown_without_backing_off() {
        let mut p = policy();
        let now = Instant::now();
        p.record_clean(now, Some(85));
        p.note_manual_clean(now + Duration::from_secs(100));
        assert_eq!(p.effective_cooldown(), COOLDOWN * 2, "backoff unchanged");
        assert_eq!(
            p.decide(90, now + Duration::from_secs(101)),
            Decision::CoolingDown,
            "no auto-clean right after the user's clean"
        );
    }

    #[test]
    fn reset_backoff_keeps_the_running_cooldown() {
        let mut p = policy();
        let now = Instant::now();
        p.record_clean(now, Some(90));
        p.reset_backoff();
        assert_eq!(p.effective_cooldown(), COOLDOWN);
        assert_eq!(p.decide(90, now + COOLDOWN / 2), Decision::CoolingDown);
        assert_eq!(p.decide(90, now + COOLDOWN), Decision::Clean);
    }
}
