//! Smart cleaning: the per-level operation chains and the adaptive leftover
//! sweep.

use anyhow::Result;

use super::Cleaner;
use super::level::CleanLevel;
use super::operation::{Operation, PlannedStep, SECOND_PASS_SUFFIX};
use super::operations::ntstatus_failure;
use super::progress::Progress;
use super::report::{CleanResult, SmartCleanResult, free_delta, larger_delta};
use super::settle::SettleMode;
use crate::memory::{MemorySnapshot, format_bytes};
use crate::platform::nt::MemoryListCommand;

/// Maximum number of adaptive leftover sweeps after a chain's final purge.
pub(super) const MAX_SWEEP_PASSES: u32 = 2;

/// Standby plus modified memory in bytes: what another flush + purge could
/// still reclaim (the flush writes both pagefile-backed and mapped-file
/// modified pages). Counts the same lists as [`CleanLevel::estimate`].
/// `None` when the page lists are unknown.
fn leftover_bytes(snapshot: &MemorySnapshot) -> Option<u64> {
    let lists = snapshot.lists.as_ref()?;
    Some((lists.total_standby_pages() + lists.modified_pages).saturating_mul(snapshot.page_size))
}

/// Leftovers below this are normal background churn and not worth another
/// pass: 1% of physical RAM, at least 64 MB.
pub(super) fn sweep_threshold(total_physical: u64) -> u64 {
    (total_physical / 100).max(64 * 1024 * 1024)
}

/// Whether another sweep is worthwhile given the current and previous
/// leftover sizes.
///
/// Requires leftovers above [`sweep_threshold`], and after a first pass also
/// requires that pass to have shrunk them by at least a quarter. Less progress
/// than that means the system refills the cache as fast as it is purged
/// (`SysMain` prefetching, heavy file I/O) and more passes would only burn time.
pub(super) fn should_sweep(leftover: u64, previous: Option<u64>, total_physical: u64) -> bool {
    leftover >= sweep_threshold(total_physical)
        && previous.is_none_or(|prev| leftover.saturating_mul(4) < prev.saturating_mul(3))
}

/// The steps `level` runs, in order: the plan `--dry-run` previews and the
/// GUI counts progress against.
///
/// `has_excludes` picks the per-process working-set trim for `Aggressive`
/// and `Nuclear` (the only levels that empty working sets). The final
/// leftover sweep is listed but only runs when needed (see
/// [`Operation::is_optional`]).
#[must_use]
pub fn dry_run_plan(level: CleanLevel, has_excludes: bool) -> Vec<PlannedStep> {
    use Operation::{
        CombinePages, EmptyWorkingSetsKernel, EmptyWorkingSetsPerProcess, FlushFileCache,
        FlushModified, FlushRegistry, LeftoverSweep, PurgeStandby,
    };

    let working_sets = if has_excludes {
        EmptyWorkingSetsPerProcess
    } else {
        EmptyWorkingSetsKernel
    };
    let first: &[Operation] = match level {
        CleanLevel::Gentle => &[PurgeStandby],
        CleanLevel::Moderate => &[FlushModified, PurgeStandby, LeftoverSweep],
        CleanLevel::Aggressive | CleanLevel::Nuclear => &[
            FlushFileCache,
            FlushRegistry,
            working_sets,
            FlushModified,
            PurgeStandby,
        ],
    };
    let step = |operation, second_pass| PlannedStep {
        operation,
        second_pass,
    };
    let mut plan: Vec<PlannedStep> = first.iter().map(|&op| step(op, false)).collect();
    match level {
        CleanLevel::Gentle | CleanLevel::Moderate => {}
        CleanLevel::Aggressive => plan.push(step(LeftoverSweep, false)),
        CleanLevel::Nuclear => plan.extend([
            step(CombinePages, false),
            step(FlushModified, true),
            step(PurgeStandby, true),
            step(LeftoverSweep, false),
        ]),
    }
    plan
}

impl Cleaner<'_> {
    /// Run the operations of `level` in the optimal order for maximum RAM
    /// recovery.
    ///
    /// | Level | Operations |
    /// |---|---|
    /// | **Gentle** | Purge ALL standby (all priorities) |
    /// | **Moderate** | Flush modified list → Purge ALL standby → leftover sweep |
    /// | **Aggressive** | File cache flush → Registry flush → Empty working sets → Flush modified → Purge ALL standby → leftover sweep |
    /// | **Nuclear** | Aggressive + memory combining + second flush/purge → leftover sweep |
    ///
    /// When `exclude_names` is non-empty, working sets are emptied per process
    /// so the named processes keep their pages. Only Aggressive and Nuclear
    /// empty working sets, so exclusions have no effect at lower levels.
    pub fn smart_clean(
        &mut self,
        level: CleanLevel,
        exclude_names: &[String],
    ) -> Result<SmartCleanResult> {
        let overall_before = self.sys.snapshot()?;
        let start = std::time::Instant::now();

        let mut results = match level {
            // Standby pages are already outside every process's working set,
            // so purging them is safe at any time.
            CleanLevel::Gentle => vec![self.purge_standby()?],
            // No working-set eviction: running processes are unaffected; only
            // triggers an I/O spike while dirty pages are written out.
            CleanLevel::Moderate => {
                let mut results = Vec::with_capacity(4);
                self.flush_and_purge(&mut results)?;
                results
            }
            CleanLevel::Aggressive => self.aggressive_chain(exclude_names)?,
            CleanLevel::Nuclear => self.nuclear_chain(exclude_names)?,
        };

        if level >= CleanLevel::Moderate {
            self.leftover_sweep(&mut results)?;
        }

        // Each operation already settles internally, so just capture final state
        let overall_after = self.sys.snapshot()?;
        let total_freed =
            overall_after.available_physical as i64 - overall_before.available_physical as i64;
        let total_free_delta = free_delta(&overall_before, &overall_after);
        Ok(SmartCleanResult {
            total_freed,
            total_free_delta,
            total_reclaimed: larger_delta(total_freed, total_free_delta),
            total_elapsed_secs: start.elapsed().as_secs_f64(),
            results,
            overall_before,
            overall_after,
        })
    }

    /// Flush the modified list, then purge all standby pages.
    ///
    /// Both use a full settle: the modified page writer finishes its I/O
    /// asynchronously, and pages still in flight when the purge runs land on
    /// the standby list right afterwards as leftovers.
    fn flush_and_purge(&mut self, results: &mut Vec<CleanResult>) -> Result<()> {
        results.push(self.memory_list_op(MemoryListCommand::FlushModifiedList, SettleMode::Full)?);
        results.push(self.memory_list_op(MemoryListCommand::PurgeStandbyList, SettleMode::Full)?);
        Ok(())
    }

    /// File cache flush → Registry flush → Empty working sets → Flush
    /// modified → Purge ALL standby.
    fn aggressive_chain(&mut self, exclude_names: &[String]) -> Result<Vec<CleanResult>> {
        let mut results = vec![
            self.file_cache_op(SettleMode::Quick)?,
            self.registry_op(SettleMode::Quick)?,
        ];
        results.push(if exclude_names.is_empty() {
            self.memory_list_op(MemoryListCommand::EmptyWorkingSets, SettleMode::Quick)?
        } else {
            self.per_process_trim(exclude_names, SettleMode::Quick)?
        });
        self.flush_and_purge(&mut results)?;
        Ok(results)
    }

    /// The aggressive chain, then memory combining, then a second flush +
    /// purge for the pages that combining released or dirtied.
    fn nuclear_chain(&mut self, exclude_names: &[String]) -> Result<Vec<CleanResult>> {
        let mut results = self.aggressive_chain(exclude_names)?;
        results.push(self.combine_op(SettleMode::Quick)?);

        self.report(Progress::SecondPass);
        let second_pass_start = results.len();
        self.flush_and_purge(&mut results)?;
        // Label the second pass so it matches the dry-run plan and users can
        // tell the passes apart.
        for result in &mut results[second_pass_start..] {
            result.operation.push_str(SECOND_PASS_SUFFIX);
        }
        Ok(results)
    }

    /// Re-run flush + purge while meaningful leftovers remain (see
    /// [`should_sweep`]). Each pass that runs is appended to `results`.
    fn leftover_sweep(&mut self, results: &mut Vec<CleanResult>) -> Result<()> {
        let mut previous: Option<u64> = None;

        for pass in 1..=MAX_SWEEP_PASSES {
            let before = self.sys.snapshot()?;
            let Some(leftover) = leftover_bytes(&before) else {
                return Ok(()); // page lists unavailable: nothing to measure against
            };
            if !should_sweep(leftover, previous, before.total_physical) {
                return Ok(());
            }
            previous = Some(leftover);

            self.report(Progress::Sweep {
                pass,
                leftover_bytes: leftover,
            });
            let name = format!("{} (pass {pass})", Operation::LeftoverSweep.name());
            let start = std::time::Instant::now();

            // A failed flush is not fatal: the purge still reclaims the standby part.
            if self
                .sys
                .memory_command(MemoryListCommand::FlushModifiedList)
                .is_ok()
            {
                self.wait_for_settle_silently(SettleMode::Quick)?;
            }

            let result = match self.sys.memory_command(MemoryListCommand::PurgeStandbyList) {
                Ok(()) => {
                    let after = self.wait_for_settle(SettleMode::Full)?;
                    let remaining =
                        leftover_bytes(&after).map_or_else(|| "unknown".to_owned(), format_bytes);
                    CleanResult::success(
                        &name,
                        format!("Leftovers {} -> {remaining}", format_bytes(leftover)),
                        &before,
                        &after,
                        start.elapsed(),
                    )
                }
                Err(status) => {
                    CleanResult::failure(&name, ntstatus_failure("Standby purge", status), &before)
                }
            };

            let failed = !result.success;
            results.push(result);
            if failed {
                return Ok(());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn dry_run_plan_operation_counts() {
        assert_eq!(dry_run_plan(CleanLevel::Gentle, false).len(), 1);
        assert_eq!(dry_run_plan(CleanLevel::Moderate, false).len(), 3);
        assert_eq!(dry_run_plan(CleanLevel::Aggressive, false).len(), 6);
        assert_eq!(dry_run_plan(CleanLevel::Nuclear, false).len(), 9);
    }

    #[test]
    fn dry_run_plan_with_excludes_shows_per_process() {
        let ops: Vec<Operation> = dry_run_plan(CleanLevel::Aggressive, true)
            .iter()
            .map(|step| step.operation)
            .collect();
        assert!(ops.contains(&Operation::EmptyWorkingSetsPerProcess));
        assert!(!ops.contains(&Operation::EmptyWorkingSetsKernel));
    }

    #[test]
    fn dry_run_plan_moderate_ops() {
        let labels: Vec<String> = dry_run_plan(CleanLevel::Moderate, false)
            .into_iter()
            .map(PlannedStep::label)
            .collect();
        assert_eq!(
            labels,
            [
                "Flush Modified List",
                "Purge All Standby",
                "Leftover Sweep (only if needed)"
            ]
        );
    }

    #[test]
    fn sweep_threshold_scales_with_ram_and_has_floor() {
        assert_eq!(sweep_threshold(4 * GIB), 64 * 1024 * 1024, "floor applies");
        assert_eq!(sweep_threshold(64 * GIB), 64 * GIB / 100, "1% of RAM");
    }

    #[test]
    fn should_sweep_requires_meaningful_leftovers() {
        assert!(!should_sweep(10 * 1024 * 1024, None, 16 * GIB));
        assert!(should_sweep(GIB, None, 16 * GIB));
    }

    #[test]
    fn should_sweep_stops_without_progress() {
        // The first pass only shrank leftovers from 1 GiB to 900 MiB: refilling.
        assert!(!should_sweep(900 * 1024 * 1024, Some(GIB), 16 * GIB));
        // Halved: keep going.
        assert!(should_sweep(GIB / 2, Some(GIB), 16 * GIB));
    }
}
