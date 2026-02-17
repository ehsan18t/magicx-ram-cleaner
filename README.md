# MagicX RAM Cleaner

**The world's most powerful Windows RAM cleaner CLI tool.**

MagicX RAM Cleaner goes far beyond tools like EmptyStandbyList by providing granular control over every Windows memory subsystem, smart multi-step cleaning, real-time monitoring with auto-clean, and detailed diagnostics — all in a single ~580 KB binary.

---

## Table of Contents

- [Quick Start](#quick-start)
- [Installation](#installation)
- [Why MagicX?](#why-magicx)
- [Commands Reference](#commands-reference)
  - [clean](#clean--smart-clean)
  - [status](#status--memory-diagnostics)
  - [purge-standby](#purge-standby--standby-list-purge)
  - [flush-modified](#flush-modified--modified-page-flush)
  - [empty-workingsets](#empty-workingsets--working-set-trim)
  - [flush-cache](#flush-cache--file-system-cache-flush)
  - [combine](#combine--memory-page-deduplication)
  - [monitor](#monitor--continuous-monitoring)
- [Cleaning Levels Explained](#cleaning-levels-explained)
- [Usage Examples](#usage-examples)
  - [For Normal Users](#for-normal-users)
  - [For Gamers](#for-gamers)
  - [For Power Users / Sysadmins](#for-power-users--sysadmins)
  - [For Automation / Scripts](#for-automation--scripts)
- [How Windows Memory Works](#how-windows-memory-works)
- [Comparison with Other Tools](#comparison-with-other-tools)
- [FAQ](#faq)
- [Building from Source](#building-from-source)
- [Technical Architecture](#technical-architecture)
- [Supported Systems](#supported-systems)
- [License](#license)

---

## Quick Start

```powershell
# 1. Open PowerShell or CMD as Administrator (required!)
#    Right-click → "Run as administrator"

# 2. Run an aggressive clean (recommended for most users)
magicx-ram-cleaner clean

# 3. Check how much RAM you freed
magicx-ram-cleaner status
```

That's it. For most users, `clean` is all you need.

---

## Installation

### Option A: Download Pre-built Binary

Download `magicx-ram-cleaner.exe` from the releases page and place it anywhere on your system (e.g., `C:\Tools\`).

### Option B: Build from Source

```powershell
# Requires Rust toolchain (https://rustup.rs)
git clone <repo-url>
cd magicx-ram-cleaner
cargo build --release
# Binary is at: target\release\magicx-ram-cleaner.exe
```

### Adding to PATH (optional)

```powershell
# Add the directory to your user PATH so you can run it from anywhere
[Environment]::SetEnvironmentVariable("Path", "$env:PATH;C:\Tools", "User")
```

---

## Why MagicX?

### vs EmptyStandbyList

| Feature                         | EmptyStandbyList |       MagicX RAM Cleaner       |
| ------------------------------- | :--------------: | :----------------------------: |
| Purge standby list              |        ✅         |               ✅                |
| Purge low-priority standby only |        ✅         |               ✅                |
| Empty working sets              |        ✅         | ✅ Kernel-level AND per-process |
| Flush modified pages            |        ✅         |               ✅                |
| File system cache flush         |        ❌         |               ✅                |
| Memory page combining/dedup     |        ❌         |               ✅                |
| Smart multi-step cleaning       |        ❌         |           ✅ 4 levels           |
| Before/after RAM reporting      |        ❌         |               ✅                |
| Detailed memory list breakdown  |        ❌         |  ✅ Per-priority standby stats  |
| Monitoring with auto-clean      |        ❌         |               ✅                |
| JSON output for scripting       |        ❌         |               ✅                |
| Optimal operation ordering      |        ❌         |               ✅                |
| Second-pass cleaning            |        ❌         |               ✅                |
| UAC auto-elevation (manifest)   |        ❌         |               ✅                |
| Single-file, no dependencies    |        ✅         |        ✅ ~580 KB binary        |

### Key Advantages

1. **Smarter cleaning**: MagicX flushes modified pages *before* purging standby, which means pages that were dirty get saved and then freed. EmptyStandbyList misses these.

2. **File cache control**: The file system cache can consume gigabytes of RAM. MagicX can flush it directly — EmptyStandbyList cannot.

3. **Memory combining**: Windows 10+ can deduplicate identical memory pages using copy-on-write. MagicX triggers this; EmptyStandbyList doesn't support it.

4. **Kernel-level working set trim**: MagicX uses `NtSetSystemInformation(MemoryEmptyWorkingSets)` — a single kernel call that hits ALL processes including protected/system processes that per-process `EmptyWorkingSet()` cannot touch.

5. **Multi-pass cleaning**: The nuclear level does a second pass after memory combining to catch newly-modified pages.

---

## Commands Reference

### `clean` — Smart Clean

The recommended way to free RAM. Runs multiple operations in optimal order.

```
magicx-ram-cleaner clean [OPTIONS]
```

**Options:**
| Flag                  | Description                                                                      |
| --------------------- | -------------------------------------------------------------------------------- |
| `-l, --level <LEVEL>` | Cleaning aggressiveness: `gentle`, `moderate`, `aggressive` (default), `nuclear` |
| `-v, --verbose`       | Show detailed progress of each operation                                         |

**Examples:**
```powershell
# Default aggressive clean — best for most situations
magicx-ram-cleaner clean

# Gentle — safe for gaming, only frees what Windows would free first
magicx-ram-cleaner clean --level gentle

# Moderate — good balance
magicx-ram-cleaner clean -l moderate

# Nuclear — maximum RAM recovery, may cause brief slowdown
magicx-ram-cleaner clean -l nuclear -v

# Short form
magicx-ram-cleaner clean -l gentle
```

---

### `status` — Memory Diagnostics

Shows comprehensive memory usage information.

```
magicx-ram-cleaner status [OPTIONS]
```

**Options:**
| Flag             | Description                                                                |
| ---------------- | -------------------------------------------------------------------------- |
| `-d, --detailed` | Show memory page list breakdown (standby priorities, modified pages, etc.) |
| `-j, --json`     | Output as JSON for scripting/automation                                    |

**Examples:**
```powershell
# Basic memory overview
magicx-ram-cleaner status

# Detailed view including standby list per-priority breakdown
magicx-ram-cleaner status --detailed

# JSON output for scripts
magicx-ram-cleaner status --json

# Detailed JSON — great for monitoring scripts
magicx-ram-cleaner status -d -j
```

**Sample output (detailed):**
```
─── Physical Memory ────────────────────
  Memory Load:    72%
  Total:          16.00 GB
  Used:           11.52 GB
  Available:      4.48 GB

─── Memory Page Lists ──────────────────
  Zeroed:         128.00 MB  (32768 pages)
  Free:           0 B  (0 pages)
  Modified:       256.00 MB  (65536 pages)
  Mod-NoWrite:    0 B  (0 pages)

─── Standby List (by priority) ─────────
  Total Standby:  3.84 GB  (1006632 pages)
  Priority 0:       512.00 MB   13.0%  ████            Lowest
  Priority 1:       1.28 GB     33.3%  ██████████
  Priority 2:       384.00 MB   10.0%  ███
  Priority 3:       256.00 MB    6.7%  ██
  Priority 4:       512.00 MB   13.3%  ████
  Priority 5:       640.00 MB   16.7%  █████
  Priority 7:       256.00 MB    6.7%  ██              Highest
```

---

### `purge-standby` — Standby List Purge

Direct equivalent of EmptyStandbyList, but with options.

```
magicx-ram-cleaner purge-standby [OPTIONS]
```

**Options:**
| Flag             | Description                                  |
| ---------------- | -------------------------------------------- |
| `--low-priority` | Only purge priority-0 standby pages (safest) |
| `-v, --verbose`  | Show detailed progress                       |

**Examples:**
```powershell
# Purge ALL standby pages (like EmptyStandbyList)
magicx-ram-cleaner purge-standby

# Only purge low-priority pages (safer, less impact)
magicx-ram-cleaner purge-standby --low-priority
```

---

### `flush-modified` — Modified Page Flush

Forces dirty (modified) pages to be written to disk/pagefile.

```
magicx-ram-cleaner flush-modified [OPTIONS]
```

**Why you should use this:**
Modified pages are pages that have been changed in memory but not yet written to disk. They can't be freed until they're saved. Flushing them first means the subsequent standby purge will free more RAM.

**Pro tip:** Run this before `purge-standby` for maximum effect:
```powershell
magicx-ram-cleaner flush-modified
magicx-ram-cleaner purge-standby
```
(The `clean` command does this automatically.)

---

### `empty-workingsets` — Working Set Trim

Forces all processes to release their allocated memory pages.

```
magicx-ram-cleaner empty-workingsets [OPTIONS]
```

**Options:**
| Flag            | Description                                                                 |
| --------------- | --------------------------------------------------------------------------- |
| `--per-process` | Use per-process trimming instead of kernel-level (slower but more detailed) |
| `-v, --verbose` | Show detailed progress                                                      |

**Examples:**
```powershell
# Kernel-level trim — fastest, hits ALL processes
magicx-ram-cleaner empty-workingsets

# Per-process trim — shows how many processes were trimmed
magicx-ram-cleaner empty-workingsets --per-process -v
```

**Kernel-level vs Per-process:**
| Aspect      | Kernel-level (default)            | Per-process                |
| ----------- | --------------------------------- | -------------------------- |
| Speed       | Single kernel call                | Iterates all processes     |
| Coverage    | ALL processes including protected | Only processes we can open |
| Detail      | No per-process info               | Reports counts             |
| Reliability | Cannot fail per-process           | Some processes may fail    |

---

### `flush-cache` — File System Cache Flush

Releases the file system cache, freeing RAM used for cached file data.

```
magicx-ram-cleaner flush-cache [OPTIONS]
```

**This is unique to MagicX** — EmptyStandbyList cannot do this. The file system cache can consume many gigabytes of RAM on systems that do heavy I/O.

**Examples:**
```powershell
magicx-ram-cleaner flush-cache
magicx-ram-cleaner flush-cache -v
```

---

### `combine` — Memory Page Deduplication

Scans physical memory for identical pages and combines them using copy-on-write.

```
magicx-ram-cleaner combine [OPTIONS]
```

**Windows 10+ only.** This can take several seconds on systems with lots of RAM. It's particularly effective when you have many instances of the same application (e.g., multiple browser tabs, VMs).

---

### `monitor` — Continuous Monitoring

Watch memory usage in real-time with optional auto-cleaning.

```
magicx-ram-cleaner monitor [OPTIONS]
```

**Options:**
| Flag                        | Description                                         |
| --------------------------- | --------------------------------------------------- |
| `-i, --interval <SECONDS>`  | Check interval in seconds (default: 5)              |
| `-t, --threshold <PERCENT>` | Auto-clean when memory load exceeds this %          |
| `-l, --level <LEVEL>`       | Cleaning level for auto-clean (default: aggressive) |
| `-v, --verbose`             | Show details during auto-clean                      |

**Examples:**
```powershell
# Just monitor, no auto-clean
magicx-ram-cleaner monitor

# Monitor every 10 seconds
magicx-ram-cleaner monitor --interval 10

# Auto-clean at 80% usage with aggressive cleaning
magicx-ram-cleaner monitor --threshold 80

# Auto-clean at 90% with gentle cleaning, check every 3 seconds
magicx-ram-cleaner monitor -i 3 -t 90 -l gentle

# Detailed monitoring with nuclear auto-clean at 85%
magicx-ram-cleaner monitor -t 85 -l nuclear -v
```

---

## Cleaning Levels Explained

| Level          | Operations                                                           | Impact                                             | Best For                                       |
| -------------- | -------------------------------------------------------------------- | -------------------------------------------------- | ---------------------------------------------- |
| **gentle**     | Purge low-priority standby                                           | Minimal — only frees what Windows would free first | Gaming, production servers                     |
| **moderate**   | Empty working sets → Purge low-priority standby                      | Low — trims bloated processes                      | Daily maintenance                              |
| **aggressive** | File cache → Empty working sets → Flush modified → Purge ALL standby | Medium — brief I/O spike as apps re-fault pages    | Most users (default)                           |
| **nuclear**    | Everything + memory combining + second pass                          | Highest — may cause temporary slowdown             | Before running demanding apps, troubleshooting |

### What happens at each level?

#### Gentle
```
1. Purge low-priority standby pages (priority 0 only)
```
Only removes pages that Windows would evict first anyway. Your frequently-used cached data stays intact. Perfect for gaming where you want free RAM without losing Steam/game cache.

#### Moderate
```
1. Empty all process working sets (kernel-level)
2. Purge low-priority standby pages
```
Adds working set trimming — forces processes to give back memory they allocated but aren't actively using. The trimmed pages move to standby, and low-priority ones get freed.

#### Aggressive (Default)
```
1. Flush file system cache
2. Empty all process working sets (kernel-level)
3. Flush modified page list to disk
4. Purge ALL standby pages
```
The full sequence. Flushes the file cache first (can reclaim GBs on I/O-heavy systems), trims all processes, writes dirty pages to disk, then purges the entire standby list. This is what you want when you need RAM NOW.

#### Nuclear
```
1. Flush file system cache
2. Empty all process working sets (kernel-level)
3. Flush modified page list to disk
4. Purge low-priority standby pages
5. Purge ALL standby pages
6. Memory page combining (dedup)
7. Second-pass: Flush modified list again
8. Second-pass: Purge standby list again
```
Everything plus memory deduplication. The second pass catches pages that were modified during the combining step. Use this when you absolutely need maximum free RAM.

---

## Usage Examples

### For Normal Users

**"I just want more free RAM":**
```powershell
magicx-ram-cleaner clean
```

**"My PC feels slow":**
```powershell
# Check memory status first
magicx-ram-cleaner status

# If memory load is high (>80%), clean it
magicx-ram-cleaner clean
```

**"I want to clean RAM before launching a game":**
```powershell
magicx-ram-cleaner clean --level gentle
```

---

### For Gamers

**Before launching a game — free RAM without hurting game cache:**
```powershell
magicx-ram-cleaner clean -l gentle
```

**After closing a game — reclaim all memory:**
```powershell
magicx-ram-cleaner clean -l aggressive
```

**Auto-clean while gaming — keep RAM free automatically:**
```powershell
# Clean when RAM usage hits 85%, check every 10 seconds, gentle level
magicx-ram-cleaner monitor -i 10 -t 85 -l gentle
```

---

### For Power Users / Sysadmins

**Maximum single clean with verbose output:**
```powershell
magicx-ram-cleaner clean -l nuclear -v
```

**Manual step-by-step cleaning (full control):**
```powershell
# Step 1: See what we're working with
magicx-ram-cleaner status --detailed

# Step 2: Flush the file system cache
magicx-ram-cleaner flush-cache

# Step 3: Trim process working sets
magicx-ram-cleaner empty-workingsets

# Step 4: Flush modified pages to disk
magicx-ram-cleaner flush-modified

# Step 5: Purge the standby list
magicx-ram-cleaner purge-standby

# Step 6: Check results
magicx-ram-cleaner status --detailed
```

**Monitor a server and auto-clean at 90%:**
```powershell
magicx-ram-cleaner monitor -i 30 -t 90 -l moderate
```

**Get memory stats as JSON for your monitoring script:**
```powershell
$memory = magicx-ram-cleaner status --json --detailed | ConvertFrom-Json
if ($memory.snapshot.memory_load_percent -gt 85) {
    Write-Host "WARNING: High memory usage!"
}
```

---

### For Automation / Scripts

**Scheduled Task — clean every hour:**
```powershell
# Create a scheduled task (run in elevated PowerShell)
$action = New-ScheduledTaskAction -Execute "C:\Tools\magicx-ram-cleaner.exe" -Argument "clean -l moderate"
$trigger = New-ScheduledTaskTrigger -RepetitionInterval (New-TimeSpan -Hours 1) -At "00:00" -Once
$principal = New-ScheduledTaskPrincipal -UserId "SYSTEM" -LogonType ServiceAccount -RunLevel Highest
Register-ScheduledTask -TaskName "MagicX RAM Clean" -Action $action -Trigger $trigger -Principal $principal
```

**Batch file — clean and log:**
```batch
@echo off
echo [%date% %time%] Starting RAM clean >> C:\Logs\ramclean.log
C:\Tools\magicx-ram-cleaner.exe clean -l aggressive >> C:\Logs\ramclean.log 2>&1
echo [%date% %time%] Clean complete >> C:\Logs\ramclean.log
```

**PowerShell — conditional cleaning:**
```powershell
$status = magicx-ram-cleaner status --json | ConvertFrom-Json
if ($status.snapshot.memory_load_percent -gt 80) {
    magicx-ram-cleaner clean -l aggressive
    Write-Host "RAM cleaned! Load was $($status.snapshot.memory_load_percent)%"
}
```

---

## How Windows Memory Works

Understanding Windows memory management helps you use MagicX effectively.

### Memory Page Lists

Windows organizes physical memory pages into several lists:

```
┌──────────────────────────────────────────────────┐
│                Physical RAM                       │
├──────────┬───────────┬───────────┬───────────────┤
│ Active   │ Modified  │ Standby   │ Free/Zeroed   │
│ (In Use) │ (Dirty)   │ (Cached)  │ (Available)   │
├──────────┼───────────┼───────────┼───────────────┤
│ Pages    │ Pages     │ Pages     │ Pages ready   │
│ actively │ changed   │ cached    │ for immediate │
│ used by  │ but not   │ but not   │ allocation.   │
│ running  │ written   │ actively  │               │
│ programs │ to disk   │ used.     │               │
│          │ yet.      │ Can be    │               │
│          │           │ reclaimed.│               │
└──────────┴───────────┴───────────┴───────────────┘
```

- **Active (In Use)**: Pages actively being used by running programs. Can't be freed without terminating the program.
- **Modified (Dirty)**: Pages that have been changed in memory but not yet saved to disk. Must be written to disk before they can be freed.
- **Standby (Cached)**: Pages from recently-used files or programs. Still contain valid data that could be re-used (avoiding a disk read), but can be repurposed immediately if needed.
- **Free/Zeroed**: Empty pages ready for allocation.

### Standby Priority Levels (0-7)

Windows assigns priority levels to standby pages:
- **Priority 0 (Lowest)**: Least important — freed first when memory pressure occurs.
- **Priority 7 (Highest)**: Most important — freed last.

MagicX's `--low-priority` flag only frees priority 0 pages, preserving the more important cached data.

### What "Available" Memory Really Means

Windows Task Manager shows "Available" memory as: **Free + Zeroed + Standby**. This is because standby pages CAN be repurposed instantly. But they also contain cached data that speeds up your system.

MagicX moves pages from Standby (cached) to Free, increasing truly-free RAM at the cost of losing cached data (which will be re-read from disk when needed).

### The Optimal Cleaning Sequence

MagicX's aggressive/nuclear levels follow this sequence for a reason:

1. **Flush file cache** → Releases cached file data, immediately freeing RAM
2. **Empty working sets** → Forces processes to give back unused pages (these move to Modified/Standby)
3. **Flush modified** → Writes dirty pages to disk (Modified → Standby)
4. **Purge standby** → Frees all cached pages (Standby → Free)

If you skip step 3, modified pages can't be freed by step 4. This is why MagicX cleans more effectively than just running `EmptyStandbyList standbylist`.

---

## Comparison with Other Tools

| Feature               | MagicX  | EmptyStandbyList | RAMMap | Mem Reduct |
| --------------------- | :-----: | :--------------: | :----: | :--------: |
| Standby purge         |    ✅    |        ✅         |   ✅    |     ✅      |
| Low-priority only     |    ✅    |        ✅         |   ❌    |     ❌      |
| Working set trim      |    ✅    |        ✅         |   ✅    |     ✅      |
| Modified flush        |    ✅    |        ✅         |   ❌    |     ❌      |
| File cache flush      |    ✅    |        ❌         |   ❌    |     ❌      |
| Memory combining      |    ✅    |        ❌         |   ❌    |     ❌      |
| Smart levels          |    ✅    |        ❌         |   ❌    |     ❌      |
| Before/after stats    |    ✅    |        ❌         |   ❌    |  Partial   |
| Memory list details   |    ✅    |        ❌         |   ✅    |     ❌      |
| Auto-monitoring       |    ✅    |        ❌         |   ❌    |     ✅      |
| JSON output           |    ✅    |        ❌         |   ❌    |     ❌      |
| CLI (no GUI)          |    ✅    |        ✅         |   ❌    |     ❌      |
| Portable (no install) |    ✅    |        ✅         |   ✅    |     ❌      |
| Open source           |    ✅    |        ❌         |   ❌    |     ✅      |
| Binary size           | ~580 KB |      18 KB       | 1.2 MB |   800 KB   |

---

## FAQ

### Do I need to run as Administrator?

**Yes.** Memory management requires elevated privileges. Right-click your terminal (PowerShell/CMD) and select "Run as administrator", or right-click the .exe and choose "Run as administrator".

The binary includes a UAC manifest that will prompt for elevation automatically when double-clicked.

### Will this break anything?

**No.** MagicX only uses official Windows APIs to manage memory. It's doing what Windows would do naturally under memory pressure — just doing it on-demand instead of waiting.

After cleaning:
- Programs may feel slightly slower for a moment as they re-read cached data from disk
- No data is lost — modified pages are saved to disk before being freed
- All programs continue running normally

### How often should I clean?

- **Normal use**: When you notice high memory usage or slowdowns
- **Gaming**: Before launching games (gentle level)
- **Servers**: Use the monitor with auto-clean at 85-90%
- **General maintenance**: The aggressive level every few hours is fine

### What's the difference between this and just restarting?

A restart clears ALL memory but also closes all your programs. MagicX frees RAM while keeping everything running.

### Does this work on Windows ARM?

Currently built for x86-64 only. ARM support may be added in the future.

### Can I use this without the command line?

MagicX is CLI-only by design. For a GUI, consider creating a shortcut:
1. Right-click desktop → New → Shortcut
2. Location: `C:\Tools\magicx-ram-cleaner.exe clean`
3. Name: "Clean RAM"
4. Right-click shortcut → Properties → Advanced → "Run as administrator"

Double-click to clean RAM instantly.

---

## Building from Source

### Prerequisites

- [Rust](https://rustup.rs/) 1.85 or newer (edition 2024)
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
# Debug build (faster compile, slower runtime)
cargo build

# Run tests
cargo test

# Run with a specific command
cargo run -- clean -l gentle -v

# Check for warnings
cargo clippy
```

---

## Technical Architecture

```
src/
├── main.rs         # CLI entry point, argument parsing (clap)
├── cleaner.rs      # Core cleaning operations + smart clean engine
├── ntapi.rs        # NT Native API FFI (NtSetSystemInformation)
├── privilege.rs    # Windows privilege management
├── stats.rs        # Memory statistics (GlobalMemoryStatusEx, GetPerformanceInfo)
├── display.rs      # Terminal output formatting
└── monitor.rs      # Continuous monitoring loop
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

---

## Supported Systems

| OS                                  | Version          | Status                         |
| ----------------------------------- | ---------------- | ------------------------------ |
| Windows 10 IoT Enterprise LTSC 2021 | 21H2 (19044)     | ✅ Fully supported              |
| Windows 11                          | 24H2 and later   | ✅ Fully supported              |
| Windows 10                          | 21H2+            | ✅ Should work (tested on LTSC) |
| Windows Server                      | 2019, 2022, 2025 | ✅ Should work                  |

**Requirements:**
- x86-64 (64-bit) processor
- Administrator privileges
- ~1 MB disk space

---

## License

MIT License — see [LICENSE](LICENSE) for details.

---

*Built with Rust, powered by Windows NT kernel APIs, designed to be the best RAM cleaner ever made.*
