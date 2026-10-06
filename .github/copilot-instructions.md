# Copilot & AI Agent Instructions — MagicX RAM Cleaner

> This document defines how AI coding agents (GitHub Copilot, Cursor, Windsurf,
> Claude, etc.) must interact with this codebase. Treat every rule here as a
> hard constraint unless the human operator explicitly overrides it.

---

## 1 · Project Identity

| Field      | Value                                            |
| ---------- | ------------------------------------------------ |
| Language   | Rust (edition **2024**)                          |
| Platform   | Windows only (x86-64)                            |
| Binary     | CLI + GUI tool — single portable exe             |
| License    | MIT                                              |
| Min Rust   | latest stable (currently 1.93+)                  |
| Repository | `https://github.com/ehsan18t/magicx-ram-cleaner` |

---

## 2 · Coding Philosophy (non-negotiable)

1. **Zero-tolerance linting.** Clippy `all + pedantic + nursery` at **deny** level.
   Every lint violation is a compile error. Never `#[allow(...)]` a lint without a
   neighbouring comment explaining _why_.
2. **Unsafe is deny-by-default.** `src/lib.rs` has `#![deny(unsafe_code)]`; the single exception is `#[allow(unsafe_code)] pub mod platform;`. No other module may opt out — all FFI goes into `src/platform/` (enforced by `tests/architecture.rs`). Every `unsafe {}` block **must** carry a `// SAFETY:` comment that explains which invariants are upheld, and every `unsafe fn` documents its contract.
3. **Error handling via `anyhow`.** Use `anyhow::Result` for fallible functions.
   Provide context with `.context()` / `.with_context()`. Never `unwrap()` in
   non-test code.
4. **Coloured terminal output via `colored` crate.** Use semantic colour mapping:
   green = success/good, yellow = warning/caution, red = error/critical,
   cyan = info/labels, bold = emphasis.
5. **Doc comments on every public item.** Clippy's `missing_docs` lint is active.
   Write idiomatic `///` doc comments. Use backticks for code identifiers
   (`EmptyStandbyList`, `SeDebugPrivilege`, etc.) to satisfy `doc_markdown`.
6. **Functions ≤ 100 lines** (`too_many_lines` at deny). Split large blocks into
   well-named helpers.
7. **Cognitive complexity ≤ 30** per function. Prefer early returns and guard
   clauses over deep nesting.
8. **No disallowed macros:** `dbg!()`, `todo!()`, `unimplemented!()` are banned.
   Use `anyhow::bail!` or proper error handling instead.

---

## 3 · Architecture Rules

Authoritative sources: `docs/ARCHITECTURE.md` (layers, rules, rationale) and the layer diagram in `src/lib.rs`. If this section and those disagree, they win — and fix this section.

Layers, from the entry point down:

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

```
src/
  main.rs             — binary entry point: one line, magicx_ram_cleaner::app::run()
  lib.rs              — crate root: layer diagram, crate-wide lint gates (deny unsafe_code except platform)
  app.rs              — launcher: GUI or CLI, console setup, argument parsing, outcome → exit code
  strings.rs          — all user-facing text (usable from every layer)
  platform/           — every Win32 / NT call; the ONLY place unsafe is allowed
    nt.rs             — NT FFI: NtSetSystemInformation / NtQuerySystemInformation, MemoryListCommand, NtStatus,
                        execute_memory_command(), execute_combine_memory(), execute_registry_flush()
    memory.rs         — memory_status(), performance_info(), MemoryListInfo, FileCacheSnapshot, flush_system_file_cache()
    process.rs        — process list (ProcessEntry), memory counters, empty_working_set()
    privilege.rs      — check_admin(), enable_privilege(), enable_all_privileges()
    handle.rs         — owned_or_null() / owned_or_invalid(): raw HANDLE → std OwnedHandle
    registry.rs       — RegKey (owned key), Hive, create/set/delete/exists helpers
    task_scheduler.rs — logon tasks via schtasks.exe
    console.rs        — console attach/alloc, ANSI, Ctrl+C (watch_interrupts / interrupted), pause-before-exit
    window.rs         — main-window lookup, cloaking to tray, dark/light title bar and menus
    notify.rs         — balloon notifications (--notify)
    instance.rs       — SingleInstance guard for the GUI
    shell.rs          — unelevated URL launch through Explorer's token
    dialog.rs         — native JSON open/save dialogs
    identity.rs       — the account this process runs as (from the token)
    paths.rs          — system directories via GetSystemDirectoryW / GetSystemWindowsDirectoryW
    time.rs, wide.rs  — local time; UTF-16 string conversions
  memory/             — domain types
    mod.rs            — MemorySnapshot, QuickMemoryReading; re-exports MemoryListInfo, FileCacheSnapshot
    process.rs        — ProcessMemoryInfo, query_top_processes(), query_all_processes()
    format.rs         — format_bytes(), format_signed_bytes()
  engine/             — cleaning engine (never prints; OS access only via MemorySystem)
    mod.rs            — Cleaner { sys, on_progress }: Cleaner::new(sys, cb) / Cleaner::silent(sys)
    system.rs         — MemorySystem trait + WindowsMemory (production impl over platform)
    operations.rs     — single operations as Cleaner methods + command_labels()
    smart.rs          — Cleaner::smart_clean(level, exclude_names), level chains, leftover sweep, dry_run_plan()
    settle.rs         — SettleMode, Cleaner::wait_for_settle()
    level.rs          — CleanLevel
    report.rs         — CleanResult, SmartCleanResult
    progress.rs       — Progress events
    auto_clean.rs     — AutoCleanPolicy + Decision (threshold, cooldown, backoff; shared by CLI and GUI)
    fake.rs           — #[cfg(test)] page-accurate simulated MemorySystem (FakeSystem, Model, Call)
    tests.rs          — #[cfg(test)] engine behaviour tests against FakeSystem
  integration/        — Windows integration
    context_menu.rs   — Desktop right-click submenu install/uninstall
    autostart.rs      — elevated logon task (set_enabled / is_enabled)
  cli/                — command-line front end
    mod.rs            — cli::run(command, quiet, notify) -> Outcome
    args.rs           — clap Cli, Commands, ContextMenuAction, LevelArg, help text constants, STYLES
    commands.rs       — dispatch(): runs each subcommand
    display.rs        — ALL terminal formatting (banner, status, results, progress_printer())
    monitor.rs        — run_monitor(): continuous monitoring + auto-clean
    notification.rs   — --notify balloon summaries
  gui/                — egui front end
    mod.rs            — run_gui() launcher
    app/
      mod.rs          — MagicXApp state, eframe::App impl, Panel enum
      cleaning.rs     — cleans on a worker thread, results, auto-clean monitor
      background.rs   — background threads for memory stats and the process list
      tray_events.rs  — tray icon events and rebuilds
    settings.rs       — GuiSettings: persisted fields, defaults, valid ranges (defined once)
    persistence.rs    — SettingsManager: settings file I/O, import/export
    sidebar.rs        — navigation sidebar + panel routing
    theme.rs          — colour palette, spacing constants, dark/light Visuals
    tray.rs           — system tray icon with context menu and Phosphor glyph icons
    widgets.rs        — reusable UI components (cards, stat labels, toggle switch)
    panels/           — one file per tab: about, dashboard, monitor, processes, settings
tests/
  architecture.rs     — fails the build on an upward/sibling import or unsafe outside platform
build.rs              — embeds admin-elevation manifest, application icon, Phosphor context-menu sub-icons (IDs 2–6), and version metadata
assets/
  app.ico             — multi-size application icon (16–256 px) embedded as resource ID 1
  app.png             — PNG version of the app icon used as the egui window icon
```

Rules (hard constraints; the first two are enforced by `tests/architecture.rs`):

- **Dependencies point down:** `platform` ← `memory` ← `engine` / `integration` ← `cli` / `gui` ← `app`. A layer never imports a layer above it. `cli` and `gui` never import each other; `engine` and `integration` never import each other. `strings` is usable from anywhere.
- **All `unsafe` FFI lives under `src/platform/`.** Never add raw Win32/NT calls (or `#[allow(unsafe_code)]`) anywhere else. Higher layers call safe `platform` wrappers with documented failure modes.
- **The engine never prints.** No `println!`/`eprintln!`/`colored` in `src/engine/`. It emits `Progress` events through the `Cleaner`'s callback; the CLI renders them (`cli::display::progress_printer`, for `--verbose`) and the GUI ignores them.
- **The engine reaches the OS only through the `MemorySystem` trait.** Never call `platform` or `MemorySnapshot::capture()` directly from engine logic; go through `self.sys`. A new OS operation needs a `MemorySystem` method, its `WindowsMemory` implementation, a matching update to the simulation in `src/engine/fake.rs` (`FakeSystem`), and a behaviour test in `src/engine/tests.rs`.
- **Handles are owned.** Kernel handles are `std::os::windows::io::OwnedHandle` via `platform::handle`; registry keys are `platform::registry::RegKey`. Never close handles by hand.
- **Don't trust the inherited environment for elevated decisions.** System paths come from `platform::paths`, the user account from the process token (`platform::identity`), never from environment variables.
- `main.rs` stays a one-liner; launch logic belongs in `app.rs`.
- **Do not create new modules** without explicit human approval.
- **Do not add new dependencies** without explicit human approval.
  If a feature can be implemented with `std`, `windows-sys`, or existing deps, do that.

---

## 4 · Windows API Patterns

- Use `windows-sys` (not `windows`). It's zero-cost FFI bindings.
- All Win32 calls must check return values and convert errors via `anyhow`.
- Memory list operations go through `platform::nt::execute_memory_command()`, which the engine reaches only via `MemorySystem::memory_command()` (implemented by `WindowsMemory`).
- Type casts across the FFI boundary (`u32↔i32`, `usize→u32`) are allowed —
  see the `cast_*` lint allows in `Cargo.toml`.
- Privilege names are string constants (`"SeProfileSingleProcessPrivilege"`, etc.).
  Never hard-code token numeric values.

---

## 5 · Formatting & Style

- **rustfmt** with `edition = "2024"`, `max_width = 100`.
- Run `cargo fmt` before every commit.
- Use `snake_case` for functions/variables, `PascalCase` for types/enums,
  `SCREAMING_SNAKE_CASE` for constants.
- Prefer `const` over `static` where possible.
- Line comments (`//`) for implementation notes; doc comments (`///`) for API docs.

---

## 6 · Testing

- Write unit tests for all pure/deterministic logic (formatting, enums, calculations).
- Tests live in `#[cfg(test)] mod tests` inside each module.
- Engine behaviour (operation order, settling, leftover sweep, measurement) is tested in `src/engine/tests.rs` against the simulated `FakeSystem` in `src/engine/fake.rs` — no admin rights, no effect on the machine. Any engine change gets a behaviour test there.
- `tests/architecture.rs` enforces the layering and the `unsafe` boundary; never weaken it to make a change compile — fix the dependency instead.
- Integration tests requiring admin privileges should be `#[ignore]`-d with a comment.
- Use `assert_eq!` with descriptive messages: `assert_eq!(result, expected, "reason")`.
- Run `cargo test` locally before pushing.

---

## 7 · Commit Rules

### 7.1 · Commit After Every Completed Task

Agents **must** commit immediately after completing each discrete task or fix.
Do not batch multiple unrelated changes into a single commit. Each commit
should represent **one logical change** that can be understood, reviewed, and
reverted independently.

- Fix one bug → commit. Fix the next bug → commit again.
- Refactor one module → commit. Then move to the next.
- Add a feature → commit. Update its docs → commit (or same commit if tightly coupled).
- Never leave uncommitted work when moving to a different task.

### 7.2 · Conventional Commits Format

This project enforces **Conventional Commits** via a `commit-msg` git hook.

```
<type>(<optional-scope>): <lowercase description>
```

Allowed types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
`build`, `ci`, `chore`, `revert`, `enforce`.

Rules:

- Description starts lowercase, 5–200 characters.
- No trailing period.
- Scope is optional, lowercase, alphanumeric + hyphens.

### 7.3 · Commit Description Quality

Commit messages must be **concise yet descriptive**. The subject line should
clearly explain _what_ changed so a reviewer can understand without reading
the diff. Use the commit body (via `-m` flags) for _why_ and _how_ when the
change is non-trivial.

Good examples:

```
fix(platform): free SID memory on all error paths
refactor(engine): extract wait_for_settle helper
perf(memory): avoid redundant GlobalMemoryStatusEx call
fix(platform): validate command enum before FFI call
docs: update architecture section for new modules
```

Bad examples (too vague):

```
fix: fix bug
refactor: cleanup
chore: updates
```

---

## 8 · Git Hooks (install once)

```powershell
.\scripts\install-hooks.ps1
```

| Hook         | Gates                                                                       |
| ------------ | --------------------------------------------------------------------------- |
| `pre-commit` | `cargo fmt --check`, `cargo clippy`, `cargo test`                           |
| `pre-push`   | 6-gate quality gate (fmt, clippy, test, docs, deny if installed, debug build) |
| `commit-msg` | Conventional Commits format validation                                      |

---

## 9 · Documentation Update Rule ⚠️

**When you change behaviour, you MUST update documentation in the same commit.**

| What changed                     | Update these                                 |
| -------------------------------- | -------------------------------------------- |
| New CLI flag / subcommand        | `--help` text (clap doc attrs), README.md    |
| New cleaning operation           | README.md, docs/RUST_IMPLEMENTATION_GUIDE.md |
| NT API usage change              | docs/WINDOWS_MEMORY_INTERNALS.md             |
| Build / CI change                | docs/CONTRIBUTING.md, README.md (badges)     |
| New module or architecture shift | docs/ARCHITECTURE.md, `src/lib.rs` diagram, README.md tree, this file, docs/RUST_IMPLEMENTATION_GUIDE.md |
| Dependency added / removed       | Cargo.toml, deny.toml (if license changes)   |
| Hook / workflow change           | docs/CONTRIBUTING.md                         |

If you are unsure whether a doc update is needed, **update it anyway**. Stale docs
are worse than verbose docs.

Update the relevant `--help` text when changing any CLI-facing behaviour. The help
text is defined as constants (`LONG_ABOUT`, `AFTER_HELP_SHORT`, `AFTER_HELP_LONG`)
in `src/cli/args.rs`.

---

## 10 · CI Pipeline

CI runs on **pull requests to `main`** only (not on push). Two jobs, **7 gates total**:

1. **quality-gate** — fmt, clippy, test, bench compile (`--no-run`), debug build, cargo doc
2. **audit** — `cargo deny check`

All gates must pass before merge. See `.github/workflows/ci.yml`.

---

## 11 · Dependency Policy

- Prefer `std` over external crates.
- Only MIT / Apache-2.0 / BSD / MPL-2.0 licensed crates.
- `cargo deny check` must pass (see `deny.toml`).
- Pin major versions in `Cargo.toml` (e.g., `"4"` not `"*"`).
- Run `cargo update` periodically to pull latest patch versions.

---

## 12 · MCP & Internet Usage

When available, agents **should** use MCP tools and internet access to:

- Look up latest crate versions on crates.io before suggesting dependency changes.
- Fetch up-to-date Rust / Windows API documentation via context7 or similar.
- Check GitHub issues / PRs for context on reported problems.
- Verify NT API structures and constants against Microsoft documentation.

Do **not** blindly trust cached knowledge about Windows internals or crate APIs.
Always verify against current sources when the information is critical.

---

## 13 · What NOT to Do

- ❌ Add `println!` for debugging — terminal output belongs in `src/cli/display.rs` (`colored` helpers); the engine never prints, it emits `Progress` events.
- ❌ Put `unsafe` or raw Win32/NT calls outside `src/platform/`.
- ❌ Import upward (e.g. `engine` → `cli`) or between siblings (`cli` ↔ `gui`, `engine` ↔ `integration`).
- ❌ Call the OS from engine logic except through `MemorySystem`.
- ❌ Use `std::process::exit()` — return `anyhow::Result` / `cli::Outcome` and let `app::run()` map it to an `ExitCode`.
- ❌ Add cross-platform abstractions — this is Windows-only by design.
- ❌ Introduce async/await — the tool is synchronous and simple.
- ❌ Add a new GUI framework — the project uses egui/eframe; do not replace it.
- ❌ Use `unwrap()` or `expect()` outside of tests.
- ❌ Add `#[allow(clippy::*)]` without a comment justifying it.
- ❌ Commit without running all quality gates.
- ❌ Change architecture without human approval.
- ❌ Skip doc updates when behaviour changes.

---

## 14 · Quick Reference for Common Tasks

### Adding a new OS call:

1. Add a safe wrapper to the matching `src/platform/` module (or a new one, with approval), with a `// SAFETY:` comment on each `unsafe` block and documented failure modes.

### Adding a new cleaning operation:

1. If it needs a new OS call, add the safe wrapper in `src/platform/` (e.g. a variant in `platform::nt::MemoryListCommand`, plus its labels in `command_labels()` in `src/engine/operations.rs`).
2. Add a method to the `MemorySystem` trait (`src/engine/system.rs`) and implement it for `WindowsMemory` — unless an existing method such as `memory_command()` already covers it.
3. Implement the operation as a `Cleaner` method in `src/engine/operations.rs`, following the existing capture → execute → settle → `CleanResult` pattern; report progress with `Progress` events, never print.
4. Teach the simulation in `src/engine/fake.rs` (`FakeSystem`) how the operation moves pages, and add a behaviour test in `src/engine/tests.rs`.
5. If it belongs in a level chain, update `src/engine/smart.rs` (chain and `dry_run_plan()`).
6. To expose it on the CLI, follow "Adding a new CLI command" below.
7. Update README.md, `AFTER_HELP_LONG`, and docs/RUST_IMPLEMENTATION_GUIDE.md.

### Adding a new CLI command:

1. Add the `Commands` variant in `src/cli/args.rs` with clap attributes and a doc comment.
2. Handle it in `dispatch()` in `src/cli/commands.rs`; put any terminal formatting in `src/cli/display.rs`.
3. Update `--help` text constants and README.md.
4. Test with `cargo run -- --help`.

### Adding a new CLI flag:

1. Add the field to the relevant clap struct/variant in `src/cli/args.rs`.
2. Use it in `src/cli/commands.rs` (or `src/app.rs` for global launch flags).
3. Update `--help` text, README.md.
4. Test with `cargo run -- --help`.

### Adding a new GUI panel:

1. Add a file in `src/gui/panels/` with a `draw()` function and re-export it from `src/gui/panels/mod.rs`.
2. Add a `Panel` variant in `src/gui/app/mod.rs`.
3. Add its navigation entry and routing arm in `src/gui/sidebar.rs`, with any new text in `src/strings.rs`.
4. New persisted settings go in `GuiSettings` (`src/gui/settings.rs`), with defaults and ranges defined there once.

### Updating a dependency:

1. Check latest version on crates.io (use MCP/internet if available).
2. Update version in `Cargo.toml`.
3. Run `cargo update`.
4. Run `cargo deny check` to verify license compatibility.
5. Run full test suite.
6. Update deny.toml if the license changed.
