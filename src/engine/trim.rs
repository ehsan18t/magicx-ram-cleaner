//! Trimming the working sets of one program's processes (the Processes
//! page's Trim action).

use super::MemorySystem;
use crate::platform::process::TrimOutcome;

/// One process to trim, as it was when the process list was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrimTarget {
    /// Process ID.
    pub pid: u32,
    /// Start time when listed, so a PID that Windows has since reused for
    /// another process is left alone. `None` skips that check.
    pub started: Option<u64>,
}

/// The outcome of trimming a program's processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrimReport {
    /// Working-set bytes the trimmed processes gave back.
    pub freed_bytes: u64,
    /// Processes Windows refused to trim (protected).
    pub skipped: usize,
    /// Processes that had exited since the list was read (not counted in
    /// `total`).
    pub exited: usize,
    /// Running processes attempted.
    pub total: usize,
}

/// Trim every process in `targets`. The engine's own process is left alone.
///
/// A trim loses no data: pages leave the working set and fault back in as
/// the program touches them.
pub fn trim_processes(sys: &dyn MemorySystem, targets: &[TrimTarget]) -> TrimReport {
    let own = sys.own_pid();
    let mut report = TrimReport {
        freed_bytes: 0,
        skipped: 0,
        exited: 0,
        total: 0,
    };
    for target in targets.iter().filter(|t| t.pid != own) {
        match sys.trim_process(target.pid, target.started) {
            TrimOutcome::Trimmed { freed_bytes } => {
                report.total += 1;
                report.freed_bytes += freed_bytes;
            }
            TrimOutcome::Denied => {
                report.total += 1;
                report.skipped += 1;
            }
            TrimOutcome::Exited => report.exited += 1,
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fake::{Call, FakeSystem, Model, OWN_PID};

    const MIB: u64 = 1024 * 1024;

    /// A fake where trimming 2000 frees 300 MiB and 3000 frees 150 MiB.
    fn system() -> FakeSystem {
        FakeSystem::new(Model {
            trim_freed: vec![(2000, 300 * MIB), (3000, 150 * MIB)],
            ..Model::default()
        })
    }

    fn targets(pids: &[u32]) -> Vec<TrimTarget> {
        pids.iter()
            .map(|&pid| TrimTarget { pid, started: None })
            .collect()
    }

    #[test]
    fn freed_bytes_add_up_across_processes() {
        let report = trim_processes(&system(), &targets(&[2000, 3000]));
        assert_eq!(report.freed_bytes, 300 * MIB + 150 * MIB);
        assert_eq!(report.skipped, 0);
        assert_eq!(report.total, 2);
    }

    #[test]
    fn protected_processes_are_counted_as_skipped() {
        let report = trim_processes(&system(), &targets(&[2000, 4000]));
        assert_eq!(report.skipped, 1);
        assert_eq!(report.total, 2);
        assert_eq!(report.freed_bytes, 300 * MIB);
    }

    #[test]
    fn exited_processes_are_not_counted_as_protected() {
        let sys = system();
        sys.model.borrow_mut().exited.push(3000);
        let report = trim_processes(&sys, &targets(&[2000, 3000, 4000]));
        assert_eq!(report.exited, 1);
        assert_eq!(report.skipped, 1, "only the protected process");
        assert_eq!(report.total, 2, "the exited process is not attempted");
    }

    #[test]
    fn own_process_is_never_trimmed() {
        let sys = system();
        let report = trim_processes(&sys, &targets(&[OWN_PID, 3000]));
        assert_eq!(report.total, 1);
        assert!(!sys.calls().contains(&Call::Trim(OWN_PID)));
    }
}
