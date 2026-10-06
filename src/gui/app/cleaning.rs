//! Running cleans on a worker thread, collecting their results, and the
//! auto-clean monitor.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::MagicXApp;
use crate::engine::auto_clean::Decision;
use crate::engine::{self, CleanLevel, Progress, SmartCleanResult};
use crate::memory::{self, MemoryComposition};

/// Maximum number of events kept in the auto-clean activity list.
pub(super) const MONITOR_LOG_CAPACITY: usize = 500;

/// What an auto-clean event reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// Monitor turned on or off, a clean starting, or a backoff.
    Info,
    /// An auto-clean finished.
    Cleaned,
    /// An auto-clean failed.
    Failed,
}

/// One entry in the auto-clean activity list.
#[derive(Debug, Clone)]
pub struct MonitorEvent {
    /// Local time it happened, `HH:MM:SS`.
    pub time: String,
    /// What kind of event it is.
    pub kind: EventKind,
    /// What happened, in plain words.
    pub text: String,
}

/// Result of a background cleaning operation sent back to the UI thread.
pub struct CleanResultMsg {
    /// The cleaning result (or error string).
    pub result: std::result::Result<SmartCleanResult, String>,
    /// Which level was requested.
    pub level: CleanLevel,
    /// `true` when the clean was started by monitor auto-clean.
    pub auto: bool,
}

/// How far a running clean has got, updated by the worker thread.
#[derive(Debug, Clone)]
pub struct CleanProgress {
    /// The level being run.
    pub level: CleanLevel,
    /// `true` when auto-clean started it.
    pub auto: bool,
    /// Steps started so far.
    pub step: usize,
    /// Steps the level plans to run (a leftover sweep can add more).
    pub total: usize,
    /// What the current step is doing, e.g. "Purging all standby pages...".
    pub label: &'static str,
    /// When the clean started.
    pub started: Instant,
}

/// Progress shared between the clean worker and the UI.
pub type SharedProgress = Arc<Mutex<Option<CleanProgress>>>;

/// The memory map moving from its state before a clean to after it.
#[derive(Debug, Clone, Copy)]
pub struct MapTransition {
    /// Memory lists before the clean.
    pub from: MemoryComposition,
    /// Memory lists after the clean.
    pub to: MemoryComposition,
    /// When the transition started.
    pub started: Instant,
}

impl MapTransition {
    /// How long the memory map takes to move to its new state.
    pub const DURATION: Duration = Duration::from_millis(900);

    /// How long the map then keeps showing the after-state: longer than the
    /// stats thread's capture interval, so a fresh reading has replaced the
    /// one taken before the clean.
    pub const HOLD: Duration = Duration::from_millis(1200);
}

impl MagicXApp {
    /// Start a manual cleaning operation on a background thread.
    pub fn start_clean(&mut self, level: CleanLevel) {
        self.spawn_clean(level, false);
    }

    /// Start a cleaning operation on a background thread.
    ///
    /// `auto` marks cleans triggered by the monitor so the result is logged
    /// and the cooldown applied only for those.
    pub(super) fn spawn_clean(&mut self, level: CleanLevel, auto: bool) {
        if self.cleaning_in_progress {
            return;
        }
        self.cleaning_in_progress = true;
        self.last_clean_result = None;
        if let Ok(mut progress) = self.clean_progress.lock() {
            *progress = Some(CleanProgress {
                level,
                auto,
                step: 0,
                total: engine::dry_run_plan(level, false).len(),
                label: "",
                started: Instant::now(),
            });
        }

        let tx = self.clean_tx.clone();
        let progress = Arc::clone(&self.clean_progress);
        if let Err(e) = std::thread::Builder::new()
            .name("gui-clean".into())
            .spawn(move || {
                // Catch a panic so a result is always sent; otherwise
                // `cleaning_in_progress` would stay set forever. This only
                // matters in dev builds: the release profile uses
                // `panic = "abort"`, where a panic ends the process instead.
                let result = std::panic::catch_unwind(|| {
                    engine::Cleaner::new(&engine::WindowsMemory, |event| {
                        if let Progress::Started { label } = event
                            && let Ok(mut lock) = progress.lock()
                            && let Some(p) = lock.as_mut()
                        {
                            p.step += 1;
                            p.label = label;
                        }
                    })
                    .smart_clean(level, &[])
                    .map_err(|e| format!("{e:#}"))
                })
                .unwrap_or_else(|_| Err("clean worker panicked".to_owned()));
                drop(tx.send(CleanResultMsg {
                    result,
                    level,
                    auto,
                }));
            })
        {
            drop(self.clean_tx.send(CleanResultMsg {
                result: Err(format!("failed to spawn clean thread: {e}")),
                level,
                auto,
            }));
        }
    }

    /// Add an event to the auto-clean activity list, dropping the oldest
    /// once [`MONITOR_LOG_CAPACITY`] is reached.
    pub(super) fn push_monitor_log(&mut self, kind: EventKind, text: String) {
        if self.monitor_log.len() >= MONITOR_LOG_CAPACITY {
            self.monitor_log.pop_front();
        }
        let now = crate::platform::time::local_now();
        let time = now.get(11..).unwrap_or(&now).to_owned();
        self.monitor_log
            .push_back(MonitorEvent { time, kind, text });
    }

    /// Poll for completed cleaning results.
    ///
    /// Auto-clean results are logged to the activity log exactly once (here,
    /// not in `logic()`) and start the cooldown, with backoff when the clean
    /// did not bring memory load below the threshold.
    pub(super) fn poll_clean_results(&mut self) {
        let Ok(msg) = self.clean_rx.try_recv() else {
            return;
        };
        self.cleaning_in_progress = false;
        if let Ok(mut progress) = self.clean_progress.lock() {
            *progress = None;
        }
        if let Ok(r) = &msg.result
            && let (Some(from), Some(to)) = (
                r.overall_before.composition(),
                r.overall_after.composition(),
            )
        {
            self.map_transition = Some(MapTransition {
                from,
                to,
                started: Instant::now(),
            });
        }

        if msg.auto {
            let (kind, text) = match &msg.result {
                Ok(r) => (
                    EventKind::Cleaned,
                    format!(
                        "{} freed {} in {:.1} s",
                        msg.level.title_case_name(),
                        memory::format_bytes(r.reclaimed_bytes().max(0) as u64),
                        r.total_elapsed_secs,
                    ),
                ),
                Err(e) => (EventKind::Failed, format!("Auto-clean failed: {e}")),
            };
            self.push_monitor_log(kind, text);

            let load_after = msg.result.as_ref().map_or_else(
                |_| {
                    self.latest_snapshot
                        .lock()
                        .ok()
                        .and_then(|s| s.as_ref().map(|s| s.memory_load_percent))
                },
                |r| Some(r.overall_after.memory_load_percent),
            );
            self.sync_auto_clean_limits();
            if self.auto_clean.record_clean(Instant::now(), load_after) {
                let msg = format!(
                    "Memory is still at {}% or more. Next auto-clean in {} s.",
                    self.settings.monitor_threshold,
                    self.auto_clean.effective_cooldown().as_secs(),
                );
                self.push_monitor_log(EventKind::Info, msg);
            }
        }

        self.last_clean_result = Some(msg);
    }

    /// Apply the current threshold and cooldown settings to the policy.
    pub(super) const fn sync_auto_clean_limits(&mut self) {
        self.auto_clean.set_limits(
            self.settings.monitor_threshold,
            Duration::from_secs(self.settings.monitor_cooldown_secs),
        );
    }

    /// Handle auto-clean in monitor mode: check the current memory load
    /// against the threshold and start a clean when it is reached.
    pub(super) fn handle_monitor_auto_clean(&mut self) {
        if !self.monitor_active || self.cleaning_in_progress {
            return;
        }

        let load = self
            .latest_snapshot
            .lock()
            .ok()
            .and_then(|s| s.as_ref().map(|s| s.memory_load_percent));
        let Some(load) = load else {
            return;
        };

        self.sync_auto_clean_limits();
        match self.auto_clean.decide(load, Instant::now()) {
            Decision::Clean => {
                let msg = format!(
                    "Memory reached {load}%, running {}",
                    self.settings.default_clean_level.title_case_name(),
                );
                self.push_monitor_log(EventKind::Info, msg);
                self.spawn_clean(self.settings.default_clean_level, true);
            }
            // The chart shows the monitor is alive, so quiet checks are not
            // logged.
            Decision::CoolingDown | Decision::BelowThreshold => {}
        }
    }
}
