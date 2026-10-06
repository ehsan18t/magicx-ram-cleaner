# Architecture

MagicX RAM Cleaner is a single portable executable that is both a CLI and a GUI. This document describes how the code is organised, the rules that keep it that way, and why.

## Layers

```
app ──────────────────────── launcher: GUI or CLI, console, exit codes
 ├─ cli ──────────────────── arguments, dispatch, terminal output, monitor
 └─ gui ──────────────────── egui app, tray icon, settings
      │
      ├─ integration ─────── context menu, logon-task autostart
      ├─ engine ──────────── levels, operations, leftover sweep, measurement,
      │                      auto-clean policy (behind the MemorySystem trait)
      └─ memory ──────────── snapshots, per-process usage, byte formatting
           │
           platform ──────── every Win32 / NT call; the only unsafe code
```

| Layer | Responsibility | May use |
| --- | --- | --- |
| `platform` | Safe wrappers around every Win32 and NT call: memory queries and commands, processes, privileges, registry, Task Scheduler, console, windows, notifications, dialogs. | nothing (plus `strings`) |
| `memory` | Domain types: system snapshots, per-process usage, byte formatting. | `platform` |
| `engine` | What to do to free memory and how to measure it: cleaning levels, individual operations, settle detection, the leftover sweep, results, and the auto-clean policy. | `memory`, `platform` |
| `integration` | How the app hooks into Windows: the Explorer context menu and the autostart logon task. | `platform` |
| `cli` | Arguments, command dispatch, terminal output, the `monitor` loop and `--notify` summaries. | everything below |
| `gui` | The egui application, tray icon and persisted settings. | everything below |
| `app` | Chooses GUI or CLI, prepares the console, parses arguments, maps outcomes to exit codes. | everything |

`strings` holds user-facing text and may be used from any layer. `main.rs` only calls `app::run()`.

## Rules

1. **Dependencies point down.** A layer never imports a layer above it, and `cli` and `gui` never import each other. `tests/architecture.rs` fails the build on a violation.
2. **`unsafe` lives only in `platform`.** `lib.rs` denies `unsafe_code` crate-wide and `platform` is the single exception. Every `unsafe` block carries a `SAFETY:` comment, and every `unsafe fn` documents its contract.
3. **The engine never prints.** It reports `Progress` events; the CLI decides what to show (`--verbose`) and the GUI ignores them.
4. **The engine only reaches the OS through `MemorySystem`.** `WindowsMemory` is the production implementation. Tests drive the engine with a simulated memory system.
5. **Handles are owned.** Kernel handles are `std::os::windows::io::OwnedHandle` (see `platform::handle`); registry keys are `platform::registry::RegKey`. Nothing is closed by hand.
6. **No trust in the inherited environment for elevated decisions.** The process always runs elevated, but its environment comes from whoever launched it. System paths come from `GetSystemDirectoryW` / `GetSystemWindowsDirectoryW`, the user account from the process token, and files handed to `schtasks.exe` are written to a folder only SYSTEM and Administrators can access.

## Why these boundaries

- **One place for `unsafe`.** Memory cleaning needs a lot of FFI. Keeping it in one tree, behind safe functions with documented failure modes, means the rest of the code is ordinary safe Rust and every `unsafe` block can be audited in one place.
- **A testable engine.** The engine's decisions are where bugs hurt most: the wrong operation order, a sweep that never stops, or "freed" figures that are wrong. Behind `MemorySystem`, those decisions run against a page-accurate simulation in milliseconds (`src/engine/tests.rs`), with no admin rights and no effect on the machine.
- **Two front ends, one behaviour.** The CLI and the GUI share the engine and the auto-clean policy, so cleaning works the same whichever is used, and neither depends on the other.
- **A thin entry point.** `app` is the only code that knows both front ends exist, which keeps launch logic (console handling, GUI relaunch, exit codes) in one small, tested module.

## Measuring "freed" memory

Windows counts the standby cache as available memory, so purging it barely changes the Available figure; what changes is free memory (the zeroed and free page lists). Trimming working sets is the opposite. Every operation therefore records both deltas, and `reclaimed_bytes()` reports the larger one. Snapshots include the kernel page-list breakdown whenever the process has `SeProfileSingleProcessPrivilege`.

## Adding things

- **A new OS call:** add a safe wrapper to the matching `platform` module (or a new one), with a `SAFETY:` comment on each `unsafe` block.
- **A new cleaning operation:** add it to `MemorySystem` (and `WindowsMemory`), implement it as a `Cleaner` method in `engine/operations.rs`, teach the simulation in `engine/fake.rs` how it moves pages, and add a behaviour test.
- **A new CLI command:** add the variant in `cli/args.rs` and its handling in `cli/commands.rs`.
- **A new GUI panel:** add a file in `gui/panels/` and an entry in `gui/nav.rs`.
