//! The Processes page's Trim action: trimming every instance of one program
//! on a worker thread, and keeping each program's result for display.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::MagicXApp;
use crate::engine::{self, TrimReport};

/// Where a program's trim stands.
#[derive(Debug, Clone, Copy)]
pub enum TrimState {
    /// The worker is trimming.
    Running,
    /// Finished, with what it achieved and when.
    Done(TrimReport, Instant),
}

/// Trims that are running or recently finished.
#[derive(Debug, Clone, Default)]
pub struct TrimLog {
    /// State per program, keyed by lower-case executable name.
    pub by_program: HashMap<String, TrimState>,
    /// The latest finished trim: program name, result and time. Shown above
    /// the list, since a trimmed program usually sorts out of view.
    pub last: Option<(String, TrimReport, Instant)>,
}

/// The trim log shared with the worker threads.
pub type SharedTrimLog = Arc<Mutex<TrimLog>>;

impl MagicXApp {
    /// Trim the working sets of `pids`, all instances of the program `name`
    /// (`key` is its lower-case form). Does nothing while that program is
    /// already being trimmed.
    pub fn trim_program(&self, key: &str, name: &str, pids: Vec<u32>) {
        let Ok(mut log) = self.trim_log.lock() else {
            return;
        };
        if matches!(log.by_program.get(key), Some(TrimState::Running)) {
            return;
        }
        log.by_program.insert(key.to_owned(), TrimState::Running);
        drop(log);

        let worker_key = key.to_owned();
        let worker_name = name.to_owned();
        let log = Arc::clone(&self.trim_log);
        let stale = Arc::clone(&self.processes_stale);
        let spawned = std::thread::Builder::new()
            .name("gui-trim".into())
            .spawn(move || {
                let report = engine::trim_processes(&engine::WindowsMemory, &pids, |pid| {
                    crate::platform::process::memory_counters(pid).map(|c| c.working_set)
                });
                let now = Instant::now();
                if let Ok(mut log) = log.lock() {
                    log.by_program
                        .insert(worker_key, TrimState::Done(report, now));
                    log.last = Some((worker_name, report, now));
                }
                // Show the trimmed sizes without waiting for the next refresh.
                stale.store(true, Ordering::Release);
            });
        if spawned.is_err()
            && let Ok(mut log) = self.trim_log.lock()
        {
            log.by_program.remove(key);
        }
    }
}
