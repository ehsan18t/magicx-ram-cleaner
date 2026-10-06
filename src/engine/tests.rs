//! Behaviour tests for the cleaning engine against the simulated memory
//! system in [`super::fake`].
//!
//! These pin down what each level does and in which order, how exclusions,
//! failures and the leftover sweep behave, and how freed memory is measured,
//! so a refactor cannot change cleaning behaviour silently.

use super::fake::{Call, FakeSystem, GIB_PAGES, Model, PAGE};
use super::smart::MAX_SWEEP_PASSES;
use super::{CleanLevel, Cleaner, Progress, SmartCleanResult, dry_run_plan};
use crate::platform::nt::MemoryListCommand::{
    EmptyWorkingSets, FlushModifiedList, PurgeStandbyList,
};

const GIB: i64 = 1024 * 1024 * 1024;

/// Run `level` against `sys`, collecting progress events.
fn run(
    sys: &FakeSystem,
    level: CleanLevel,
    excludes: &[&str],
) -> (SmartCleanResult, Vec<Progress>) {
    let excludes: Vec<String> = excludes.iter().map(|&s| s.to_owned()).collect();
    let mut events = Vec::new();
    let result = Cleaner::new(sys, |p| events.push(p))
        .smart_clean(level, &excludes)
        .expect("clean succeeds");
    (result, events)
}

fn operation_names(result: &SmartCleanResult) -> Vec<&str> {
    result
        .results
        .iter()
        .map(|r| r.operation.as_str())
        .collect()
}

/// The dry-run plan's labels without the sweep, which only runs when needed.
fn plan_without_sweep(level: CleanLevel, has_excludes: bool) -> Vec<String> {
    dry_run_plan(level, has_excludes)
        .into_iter()
        .filter(|step| !step.operation.is_optional())
        .map(super::PlannedStep::label)
        .collect()
}

// ─── Operation order per level ───────────────────────────────────────────────

#[test]
fn gentle_only_purges_standby() {
    let sys = FakeSystem::default();
    run(&sys, CleanLevel::Gentle, &[]);
    assert_eq!(sys.calls(), vec![Call::Command(PurgeStandbyList)]);
}

#[test]
fn moderate_flushes_modified_before_purging() {
    let sys = FakeSystem::default();
    run(&sys, CleanLevel::Moderate, &[]);
    assert_eq!(
        sys.calls(),
        vec![
            Call::Command(FlushModifiedList),
            Call::Command(PurgeStandbyList)
        ]
    );
}

#[test]
fn aggressive_runs_full_chain_in_order() {
    let sys = FakeSystem::default();
    run(&sys, CleanLevel::Aggressive, &[]);
    assert_eq!(
        sys.calls(),
        vec![
            Call::FlushFileCache,
            Call::FlushRegistry,
            Call::Command(EmptyWorkingSets),
            Call::Command(FlushModifiedList),
            Call::Command(PurgeStandbyList),
        ]
    );
}

#[test]
fn nuclear_adds_combining_and_a_second_pass() {
    let sys = FakeSystem::default();
    let (_, events) = run(&sys, CleanLevel::Nuclear, &[]);
    assert_eq!(
        sys.calls(),
        vec![
            Call::FlushFileCache,
            Call::FlushRegistry,
            Call::Command(EmptyWorkingSets),
            Call::Command(FlushModifiedList),
            Call::Command(PurgeStandbyList),
            Call::Combine,
            Call::Command(FlushModifiedList),
            Call::Command(PurgeStandbyList),
        ]
    );
    assert!(events.contains(&Progress::SecondPass));
}

#[test]
fn executed_operations_match_the_dry_run_plan() {
    let levels = [
        CleanLevel::Gentle,
        CleanLevel::Moderate,
        CleanLevel::Aggressive,
        CleanLevel::Nuclear,
    ];
    for level in levels {
        for excludes in [&[][..], &["chrome"][..]] {
            let sys = FakeSystem::default();
            let (result, _) = run(&sys, level, excludes);
            let plan = plan_without_sweep(level, !excludes.is_empty());
            assert_eq!(
                operation_names(&result),
                plan,
                "{level} with excludes {excludes:?}"
            );
        }
    }
}

// ─── Exclusions ──────────────────────────────────────────────────────────────

#[test]
fn exclusions_switch_to_per_process_trimming() {
    let sys = FakeSystem::default();
    let (result, events) = run(&sys, CleanLevel::Aggressive, &["Chrome.EXE"]);

    let calls = sys.calls();
    assert!(
        !calls.contains(&Call::Command(EmptyWorkingSets)),
        "kernel trim would hit chrome"
    );
    assert!(
        !calls.contains(&Call::Trim(2000)),
        "excluded process was trimmed"
    );
    assert!(
        !calls.contains(&Call::Trim(super::fake::OWN_PID)),
        "trimmed itself"
    );
    assert!(calls.contains(&Call::Trim(3000)));
    assert!(events.contains(&Progress::Excluded {
        name: "chrome.exe".to_owned(),
        pid: 2000
    }));

    let trim = &result.results[2];
    assert_eq!(trim.operation, "Empty Working Sets (Per-Process)");
    assert_eq!(
        trim.message,
        "Trimmed 1 processes, 1 skipped (protected/system), 1 excluded by name"
    );
}

#[test]
fn a_failed_process_listing_fails_only_the_trim_step() {
    let sys = FakeSystem::new(Model {
        processes: Vec::new(), // the fake fails to list an empty process table
        ..Model::default()
    });
    let (result, _) = run(&sys, CleanLevel::Aggressive, &["chrome"]);

    let trim = &result.results[2];
    assert_eq!(trim.operation, "Empty Working Sets (Per-Process)");
    assert!(!trim.success);
    assert!(trim.message.starts_with("Could not list processes"));
    // The rest of the chain still ran after the failed step.
    assert!(sys.calls().contains(&Call::Command(PurgeStandbyList)));
    assert_eq!(result.results.len(), 5);
}

#[test]
fn exclusions_have_no_effect_below_aggressive() {
    for level in [CleanLevel::Gentle, CleanLevel::Moderate] {
        let sys = FakeSystem::default();
        run(&sys, level, &["chrome"]);
        assert!(
            !sys.calls().iter().any(|c| matches!(c, Call::Trim(_))),
            "{level} trimmed processes"
        );
    }
}

// ─── Failures ────────────────────────────────────────────────────────────────

#[test]
fn failed_operation_is_reported_and_the_chain_continues() {
    let sys = FakeSystem::new(Model {
        failing_command: Some((EmptyWorkingSets, 0xC000_0061_u32 as i32)),
        ..Model::default()
    });
    let (result, _) = run(&sys, CleanLevel::Aggressive, &[]);

    assert_eq!(result.failed_count(), 1);
    let failed = &result.results[2];
    assert!(!failed.success);
    assert_eq!(failed.freed_bytes, 0);
    assert!(
        failed.message.contains("STATUS_PRIVILEGE_NOT_HELD"),
        "{}",
        failed.message
    );
    // The flush and purge after it still ran.
    assert!(result.results[3].success && result.results[4].success);
}

#[test]
fn failed_purge_stops_the_sweep_after_one_pass() {
    // The purge does its work but reports failure, and leftovers shrink
    // from 4 GiB to 1 GiB: only the stop-on-failure rule prevents pass 2.
    let sys = FakeSystem::new(Model {
        failing_command: Some((PurgeStandbyList, 0xC000_0022_u32 as i32)),
        failure_still_applies: true,
        refill_after_purge: [4 * GIB_PAGES, GIB_PAGES].into(),
        ..Model::default()
    });
    let (result, _) = run(&sys, CleanLevel::Moderate, &[]);

    let names = operation_names(&result);
    assert_eq!(
        names,
        vec![
            "Flush Modified List",
            "Purge All Standby",
            "Leftover Sweep (pass 1)"
        ]
    );
    assert_eq!(result.failed_count(), 2);
}

// ─── Leftover sweep ──────────────────────────────────────────────────────────

#[test]
fn sweep_reclaims_pages_that_land_after_the_purge() {
    // 1 GiB comes back right after the purge, then nothing.
    let sys = FakeSystem::new(Model {
        refill_after_purge: [GIB_PAGES].into(),
        ..Model::default()
    });
    let (result, events) = run(&sys, CleanLevel::Moderate, &[]);

    assert_eq!(
        operation_names(&result),
        vec![
            "Flush Modified List",
            "Purge All Standby",
            "Leftover Sweep (pass 1)"
        ]
    );
    assert!(events.contains(&Progress::Sweep {
        pass: 1,
        leftover_bytes: GIB_PAGES * PAGE
    }));
    assert_eq!(sys.model.borrow().standby, 0, "sweep left standby behind");
}

#[test]
fn sweep_stops_when_the_cache_refills_as_fast_as_it_is_purged() {
    // A constant 1 GiB refill after every purge: one sweep, then give up.
    let sys = FakeSystem::new(Model {
        refill_after_purge: [GIB_PAGES; 5].into(),
        ..Model::default()
    });
    let (result, _) = run(&sys, CleanLevel::Moderate, &[]);
    let sweeps = operation_names(&result)
        .iter()
        .filter(|n| n.starts_with("Leftover Sweep"))
        .count();
    assert_eq!(sweeps, 1);
}

#[test]
fn sweep_is_bounded_even_while_making_progress() {
    // Leftovers halve every pass: progress each time, but the pass cap holds.
    let sys = FakeSystem::new(Model {
        refill_after_purge: [4 * GIB_PAGES, 2 * GIB_PAGES, GIB_PAGES, GIB_PAGES / 2].into(),
        ..Model::default()
    });
    let (result, _) = run(&sys, CleanLevel::Moderate, &[]);
    let sweeps = operation_names(&result)
        .iter()
        .filter(|n| n.starts_with("Leftover Sweep"))
        .count();
    assert_eq!(sweeps, MAX_SWEEP_PASSES as usize);
}

#[test]
fn small_leftovers_do_not_trigger_a_sweep() {
    // 32 MiB is below the 1% of 16 GiB threshold.
    let sys = FakeSystem::new(Model {
        refill_after_purge: [GIB_PAGES / 32].into(),
        ..Model::default()
    });
    let (result, _) = run(&sys, CleanLevel::Moderate, &[]);
    assert_eq!(result.results.len(), 2);
}

#[test]
fn sweep_needs_page_list_data() {
    let sys = FakeSystem::new(Model {
        lists_available: false,
        refill_after_purge: [GIB_PAGES].into(),
        ..Model::default()
    });
    let (result, _) = run(&sys, CleanLevel::Aggressive, &[]);
    assert_eq!(
        result.results.len(),
        5,
        "swept without being able to measure"
    );
}

#[test]
fn gentle_never_sweeps() {
    let sys = FakeSystem::new(Model {
        refill_after_purge: [GIB_PAGES].into(),
        ..Model::default()
    });
    run(&sys, CleanLevel::Gentle, &[]);
    assert_eq!(sys.calls(), vec![Call::Command(PurgeStandbyList)]);
}

// ─── Measurement ─────────────────────────────────────────────────────────────

#[test]
fn purging_standby_counts_as_freed_memory() {
    // Standby counts as available, so a purge barely moves Available; the
    // freed figure must come from the free-list delta (4 GiB of standby).
    let sys = FakeSystem::default();
    let (result, _) = run(&sys, CleanLevel::Gentle, &[]);
    assert_eq!(result.total_freed, 0);
    assert_eq!(result.total_free_delta, Some(4 * GIB));
    assert_eq!(result.reclaimed_bytes(), 4 * GIB);
}

#[test]
fn freed_memory_falls_back_to_available_without_page_lists() {
    let sys = FakeSystem::new(Model {
        lists_available: false,
        ..Model::default()
    });
    let (result, _) = run(&sys, CleanLevel::Aggressive, &[]);
    assert_eq!(result.total_free_delta, None);
    // File cache (0.5 GiB) + trimmed working sets (2 GiB) + modified (1 GiB).
    assert_eq!(result.reclaimed_bytes(), 3 * GIB + GIB / 2);
}

#[test]
fn aggressive_frees_everything_reclaimable() {
    let sys = FakeSystem::default();
    let (result, _) = run(&sys, CleanLevel::Aggressive, &[]);
    // Standby 4 + modified 1 + trimmed 2 + file cache 0.5 GiB all end up free.
    assert_eq!(result.reclaimed_bytes(), 7 * GIB + GIB / 2);
    assert!(result.results.iter().all(|r| r.success));
}

// ─── Single operations ───────────────────────────────────────────────────────

#[test]
fn single_operations_issue_one_call_each() {
    let sys = FakeSystem::default();
    let mut cleaner = Cleaner::silent(&sys);
    cleaner.flush_file_cache().unwrap();
    cleaner.flush_registry_cache().unwrap();
    cleaner.empty_working_sets().unwrap();
    cleaner.flush_modified().unwrap();
    cleaner.purge_standby().unwrap();
    cleaner.combine_memory().unwrap();
    assert_eq!(
        sys.calls(),
        vec![
            Call::FlushFileCache,
            Call::FlushRegistry,
            Call::Command(EmptyWorkingSets),
            Call::Command(FlushModifiedList),
            Call::Command(PurgeStandbyList),
            Call::Combine,
        ]
    );
}

// ─── Settle modes ────────────────────────────────────────────────────────────

/// Settle times reported during a run. The simulated memory is stable at
/// once, so a Quick settle reports 100 ms and a Full settle 300 ms: the
/// sequence pins the settle mode of every operation.
fn settle_times(events: &[Progress]) -> Vec<u64> {
    events
        .iter()
        .filter_map(|event| match event {
            Progress::Settled { after_ms } => Some(*after_ms),
            _ => None,
        })
        .collect()
}

const QUICK: u64 = 100;
const FULL: u64 = 300;

#[test]
fn each_level_uses_the_expected_settle_modes() {
    // Quick for synchronous operations; Full for the modified flush and the
    // standby purge, whose write-back finishes asynchronously.
    let expected: [(CleanLevel, &[u64]); 4] = [
        (CleanLevel::Gentle, &[FULL]),
        (CleanLevel::Moderate, &[FULL, FULL]),
        (CleanLevel::Aggressive, &[QUICK, QUICK, QUICK, FULL, FULL]),
        (
            CleanLevel::Nuclear,
            &[QUICK, QUICK, QUICK, FULL, FULL, QUICK, FULL, FULL],
        ),
    ];
    for (level, times) in expected {
        let (_, events) = run(&FakeSystem::default(), level, &[]);
        assert_eq!(settle_times(&events), times, "{level}");
    }
}

#[test]
fn sweep_reports_only_its_final_settle() {
    let sys = FakeSystem::new(Model {
        refill_after_purge: [GIB_PAGES].into(),
        ..Model::default()
    });
    let (_, events) = run(&sys, CleanLevel::Moderate, &[]);
    // Flush and purge, then one sweep pass whose inner wait stays silent.
    assert_eq!(settle_times(&events), [FULL, FULL, FULL]);
}

/// Run one standby purge (a Full settle) with scripted settle readings.
fn purge_with_readings(offsets: impl IntoIterator<Item = u64>) -> Vec<Progress> {
    let sys = FakeSystem::new(Model {
        reading_offsets: offsets.into_iter().collect(),
        ..Model::default()
    });
    let mut events = Vec::new();
    Cleaner::new(&sys, |p| events.push(p))
        .purge_standby()
        .unwrap();
    events
}

#[test]
fn settling_starts_over_when_memory_jumps() {
    const JUMP: u64 = 100 * 1024 * 1024;
    // Readings: the start, two still polls, a jump, then still again. The
    // jump resets the count, so three still polls are needed after it.
    let events = purge_with_readings([0, 0, 0, JUMP, JUMP, JUMP, JUMP]);
    assert_eq!(settle_times(&events), [600]);
}

#[test]
fn small_jitter_counts_as_settled() {
    // Moves below the 4 MB floor are kernel jitter, not settling.
    let events = purge_with_readings([0, 1024, 2048, 1024]);
    assert_eq!(settle_times(&events), [FULL]);
}

#[test]
fn settling_gives_up_after_the_timeout() {
    // Memory moves by 100 MB on every poll for longer than 20 polls.
    let events = purge_with_readings((0..30).map(|i| i * 100 * 1024 * 1024));
    assert_eq!(settle_times(&events), [] as [u64; 0]);
    assert!(events.contains(&Progress::SettleTimedOut { after_ms: 2000 }));
}

#[test]
fn failed_flushes_and_combining_are_reported_and_the_chain_goes_on() {
    let sys = FakeSystem::new(Model {
        failing_file_cache: Some(5),
        failing_registry: Some(0xC000_0022_u32 as i32),
        failing_combine: Some(0xC000_0002_u32 as i32),
        ..Model::default()
    });
    let (result, _) = run(&sys, CleanLevel::Nuclear, &[]);
    let failed: Vec<&str> = result
        .results
        .iter()
        .filter(|r| !r.success)
        .map(|r| r.operation.as_str())
        .collect();
    assert_eq!(
        failed,
        [
            "Flush File Cache",
            "Flush Registry Cache",
            "Memory Combining"
        ]
    );
    assert_eq!(result.failed_count(), 3);
    // The second flush and purge still ran after combining failed.
    assert_eq!(
        sys.calls()
            .iter()
            .filter(|c| **c == Call::Command(PurgeStandbyList))
            .count(),
        2
    );
}

#[test]
fn single_operations_settle_fully() {
    let sys = FakeSystem::default();
    let mut events = Vec::new();
    let mut cleaner = Cleaner::new(&sys, |p| events.push(p));
    cleaner.flush_file_cache().unwrap();
    cleaner.empty_working_sets().unwrap();
    cleaner.purge_standby().unwrap();
    drop(cleaner);
    assert_eq!(settle_times(&events), [FULL, FULL, FULL]);
}
