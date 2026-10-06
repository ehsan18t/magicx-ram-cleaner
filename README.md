# MagicX RAM Cleaner

**A Windows RAM cleaner with full control over every memory subsystem. CLI + GUI in a single binary.**

MagicX goes beyond tools like EmptyStandbyList by providing granular control over Windows memory lists, smart multi-step cleaning, real-time monitoring with auto-clean, and detailed diagnostics. Double-click for the GUI, or use the command line for scripting and automation.

---

## Quick Start

```powershell
# GUI mode - just double-click the exe (requires Administrator)
magicx-ram-cleaner

# CLI mode - open PowerShell or CMD as Administrator
magicx-ram-cleaner clean

# Check how much RAM you freed
magicx-ram-cleaner status
```

For most users, `clean` is all you need.

---

## Installation

### Option A: Download Pre-built Binary

Download `magicx-ram-cleaner.exe` from the releases page and place it anywhere on your system (e.g., `C:\Tools\`).

### Option B: Build from Source

```powershell
git clone <repo-url>
cd magicx-ram-cleaner
cargo build --release
# Binary is at: target\release\magicx-ram-cleaner.exe
```

### Adding to PATH (optional)

```powershell
[Environment]::SetEnvironmentVariable("Path", "$env:PATH;C:\Tools", "User")
```

---

## Why MagicX?

### vs EmptyStandbyList

| Feature                         | EmptyStandbyList |       MagicX RAM Cleaner       |
| ------------------------------- | :--------------: | :----------------------------: |
| Purge standby list              |        Yes       |              Yes               |
| Purge low-priority standby only |        Yes       |              Yes               |
| Empty working sets              |        Yes       | Yes (kernel-level + per-process) |
| Flush modified pages            |        Yes       |              Yes               |
| File system cache flush         |        No        |              Yes               |
| Registry cache flush            |        No        |              Yes               |
| Memory page combining/dedup     |        No        |          Yes (Win10+)          |
| Smart multi-step cleaning       |        No        |          Yes (4 levels)        |
| Before/after RAM reporting      |        No        |              Yes               |
| Detailed memory list breakdown  |        No        |  Yes (per-priority standby)    |
| Monitoring with auto-clean      |        No        |              Yes               |
| JSON output for scripting       |        No        |              Yes               |
| GUI with real-time dashboard    |        No        |              Yes               |
| UAC auto-elevation (manifest)   |        No        |              Yes               |
| Single-file, no dependencies    |        Yes       |     Yes (portable exe)         |

### Key Advantages

1. **GUI + CLI in one binary** - Double-click for a graphical dashboard with real-time charts, or use the CLI for scripting. No other RAM cleaner offers both in a single exe.

2. **Smarter cleaning** - MagicX flushes modified pages *before* purging standby, so dirty pages get saved and then freed. EmptyStandbyList misses these entirely.

3. **File cache and registry flush** - The file system cache can consume gigabytes. MagicX flushes both the file cache and registry cache directly. EmptyStandbyList cannot.

4. **Memory combining** - Windows 10+ can deduplicate identical memory pages via copy-on-write. MagicX triggers this; EmptyStandbyList does not.

5. **Kernel-level working set trim** - Uses `NtSetSystemInformation(MemoryEmptyWorkingSets)`, a single kernel call that hits ALL processes including protected/system ones that per-process `EmptyWorkingSet()` cannot touch.

6. **Multi-pass cleaning** - The nuclear level does a second pass after memory combining to catch newly-modified pages.

---

## Technical Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the layers and the rules between them.

```
src/
+-- main.rs             # Entry point: calls app::run()
+-- lib.rs              # Crate root: layer overview, lint gates
+-- app.rs              # Launcher: GUI or CLI, console setup, exit codes
+-- strings.rs          # All user-facing text
+-- platform/           # Every Win32/NT call; the only unsafe code
|   +-- nt.rs           # NtSetSystemInformation / NtQuerySystemInformation
|   +-- memory.rs       # Memory status, page lists, file cache
|   +-- process.rs      # Process list, memory counters, working-set trim
|   +-- privilege.rs    # Admin check, Se*Privilege
|   +-- registry.rs     # Registry keys (RegKey, Hive)
|   +-- task_scheduler.rs # Logon tasks via schtasks.exe
|   +-- console.rs      # Console attach/alloc, ANSI, Ctrl+C
|   +-- window.rs       # Window lookup, cloaking, theming
|   +-- notify.rs       # Balloon notifications
|   +-- shell.rs        # Unelevated URL launch
|   +-- ...             # handle, identity, instance, dialog, paths, time, wide
+-- memory/             # Snapshots, per-process usage, byte formatting
+-- engine/             # Cleaning engine
|   +-- system.rs       # MemorySystem trait + WindowsMemory
|   +-- operations.rs   # Individual operations
|   +-- smart.rs        # Level chains, leftover sweep, dry-run plan
|   +-- settle.rs       # Waiting for the kernel to settle
|   +-- report.rs       # CleanResult / SmartCleanResult
|   +-- auto_clean.rs   # Auto-clean policy shared by CLI and GUI
|   +-- level.rs, progress.rs, fake.rs + tests.rs (simulation tests)
+-- integration/        # Context menu, logon-task autostart
+-- cli/                # args, commands, display, monitor, notification
+-- gui/                # egui interface
    +-- mod.rs          # run_gui() launcher
    +-- app/            # App state + eframe loop, cleaning, background, tray events
    +-- settings.rs     # Persisted settings, defaults, valid ranges
    +-- persistence.rs  # Settings file I/O, import/export
    +-- sidebar.rs      # Navigation
    +-- theme.rs, tray.rs, widgets.rs
    +-- panels/         # about, dashboard, monitor, processes, settings
tests/
+-- architecture.rs     # Enforces the layering and the unsafe boundary
```

### APIs Used

| API                        | Source       | Purpose                                                                  |
| -------------------------- | ------------ | ------------------------------------------------------------------------ |
| `NtSetSystemInformation`   | ntdll.dll    | Memory list commands (purge standby, flush modified, empty working sets) |
| `NtQuerySystemInformation` | ntdll.dll    | Query detailed memory list info                                          |
| `GlobalMemoryStatusEx`     | kernel32.dll | Physical/virtual memory stats                                            |
| `K32GetPerformanceInfo`    | kernel32.dll | Commit charge, kernel pools, system counters                             |
| `SetSystemFileCacheSize`   | kernel32.dll | File system cache management                                             |
| `K32EmptyWorkingSet`       | kernel32.dll | Per-process working set trim                                             |
| `OpenProcessToken`         | advapi32.dll | Token manipulation for privileges                                        |
| `AdjustTokenPrivileges`    | advapi32.dll | Enable required privileges                                               |
| `CreateToolhelp32Snapshot` | kernel32.dll | Process enumeration                                                      |
| `RegCreateKeyExW`          | advapi32.dll | Context menu registry key creation                                       |
| `RegDeleteTreeW`           | advapi32.dll | Context menu registry key removal                                        |

---

## Building from Source

### Prerequisites

- [Rust](https://rustup.rs/) 1.93 or newer (edition 2024)
- Windows 10 SDK (comes with Visual Studio Build Tools)
- Git

### Build Steps

```powershell
git clone <repo-url>
cd magicx-ram-cleaner
cargo build --release
```

The binary will be at `target\release\magicx-ram-cleaner.exe`.

### Development

The project always builds with the latest stable Rust (`rust-toolchain.toml` selects it, and rustup installs it on first use).

```powershell
.\scripts\install-hooks.ps1   # Once after cloning: fmt, clippy and tests run before every commit
cargo build                   # Debug build
cargo test                    # Unit, simulation and architecture tests
cargo clippy --all-targets    # Lints (deny level)
cargo run -- clean -l gentle -v
```

See the [Contributing Guide](docs/CONTRIBUTING.md) for the full quality gates and commit conventions.

---

## Documentation

- [Usage Guide](docs/USAGE.md) - Commands reference, cleaning levels, examples, FAQ
- [Architecture](docs/ARCHITECTURE.md) - Layers, dependency rules and design decisions
- [Contributing Guide](docs/CONTRIBUTING.md)
- [Rust Implementation Guide](docs/RUST_IMPLEMENTATION_GUIDE.md)
- [Windows Memory Internals](docs/WINDOWS_MEMORY_INTERNALS.md)
- [Security Policy](docs/SECURITY.md)

---

## Supported Systems

| OS                                  | Version          | Status                         |
| ----------------------------------- | ---------------- | ------------------------------ |
| Windows 10 IoT Enterprise LTSC 2021 | 21H2 (19044)     | Fully supported                |
| Windows 11                          | 24H2 and later   | Fully supported                |
| Windows 10                          | 21H2+            | Should work (tested on LTSC)   |
| Windows Server                      | 2019, 2022, 2025 | Should work                    |

**Requirements:**
- x86-64 (64-bit) processor
- Administrator privileges
- ~1 MB disk space

---

## License

MIT License - see [LICENSE](LICENSE) for details.
