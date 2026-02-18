//! Criterion benchmarks for `MagicX` RAM Cleaner hot paths.
//!
//! Focuses on functions called in tight loops or frequently during
//! cleaning operations. Does NOT benchmark operations that require
//! Administrator privileges or modify system state.
//!
//! Run with: `cargo bench`

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use magicx_ram_cleaner::ntapi;
use magicx_ram_cleaner::stats::{
    MemoryListInfo, MemorySnapshot, QuickMemoryReading, extract_exe_name, format_bytes,
};

// ─── format_bytes ────────────────────────────────────────────────────────────

/// Benchmark `format_bytes` across all magnitude ranges (B → TB).
///
/// This function is called for every memory value in status displays and
/// cleaning summaries — often 20+ times per command invocation.
fn bench_format_bytes(c: &mut Criterion) {
    let mut group = c.benchmark_group("format_bytes");

    let inputs: &[(&str, u64)] = &[
        ("0_bytes", 0),
        ("500_bytes", 500),
        ("1_kb", 1024),
        ("1.5_mb", 1_572_864),
        ("3.42_gb", 3_671_891_558),
        ("16_gb", 17_179_869_184),
        ("1_tb", 1_099_511_627_776),
    ];

    for &(name, value) in inputs {
        group.bench_with_input(BenchmarkId::new("format", name), &value, |b, &v| {
            b.iter(|| format_bytes(black_box(v)));
        });
    }

    group.finish();
}

// ─── extract_exe_name ────────────────────────────────────────────────────────

/// Benchmark `extract_exe_name` with realistic process name buffers.
///
/// Called once per process during `enumerate_processes`, which iterates
/// all running processes (typically 100–400 on a desktop system).
fn bench_extract_exe_name(c: &mut Criterion) {
    let mut group = c.benchmark_group("extract_exe_name");

    // Short process name (common case)
    let short: Vec<u16> = "cmd.exe\0".encode_utf16().collect();
    group.bench_function("short_7chars", |b| {
        b.iter(|| extract_exe_name(black_box(&short)));
    });

    // Longer name (Chrome helper, 21 chars)
    let long: Vec<u16> = "GoogleChromeHelper.exe\0\0\0\0".encode_utf16().collect();
    group.bench_function("long_21chars", |b| {
        b.iter(|| extract_exe_name(black_box(&long)));
    });

    // Full 260-char PROCESSENTRY32W buffer with short name
    // (realistic: real buffers are always 260 u16s, mostly null-padded)
    let mut full_buf = vec![0u16; 260];
    for (i, unit) in "svchost.exe".encode_utf16().enumerate() {
        full_buf[i] = unit;
    }
    group.bench_function("full_260_buffer", |b| {
        b.iter(|| extract_exe_name(black_box(&full_buf)));
    });

    group.finish();
}

// ─── ntstatus_message ────────────────────────────────────────────────────────

/// Benchmark `ntstatus_message` — a const fn match on NTSTATUS codes.
///
/// Called once per failed operation to produce a human-readable error.
/// Included to verify the const match is zero-cost at runtime.
fn bench_ntstatus_message(c: &mut Criterion) {
    let mut group = c.benchmark_group("ntstatus_message");

    group.bench_function("success", |b| {
        b.iter(|| ntapi::ntstatus_message(black_box(0)));
    });

    group.bench_function("known_error", |b| {
        b.iter(|| ntapi::ntstatus_message(black_box(ntapi::STATUS_INFO_LENGTH_MISMATCH)));
    });

    // Unknown code — worst case: falls through all match arms
    group.bench_function("unknown_fallthrough", |b| {
        b.iter(|| ntapi::ntstatus_message(black_box(0x7FFF_FFFF)));
    });

    group.finish();
}

// ─── MemorySnapshot::capture ─────────────────────────────────────────────────

/// Benchmark the two snapshot capture paths.
///
/// - `capture_full`:  `GlobalMemoryStatusEx` + `K32GetPerformanceInfo` (2 Win32 calls)
/// - `capture_quick`: `GlobalMemoryStatusEx` only (1 Win32 call)
///
/// These are the most performance-sensitive Win32 calls in the tool:
/// - `capture_full` is called before/after every cleaning operation
/// - `capture_quick` is called in the settle-detection polling loop (every 100ms)
fn bench_memory_capture(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_capture");

    group.bench_function("full_snapshot", |b| {
        b.iter(|| {
            let snap = MemorySnapshot::capture().expect("capture should succeed");
            black_box(snap);
        });
    });

    group.bench_function("quick_reading", |b| {
        b.iter(|| {
            let reading = QuickMemoryReading::capture().expect("capture should succeed");
            black_box(reading);
        });
    });

    group.finish();
}

// ─── Pure calculations ───────────────────────────────────────────────────────

/// Benchmark pure calculation methods on snapshot/list structs.
///
/// These are trivial but included to establish a performance baseline
/// and ensure no accidental regressions (e.g. from a future refactor
/// that adds expensive formatting or I/O).
fn bench_snapshot_calculations(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot_calculations");

    // commit_percent: division + multiply on u64 fields
    let snapshot = MemorySnapshot {
        memory_load_percent: 75,
        total_physical: 16 * 1024 * 1024 * 1024,
        available_physical: 4 * 1024 * 1024 * 1024,
        used_physical: 12 * 1024 * 1024 * 1024,
        total_page_file: 20 * 1024 * 1024 * 1024,
        available_page_file: 15 * 1024 * 1024 * 1024,
        total_virtual: 128 * 1024 * 1024 * 1024,
        available_virtual: 120 * 1024 * 1024 * 1024,
        commit_total_pages: 800_000,
        commit_limit_pages: 1_200_000,
        commit_peak_pages: 900_000,
        physical_available_pages: 1_048_576,
        physical_total_pages: 4_194_304,
        kernel_paged_pages: 100_000,
        kernel_nonpaged_pages: 50_000,
        page_size: 4096,
        handle_count: 85_000,
        process_count: 300,
        thread_count: 4000,
    };

    group.bench_function("commit_percent", |b| {
        b.iter(|| black_box(&snapshot).commit_percent());
    });

    // total_standby_pages: sum of 8-element array
    let list_info = MemoryListInfo {
        zeroed_pages: 1000,
        free_pages: 500,
        modified_pages: 200,
        modified_no_write_pages: 10,
        bad_pages: 0,
        standby_pages: [100, 200, 300, 400, 500, 600, 700, 800],
        repurposed_pages: [0; 8],
        modified_pagefile_pages: 50,
    };

    group.bench_function("total_standby_pages", |b| {
        b.iter(|| black_box(&list_info).total_standby_pages());
    });

    group.finish();
}

// ─── Criterion harness ───────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_format_bytes,
    bench_extract_exe_name,
    bench_ntstatus_message,
    bench_memory_capture,
    bench_snapshot_calculations,
);
criterion_main!(benches);
