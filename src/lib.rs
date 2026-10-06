// ─── Compiler-enforced quality gates ─────────────────────────────────────────
// These cannot be overridden by individual modules. Any violation = build failure.
#![deny(
    // Correctness
    unused_must_use,         // ignoring Result/Option is a bug
    unreachable_patterns,    // dead match arms = confusion
    // Safety - unsafe is denied everywhere except the `platform` module tree
    unsafe_code,
    unsafe_op_in_unsafe_fn,  // unsafe blocks inside unsafe fn must be explicit
    // Quality
    unused_imports,          // dead imports = sloppy code
    unused_variables,        // unused vars = incomplete work
    dead_code,               // dead code = maintenance burden
    // Documentation
    rustdoc::broken_intra_doc_links,
)]

//! # `MagicX` RAM Cleaner
//!
//! A Windows RAM cleaner: CLI and GUI in a single portable exe, with control
//! over the standby, modified, working-set, file-cache, registry and
//! page-combining memory subsystems.
//!
//! Double-click the binary (or run it without arguments) for the egui
//! interface; pass a subcommand (`clean`, `status`, `monitor`, ...) for the
//! scriptable CLI.
//!
//! This library exists so the binary, tests and `criterion` benchmarks share
//! one code base. **Do not depend on it as a library**: the API is unstable.
//!
//! ## Architecture
//!
//! Layers, from the entry point down. A module may only depend on modules
//! below it (`strings` holds user-facing text and is usable everywhere).
//!
//! ```text
//! app ──────────────────────── launcher: GUI or CLI, console, exit codes
//!  ├─ cli ──────────────────── arguments, dispatch, terminal output, monitor
//!  └─ gui ──────────────────── egui app, tray icon, settings
//!       │
//!       ├─ integration ─────── context menu, logon-task autostart
//!       ├─ engine ──────────── levels, operations, leftover sweep, measurement,
//!       │                      auto-clean policy (behind the MemorySystem trait)
//!       └─ memory ──────────── snapshots, per-process usage, byte formatting
//!            │
//!            platform ──────── every Win32 / NT call; the only `unsafe` code
//! ```
//!
//! See `docs/ARCHITECTURE.md` for the reasoning behind these boundaries.

/// Every Win32 and NT call: the only layer allowed to use `unsafe`.
#[allow(unsafe_code)]
pub mod platform;

/// Memory domain types: system snapshots, per-process usage, byte formatting.
pub mod memory;

/// The cleaning engine: levels, operations, leftover sweep and measurement.
pub mod engine;

/// Windows integration: Explorer context menu and logon-task autostart.
pub mod integration;

/// Command-line interface: arguments, dispatch, output and the monitor.
pub mod cli;

/// egui-based graphical user interface with dashboard, system-tray icon,
/// auto-clean monitoring, process inspector, and persistent settings.
/// Launched when the binary is executed with no CLI subcommand.
pub mod gui;

/// Application launcher: chooses GUI or CLI, console setup, exit codes.
pub mod app;

/// Centralised user-facing text constants for CLI and GUI.
pub mod strings;

/// Fixed names the app is looked up by (window title, task name).
pub mod ids;
