//! Running cleans on a worker thread, collecting their results, and the
//! auto-clean monitor.

use std::time::{Duration, Instant};

use super::MagicXApp;
use crate::engine::auto_clean::Decision;
use crate::engine::{self, CleanLevel, SmartCleanResult};
use crate::memory;

/// Maximum number of lines kept in the monitor activity log.
pub(super) const MONITOR_LOG_CAPACITY: usize = 500;

/// Result of a background cleaning operation sent back to the UI thread.
pub struct CleanResultMsg {
    /// The cleaning result (or error string).
    pub result: std::result::Result<SmartCleanResult, String>,
    /// Which level was requested.
    pub level: CleanLevel,
    /// `true` when the clean was started by monitor auto-clean.
    pub auto: bool,
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

        let tx = self.clean_tx.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("gui-clean".into())
            .spawn(move || {
                // Catch a panic so a result is always sent; otherwise
                // `cleaning_in_progress` would stay set forever. This only
                // matters in dev builds: the release profile uses
                // `panic = "abort"`, where a panic ends the process instead.
                let result = std::panic::catch_unwind(|| {
                    engine::Cleaner::silent(&engine::WindowsMemory)
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

    /// Append a line to the monitor activity log, dropping the oldest line
    /// once [`MONITOR_LOG_CAPACITY`] is reached.
    pub(super) fn push_monitor_log(&mut self, msg: String) {
        if self.monitor_log.len() >= MONITOR_LOG_CAPACITY {
            self.monitor_log.pop_front();
        }
        self.monitor_log.push_back(msg);
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

        if msg.auto {
            let log_msg = match &msg.result {
                Ok(r) => format!(
                    "Auto-clean complete: freed {}",
                    memory::format_bytes(r.reclaimed_bytes().max(0) as u64),
                ),
                Err(e) => format!("Auto-clean failed: {e}"),
            };
            self.push_monitor_log(log_msg);

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
                    "Memory load still at or above {}%; next auto-clean in {}s.",
                    self.settings.monitor_threshold,
                    self.auto_clean.effective_cooldown().as_secs(),
                );
                self.push_monitor_log(msg);
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

    /// Handle auto-clean in monitor mode.
    ///
    /// Checks the current memory load against the configured threshold and
    /// triggers a clean when exceeded.  Also emits periodic heartbeat
    /// messages to the activity log so the user can see the monitor is
    /// actively checking (every 60 seconds).
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
                    "Memory load {load}% >= threshold {}%, auto-cleaning ({})...",
                    self.settings.monitor_threshold,
                    self.settings.default_clean_level.title_case_name(),
                );
                self.push_monitor_log(msg);
                self.last_monitor_status_log = Some(Instant::now());
                self.spawn_clean(self.settings.default_clean_level, true);
            }
            Decision::CoolingDown => {}
            Decision::BelowThreshold => {
                // Periodic heartbeat so the user knows the monitor is alive.
                let should_log = self
                    .last_monitor_status_log
                    .is_none_or(|t| t.elapsed() >= Duration::from_secs(60));
                if should_log {
                    self.push_monitor_log(format!(
                        "Checked: memory at {load}%, below threshold {}%. No action needed.",
                        self.settings.monitor_threshold,
                    ));
                    self.last_monitor_status_log = Some(Instant::now());
                }
            }
        }
    }
}
