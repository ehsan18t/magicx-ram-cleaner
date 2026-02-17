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
| Binary     | CLI tool — no GUI, no web, no library crate      |
| License    | MIT                                              |
| Min Rust   | latest stable (currently 1.93+)                  |
| Repository | `https://github.com/ehsan18t/magicx-ram-cleaner` |

---

## 2 · Coding Philosophy (non-negotiable)

1. **Zero-tolerance linting.** Clippy `all + pedantic + nursery` at **deny** level.
   Every lint violation is a compile error. Never `#[allow(...)]` a lint without a
   neighbouring comment explaining _why_.
2. **Unsafe is deny-by-default.** The crate root has `#![deny(unsafe_code)]`.
   Modules that require FFI get `#[allow(unsafe_code)]` on their `mod` item only.
   Every `unsafe {}` block **must** carry a `// SAFETY:` comment that explains
   which invariants are upheld.
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

```
src/
  main.rs        — CLI entry, clap Parser, command dispatch (no business logic)
  cleaner.rs     — cleaning operations & orchestration (smart_clean, CleanLevel)
  display.rs     — terminal formatting, box drawing, colour-coded output
  monitor.rs     — continuous monitoring loop, Ctrl+C handler, auto-clean
  ntapi.rs       — NT kernel FFI (NtSetSystemInformation, NtQuerySystemInformation)
  privilege.rs   — Windows privilege elevation (Se*Privilege)
  stats.rs       — memory statistics, Win32 API calls, MemorySnapshot
build.rs         — embeds admin-elevation manifest via embed-manifest
```

- **Do not create new modules** without explicit human approval.
- **Do not add new dependencies** without explicit human approval.
  If a feature can be implemented with `std`, `windows-sys`, or existing deps, do that.
- Keep FFI isolated in `ntapi.rs` and `privilege.rs`. Never scatter raw Win32
  calls across business-logic modules.
- `stats.rs` owns all memory-reading logic; `cleaner.rs` owns all memory-writing
  logic. Respect this boundary.

---

## 4 · Windows API Patterns

- Use `windows-sys` (not `windows`). It's zero-cost FFI bindings.
- All Win32 calls must check return values and convert errors via `anyhow`.
- Memory list operations go through `ntapi::execute_memory_command()`.
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
- Integration tests requiring admin privileges should be `#[ignore]`-d with a comment.
- Use `assert_eq!` with descriptive messages: `assert_eq!(result, expected, "reason")`.
- Run `cargo test` locally before pushing.

---

## 7 · Commit Message Rules

This project enforces **Conventional Commits** via a `commit-msg` git hook.

```
<type>(<optional-scope>): <lowercase description>
```

Allowed types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
`build`, `ci`, `chore`, `revert`, `enforce`.

Rules:

- Description starts lowercase, 5–72 characters.
- No trailing period.
- Scope is optional, lowercase, alphanumeric + hyphens.

Examples:

```
feat(monitor): add cooldown configuration flag
fix(ntapi): handle STATUS_ACCESS_DENIED gracefully
docs: update README with new subcommands
refactor(cleaner): extract wait_for_settle helper
```

---

## 8 · Git Hooks (install once)

```powershell
.\scripts\install-hooks.ps1
```

| Hook         | Gates                                                                |
| ------------ | -------------------------------------------------------------------- |
| `pre-commit` | `cargo fmt --check`, `cargo clippy`, `cargo test`                    |
| `pre-push`   | Full 6-gate CI mirror (fmt, clippy, test, release build, docs, deny) |
| `commit-msg` | Conventional Commits format validation                               |

---

## 9 · Documentation Update Rule ⚠️

**When you change behaviour, you MUST update documentation in the same commit.**

| What changed                     | Update these                               |
| -------------------------------- | ------------------------------------------ |
| New CLI flag / subcommand        | `--help` text (clap doc attrs), README.md  |
| New cleaning operation           | README.md, RUST_IMPLEMENTATION_GUIDE.md    |
| NT API usage change              | WINDOWS_MEMORY_INTERNALS.md                |
| Build / CI change                | CONTRIBUTING.md, README.md (badges)        |
| New module or architecture shift | This file, RUST_IMPLEMENTATION_GUIDE.md    |
| Dependency added / removed       | Cargo.toml, deny.toml (if license changes) |
| Hook / workflow change           | CONTRIBUTING.md                            |

If you are unsure whether a doc update is needed, **update it anyway**. Stale docs
are worse than verbose docs.

Update the relevant `--help` text when changing any CLI-facing behaviour. The help
text is defined as constants (`LONG_ABOUT`, `AFTER_HELP_SHORT`, `AFTER_HELP_LONG`)
in `main.rs`.

---

## 10 · CI Pipeline

CI runs on **pull requests to `main`** only (not on push). Two jobs:

1. **quality-gate** — fmt, clippy, test, release build, cargo doc
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

- ❌ Add `println!` for debugging — use `colored` output helpers in `display.rs`.
- ❌ Use `std::process::exit()` — return `anyhow::Result` and let `main()` handle it.
- ❌ Add cross-platform abstractions — this is Windows-only by design.
- ❌ Introduce async/await — the tool is synchronous and simple.
- ❌ Add a GUI or TUI framework — this is a CLI tool.
- ❌ Use `unwrap()` or `expect()` outside of tests.
- ❌ Add `#[allow(clippy::*)]` without a comment justifying it.
- ❌ Commit without running all quality gates.
- ❌ Change architecture without human approval.
- ❌ Skip doc updates when behaviour changes.

---

## 14 · Quick Reference for Common Tasks

### Adding a new cleaning operation:

1. Add the kernel command to `ntapi::MemoryListCommand` if needed.
2. Implement the function in `cleaner.rs` following existing patterns.
3. Add a `Commands` variant in `main.rs` with clap attributes and doc comment.
4. Wire it up in the `match` dispatch in `main()`.
5. Update README.md, `AFTER_HELP_LONG`, and RUST_IMPLEMENTATION_GUIDE.md.
6. Add tests for any pure logic.

### Adding a new CLI flag:

1. Add the field to the relevant clap struct in `main.rs`.
2. Use it in the dispatch logic.
3. Update `--help` text, README.md.
4. Test with `cargo run -- --help`.

### Updating a dependency:

1. Check latest version on crates.io (use MCP/internet if available).
2. Update version in `Cargo.toml`.
3. Run `cargo update`.
4. Run `cargo deny check` to verify license compatibility.
5. Run full test suite.
6. Update deny.toml if the license changed.
