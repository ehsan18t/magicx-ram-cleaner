# Contributing to MagicX RAM Cleaner

## Quality Standards

This project enforces **zero-tolerance** for bad code through multiple automated layers.
If any of these fail, your code **will not compile, commit, or merge**.

### Layer 1: Compiler (instant feedback)

```toml
# Cargo.toml — these are set to DENY, not warn
clippy::all + clippy::pedantic + clippy::nursery = deny
```

```rust
// main.rs — crate-level enforcement
#![deny(unused_must_use, dead_code, unused_variables, unsafe_code, ...)]
// unsafe_code is denied globally; modules that need FFI get #[allow(unsafe_code)]
```

**What this means:** Clippy won't just warn you — it will **refuse to compile** your code
if it finds lint violations. Clippy lints at **deny** level mean compile errors, not warnings.

### Layer 2: Pre-commit hook (local gate)

Install once after cloning:
```powershell
.\scripts\install-hooks.ps1
```

Before every commit, this automatically runs:
1. `cargo fmt --check` — formatting
2. `cargo clippy` — all lints at deny level
3. `cargo test` — all tests must pass

If any step fails, the commit is **rejected**.

Additionally, a **commit-msg** hook validates that all commit messages follow
[Conventional Commits](https://www.conventionalcommits.org/) format:
```
<type>(<optional-scope>): <lowercase description>
```
Allowed types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
`build`, `ci`, `chore`, `revert`, `enforce`.

### Layer 3: CI Pipeline (GitHub Actions)

Every pull request to `main` runs 6 gates:
1. **Formatting** — `cargo fmt --check`
2. **Clippy** — deny-level lints with `RUSTFLAGS="-D warnings"`
3. **Tests** — `cargo test --all-targets`
4. **Release build** — confirms release profile compiles
5. **Documentation** — `cargo doc` with `-D warnings`
6. **Dependency audit** — `cargo deny check`

### Layer 4: Dependency auditing (`deny.toml`)

- Only MIT/Apache-2.0/BSD/MPL-2.0 licenses allowed
- Known-vulnerable crates are **denied**
- Only crates.io sources (no random git repos)
- Duplicate dependency versions flagged

## Development Workflow

```powershell
# Format your code
cargo fmt

# Check for lint issues (will error, not warn)
cargo clippy

# Run tests
cargo test

# Full release build
cargo build --release

# Audit dependencies (optional, requires cargo-deny)
cargo deny check
```

## Unsafe Code Policy

- `unsafe` is `#![deny]` at crate level
- Individual modules that need FFI get `#[allow(unsafe_code)]` on the `mod` declaration
- Every `unsafe {}` block **must** have a `// SAFETY:` comment explaining why it's sound
- Never add new `unsafe` without reviewing the invariants

## What Happens If You Try to Write Bad Code

| You try to...                 | What happens                                       |
| ----------------------------- | -------------------------------------------------- |
| Leave an unused variable      | **Compile error** (`deny(unused_variables)`)       |
| Add dead code                 | **Compile error** (`deny(dead_code)`)              |
| Skip formatting               | **Commit rejected** (pre-commit hook)              |
| Use `dbg!()`                  | **Compile error** (clippy disallowed-macros)       |
| Use `todo!()`                 | **Compile error** (clippy disallowed-macros)       |
| Write a 150-line function     | **Compile error** (`too_many_lines` at deny level) |
| Ignore a `Result`             | **Compile error** (`deny(unused_must_use)`)        |
| Add `unsafe` in a safe module | **Compile error** (`deny(unsafe_code)`)            |
| Add a GPL dependency          | **cargo deny** rejects it                          |
| Push without tests passing    | **CI blocks the merge**                            |
