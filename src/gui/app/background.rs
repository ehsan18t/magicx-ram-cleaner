//! Background threads that keep memory statistics and the process list
//! fresh without blocking the UI.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;

use super::MagicXApp;
use crate::memory::{self, MemorySnapshot};

/// How often the background stats thread captures a snapshot (ms).
pub(super) const STATS_POLL_INTERVAL_MS: u64 = 1000;

/// How often the process list refreshes (seconds).
pub(super) const PROCESS_REFRESH_SECS: u64 = 5;

/// Background thread that periodically captures memory snapshots.
pub(super) fn stats_thread(
    snapshot: &Arc<Mutex<Option<MemorySnapshot>>>,
    running: &Arc<AtomicBool>,
    needs_repaint: &Arc<AtomicBool>,
    needs_capture: &Arc<AtomicBool>,
    ctx: &egui::Context,
) {
    while running.load(Ordering::Acquire) {
        // Only capture when the UI is visible or the monitor needs
        // logic() to run for auto-clean threshold checks.  When the
        // window is hidden/minimized with monitoring off, skip all
        // work - no Win32 calls, no mutex locks, no repaints.
        if needs_capture.load(Ordering::Acquire)
            && let Ok(snap) = MemorySnapshot::capture()
        {
            if let Ok(mut lock) = snapshot.lock() {
                *lock = Some(snap);
            }

            // Only request a repaint when the window is actually visible.
            // When the monitor is running but the window is hidden, we
            // still capture data above for threshold checks, but skip
            // the repaint to avoid unnecessary UI wake-ups and CPU usage.
            if needs_repaint.load(Ordering::Acquire) {
                ctx.request_repaint();
            }
        }

        std::thread::sleep(Duration::from_millis(STATS_POLL_INTERVAL_MS));
    }
}

impl MagicXApp {
    /// Refresh the process list if enough time has passed and no refresh is
    /// already running.
    pub(super) fn maybe_refresh_processes(&mut self) {
        let due = self.last_process_refresh.elapsed() >= Duration::from_secs(PROCESS_REFRESH_SECS)
            || self.processes_stale.load(Ordering::Acquire);
        if !due || self.process_refresh_in_flight.swap(true, Ordering::AcqRel) {
            return;
        }
        self.processes_stale.store(false, Ordering::Release);
        self.last_process_refresh = Instant::now();
        let procs_ref = Arc::clone(&self.top_processes);
        let in_flight = Arc::clone(&self.process_refresh_in_flight);
        let spawned = std::thread::Builder::new()
            .name("gui-procs".into())
            .spawn(move || {
                if let Ok(procs) = memory::query_all_processes()
                    && let Ok(mut lock) = procs_ref.lock()
                {
                    *lock = Some(procs);
                }
                in_flight.store(false, Ordering::Release);
            });
        if spawned.is_err() {
            self.process_refresh_in_flight
                .store(false, Ordering::Release);
        }
    }
}
