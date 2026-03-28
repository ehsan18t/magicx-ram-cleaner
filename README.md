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

```
src/
+-- main.rs           # Entry point: mod declarations, main(), run(), dispatch
+-- lib.rs            # Library crate root: module re-exports for benchmarks
+-- cli.rs            # CLI definitions: clap Parser, Commands enum, help text
+-- cleaner.rs        # Core cleaning operations + smart clean engine
+-- console.rs        # Windows console management (attach/alloc, ANSI, notifications)
+-- context_menu.rs   # Desktop context menu integration (registry keys)
+-- display.rs        # Terminal formatting: banner, status, clean output
+-- gui/              # egui graphical interface module
|   +-- mod.rs        # Module entry, run_gui() launcher
|   +-- app.rs        # Core app state, eframe::App impl, sidebar, layout routing
|   +-- persistence.rs # Settings I/O, Win32 file dialogs, autostart registry
|   +-- theme.rs      # Colour palette, spacing, dark/light themes
|   +-- tray.rs       # System tray icon with context menu
|   +-- widgets.rs    # Reusable UI components (cards, stat labels, toggle)
|   +-- panels/       # One file per tab
|       +-- about.rs      # App info, developer profile
|       +-- dashboard.rs  # Memory overview + one-click cleaning
|       +-- monitor.rs    # Auto-clean configuration
|       +-- processes.rs  # Sortable grouped process memory table
|       +-- settings.rs   # Appearance, integration, backup & restore
+-- monitor.rs        # Continuous monitoring loop with auto-clean
+-- ntapi.rs          # NT Native API FFI (NtSetSystemInformation)
+-- privilege.rs      # Windows privilege management + admin check
+-- stats.rs          # Memory statistics (GlobalMemoryStatusEx, GetPerformanceInfo)
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

```powershell
cargo build          # Debug build
cargo test           # Run tests
cargo clippy         # Check for warnings
cargo run -- clean -l gentle -v
```

---

## Documentation

- [Usage Guide](docs/USAGE.md) - Commands reference, cleaning levels, examples, FAQ
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
