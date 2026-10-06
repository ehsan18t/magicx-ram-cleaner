//! Trimming the working sets of one program's processes (the Processes
//! page's Trim action).

use super::MemorySystem;

/// The outcome of trimming a program's processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrimReport {
    /// Working-set bytes the trimmed processes gave back.
    pub freed_bytes: u64,
    /// Processes Windows refused to trim (protected or already gone).
    pub skipped: usize,
    /// Processes attempted.
    pub total: usize,
}

/// Trim every process in `pids`, measuring each one's working set with
/// `working_set` before and after. The engine's own process is left alone.
///
/// A trim loses no data: pages leave the working set and fault back in as
/// the program touches them.
pub fn trim_processes(
    sys: &dyn MemorySystem,
    pids: &[u32],
    working_set: impl Fn(u32) -> Option<u64>,
) -> TrimReport {
    let own = sys.own_pid();
    let mut report = TrimReport {
        freed_bytes: 0,
        skipped: 0,
        total: 0,
    };
    for &pid in pids.iter().filter(|&&pid| pid != own) {
        report.total += 1;
        let before = working_set(pid);
        if sys.trim_process(pid) {
            if let (Some(before), Some(after)) = (before, working_set(pid)) {
                report.freed_bytes += before.saturating_sub(after);
            }
        } else {
            report.skipped += 1;
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;
    use crate::engine::fake::{Call, FakeSystem, Model, OWN_PID};

    const MIB: u64 = 1024 * 1024;

    /// Working sets that drop to a quarter once a process has been trimmed.
    fn working_sets(sys: &FakeSystem) -> impl Fn(u32) -> Option<u64> + '_ {
        let sizes: RefCell<HashMap<u32, u64>> = RefCell::new(HashMap::from([
            (2000, 400 * MIB),
            (3000, 200 * MIB),
            (4000, 100 * MIB),
        ]));
        move |pid| {
            let trimmed = sys.calls().contains(&Call::Trim(pid));
            sizes
                .borrow()
                .get(&pid)
                .map(|size| if trimmed { size / 4 } else { *size })
        }
    }

    #[test]
    fn freed_bytes_add_up_across_processes() {
        let sys = FakeSystem::new(Model::default());
        let report = trim_processes(&sys, &[2000, 3000], working_sets(&sys));
        assert_eq!(report.freed_bytes, 300 * MIB + 150 * MIB);
        assert_eq!(report.skipped, 0);
        assert_eq!(report.total, 2);
    }

    #[test]
    fn protected_processes_are_counted_as_skipped() {
        let sys = FakeSystem::new(Model::default());
        let report = trim_processes(&sys, &[2000, 4000], working_sets(&sys));
        assert_eq!(report.skipped, 1);
        assert_eq!(report.total, 2);
        assert_eq!(report.freed_bytes, 300 * MIB);
    }

    #[test]
    fn own_process_is_never_trimmed() {
        let sys = FakeSystem::new(Model::default());
        let report = trim_processes(&sys, &[OWN_PID, 3000], working_sets(&sys));
        assert_eq!(report.total, 1);
        assert!(!sys.calls().contains(&Call::Trim(OWN_PID)));
    }
}
