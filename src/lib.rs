// ─── Compiler-enforced quality gates ─────────────────────────────────────────
// These cannot be overridden by individual modules. Any violation = build failure.
#![deny(
    // Correctness
    unused_must_use,         // ignoring Result/Option is a bug
    unreachable_patterns,    // dead match arms = confusion
    // Safety — unsafe is denied globally; modules that need FFI get #[allow(unsafe_code)]
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
//! The most powerful Windows RAM cleaner — CLI + GUI in a single portable exe.
//! Surpasses `EmptyStandbyList` with granular control over every memory subsystem.
//!
//! Double-click the binary (or run without arguments) to launch the built-in
//! egui graphical interface with a real-time dashboard, one-click cleaning,
//! process inspector, and system-tray integration. Pass a CLI subcommand
//! (e.g. `clean`, `status`, `monitor`) for scriptable, terminal-based usage.
//!
//! This library crate exposes internal modules for benchmarking and testing.
//! `MagicX` RAM Cleaner is a **binary crate** — this `lib.rs` exists to
//! enable `criterion` benchmarks in `benches/`.
//!
//! **Do not depend on this as a library.** The public API is unstable and
//! may change without notice.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                    MagicX RAM Cleaner                        │
//! ├──────────┬──────────────┬───────────────┬───────────────────┤
//! │   CLI    │  Cleaner     │  Monitor      │  Display          │
//! │ (cli.rs) │  Engine      │  Loop (CLI)   │  Formatting       │
//! ├──────────┤              ├───────────────┤                   │
//! │ Console  │  Context     │  GUI (egui)   │  Stats            │
//! │ (ANSI)   │  Menu        │  + Tray Icon  │  (MemorySnapshot) │
//! │          │              │  + Auto-Clean │                   │
//! │          │              │  + Settings   │                   │
//! ├──────────┴──────────────┴───────────────┴───────────────────┤
//! │               NT Native API Bindings (ntapi)                │
//! │      NtSetSystemInformation / NtQuerySystemInformation      │
//! ├─────────────────────────────────────────────────────────────┤
//! │               Win32 API  (windows-sys)                      │
//! │  GlobalMemoryStatusEx, SetSystemFileCacheSize,              │
//! │  K32EmptyWorkingSet, SetProcessWorkingSetSizeEx             │
//! ├─────────────────────────────────────────────────────────────┤
//! │               Privilege Manager                              │
//! │  SeProfileSingleProcessPrivilege, SeIncreaseQuotaPrivilege  │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Why `MagicX` is better than `EmptyStandbyList`
//!
//! | Feature | EmptyStandbyList | MagicX |
//! |---|---|---|
//! | Standby list purge | ✓ | ✓ |
//! | Low-priority only purge | ✓ | ✓ |
//! | Working set empty | ✓ | ✓ (kernel-level AND per-process) |
//! | Modified list flush | ✓ | ✓ |
//! | File cache flush | ✗ | ✓ |
//! | Registry cache flush | ✗ | ✓ |
//! | Memory combining/dedup | ✗ | ✓ |
//! | Smart multi-step cleaning | ✗ | ✓ (4 levels) |
//! | Before/after reporting | ✗ | ✓ |
//! | Detailed memory list stats | ✗ | ✓ (per-priority breakdown) |
//! | Monitoring with auto-clean | ✗ | ✓ (CLI + GUI) |
//! | JSON output | ✗ | ✓ |
//! | Optimal operation ordering | ✗ | ✓ |
//! | Second-pass cleaning | ✗ | ✓ |
//! | Built-in GUI | ✗ | ✓ (egui dashboard, tray icon, settings) |
//! | Desktop context menu | ✗ | ✓ (right-click submenu) |

/// Core memory cleaning operations and orchestration.
#[allow(unsafe_code)]
pub mod cleaner;

/// Centralised user-facing text constants for CLI and GUI.
pub mod strings;

/// Command-line interface definitions (clap parser, subcommands, help text).
pub mod cli;

/// Windows console utilities (dynamic attachment, ANSI colours, pause,
/// notifications, dark-mode detection, and title-bar theming).
#[allow(unsafe_code)]
pub mod console;

/// Windows context menu integration (install/uninstall registry entries).
#[allow(unsafe_code)]
pub mod context_menu;

/// Terminal display and formatting (banner, status, clean output, box drawing).
#[allow(unsafe_code)]
pub mod display;

/// Continuous monitoring loop with auto-clean and Ctrl+C handling.
#[allow(unsafe_code)]
pub mod monitor;

/// NT native API bindings for kernel memory operations.
#[allow(unsafe_code)]
pub mod ntapi;

/// Windows privilege elevation (`Se*Privilege`) and admin check.
#[allow(unsafe_code)]
pub mod privilege;

/// Memory statistics, Win32 API wrappers, and `MemorySnapshot`.
#[allow(unsafe_code)]
pub mod stats;

/// egui-based graphical user interface with dashboard, system-tray icon,
/// auto-clean monitoring, process inspector, and persistent settings.
/// Launched when the binary is executed with no CLI subcommand.
pub mod gui;
