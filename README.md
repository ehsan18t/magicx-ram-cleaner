# MagicX RAM Cleaner

**The world's most powerful Windows RAM cleaner — CLI + GUI.**

MagicX RAM Cleaner goes far beyond tools like EmptyStandbyList by providing granular control over every Windows memory subsystem, smart multi-step cleaning, real-time monitoring with auto-clean, and detailed diagnostics — all in a single binary. Double-click for the GUI, or use the command line for scripting and power-user workflows.

---

## Table of Contents

- [MagicX RAM Cleaner](#magicx-ram-cleaner)
  - [Table of Contents](#table-of-contents)
  - [Quick Start](#quick-start)
  - [Installation](#installation)
    - [Option A: Download Pre-built Binary](#option-a-download-pre-built-binary)
    - [Option B: Build from Source](#option-b-build-from-source)
    - [Adding to PATH (optional)](#adding-to-path-optional)
  - [Why MagicX?](#why-magicx)
    - [vs EmptyStandbyList](#vs-emptystandbylist)
    - [Key Advantages](#key-advantages)
  - [Commands Reference](#commands-reference)
    - [`clean` — Smart Clean](#clean--smart-clean)
    - [`status` — Memory Diagnostics](#status--memory-diagnostics)
    - [`purge-standby` — Standby List Purge](#purge-standby--standby-list-purge)
    - [`flush-modified` — Modified Page Flush](#flush-modified--modified-page-flush)
    - [`empty-workingsets` — Working Set Trim](#empty-workingsets--working-set-trim)
    - [`flush-cache` — File System Cache Flush](#flush-cache--file-system-cache-flush)
    - [`flush-registry` — Registry Cache Flush](#flush-registry--registry-cache-flush)
    - [`combine` — Memory Page Deduplication](#combine--memory-page-deduplication)
    - [`monitor` — Continuous Monitoring](#monitor--continuous-monitoring)
    - [`context-menu` — Right-Click Context Menu](#context-menu--right-click-context-menu)
  - [Cleaning Levels Explained](#cleaning-levels-explained)
    - [What happens at each level?](#what-happens-at-each-level)
      - [Gentle](#gentle)
      - [Moderate](#moderate)
      - [Aggressive (Default)](#aggressive-default)
      - [Nuclear](#nuclear)
  - [Usage Examples](#usage-examples)
    - [For Normal Users](#for-normal-users)
    - [For Gamers](#for-gamers)
    - [For Power Users / Sysadmins](#for-power-users--sysadmins)
    - [For Automation / Scripts](#for-automation--scripts)
  - [How Windows Memory Works](#how-windows-memory-works)
    - [Memory Page Lists](#memory-page-lists)
    - [Standby Priority Levels (0-7)](#standby-priority-levels-0-7)
    - [What "Available" Memory Really Means](#what-available-memory-really-means)
    - [The Optimal Cleaning Sequence](#the-optimal-cleaning-sequence)
  - [Comparison with Other Tools](#comparison-with-other-tools)
  - [FAQ](#faq)
    - [Do I need to run as Administrator?](#do-i-need-to-run-as-administrator)
    - [Will this break anything?](#will-this-break-anything)
    - [How often should I clean?](#how-often-should-i-clean)
    - [What's the difference between this and just restarting?](#whats-the-difference-between-this-and-just-restarting)
    - [Does this work on Windows ARM?](#does-this-work-on-windows-arm)
    - [Can I use this without the command line?](#can-i-use-this-without-the-command-line)
  - [Building from Source](#building-from-source)
    - [Prerequisites](#prerequisites)
    - [Build Steps](#build-steps)
    - [Development](#development)
  - [Documentation](#documentation)
  - [Technical Architecture](#technical-architecture)
    - [APIs Used](#apis-used)
  - [Supported Systems](#supported-systems)
  - [License](#license)

---

## Quick Start

```powershell
# GUI Mode — just double-click the exe (must be run as Administrator)
# Or from a terminal:
magicx-ram-cleaner

# CLI Mode — open PowerShell or CMD as Administrator
# Run an aggressive clean (recommended for most users)
magicx-ram-cleaner clean

# Check how much RAM you freed
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
| Single-file, no dependencies    |        ✅         |     ✅ single portable exe      |

### Key Advantages

1. **GUI + CLI in one binary**: Double-click for a full graphical dashboard with real-time charts, or use the CLI for scripting and automation. No other RAM cleaner offers both.

2. **Smarter cleaning**: MagicX flushes modified pages *before* purging standby, which means pages that were dirty get saved and then freed. EmptyStandbyList misses these.

3. **File cache control**: The file system cache can consume gigabytes of RAM. MagicX can flush it directly — EmptyStandbyList cannot.

4. **Memory combining**: Windows 10+ can deduplicate identical memory pages using copy-on-write. MagicX triggers this; EmptyStandbyList doesn't support it.

5. **Kernel-level working set trim**: MagicX uses `NtSetSystemInformation(MemoryEmptyWorkingSets)` — a single kernel call that hits ALL processes including protected/system processes that per-process `EmptyWorkingSet()` cannot touch.

6. **Multi-pass cleaning**: The nuclear level does a second pass after memory combining to catch newly-modified pages.

---

## Global Options

These flags can be used with **any** subcommand:

| Flag            | Description                                                         |
| --------------- | ------------------------------------------------------------------- |
| `--no-color`    | Disable coloured terminal output (also respects `NO_COLOR` env var) |
| `-q`, `--quiet` | Suppress banner and non-essential output (implies no verbose)       |

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
| `--report <FILE>`     | Write cleaning results to a JSON report file                                     |
| `--dry-run`           | Preview what operations would run without executing them                         |
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

# Nuclear clean with JSON report for logging
magicx-ram-cleaner clean -l nuclear --report clean-report.json

# Preview what a nuclear clean would do (no execution)
magicx-ram-cleaner clean -l nuclear --dry-run

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
| `--top <N>`      | Show top N processes ranked by working set (physical RAM) usage            |

**Examples:**
```powershell
# Basic memory overview
magicx-ram-cleaner status

# Detailed view including standby list per-priority breakdown
magicx-ram-cleaner status --detailed

# Top 10 memory-hungry processes
magicx-ram-cleaner status --top 10

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
| Flag               | Description                                                                     |
| ------------------ | ------------------------------------------------------------------------------- |
| `--per-process`    | Use per-process trimming instead of kernel-level (slower but more detailed)     |
| `--exclude <NAME>` | Exclude processes by name (case-insensitive, repeatable). Implies --per-process |
| `-v, --verbose`    | Show detailed progress                                                          |

**Examples:**
```powershell
# Kernel-level trim — fastest, hits ALL processes
magicx-ram-cleaner empty-workingsets

# Per-process trim — shows how many processes were trimmed
magicx-ram-cleaner empty-workingsets --per-process -v

# Trim all processes except Chrome and Firefox
magicx-ram-cleaner empty-workingsets --exclude chrome --exclude firefox
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

### `flush-registry` — Registry Cache Flush

Flushes the Windows registry cache, writing dirty hive pages to disk and freeing the RAM they occupy.

```
magicx-ram-cleaner flush-registry [OPTIONS]
```

**This is unique to MagicX** — EmptyStandbyList cannot do this. Uses `NtSetSystemInformation(SystemRegistryReconciliationInformation)` to force all cached registry modifications to disk. Included automatically in aggressive and nuclear cleaning levels.

**Examples:**
```powershell
magicx-ram-cleaner flush-registry
magicx-ram-cleaner flush-registry -v
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
| Flag                        | Description                                                           |
| --------------------------- | --------------------------------------------------------------------- |
| `-i, --interval <SECONDS>`  | Check interval in seconds (default: 5)                                |
| `-t, --threshold <PERCENT>` | Auto-clean when memory load exceeds this %                            |
| `-l, --level <LEVEL>`       | Cleaning level for auto-clean (default: aggressive)                   |
| `-c, --cooldown <SECONDS>`  | Cooldown after auto-clean before cleaning again (default: 2×interval) |
| `-v, --verbose`             | Show details during auto-clean                                        |

**Examples:**
```powershell
# Just monitor, no auto-clean
magicx-ram-cleaner monitor

# Monitor every 10 seconds
magicx-ram-cleaner monitor --interval 10

# Auto-clean at 80% usage with aggressive cleaning
magicx-ram-cleaner monitor --threshold 80

# Auto-clean with 30-second cooldown between cleans
magicx-ram-cleaner monitor -t 85 --cooldown 30

# Auto-clean at 90% with gentle cleaning, check every 3 seconds
magicx-ram-cleaner monitor -i 3 -t 90 -l gentle

# Detailed monitoring with nuclear auto-clean at 85%
magicx-ram-cleaner monitor -t 85 -l nuclear -v
```

---

### `context-menu` — Right-Click Context Menu

Install or uninstall a cascading "MagicX RAM Cleaner" submenu in the Windows right-click context menu (Desktop background and folder windows).

```
magicx-ram-cleaner context-menu <install|uninstall>
```

**Subcommands:**
| Subcommand  | Description                                                 |
| ----------- | ----------------------------------------------------------- |
| `install`   | Add context menu entries (creates registry keys under HKCR) |
| `uninstall` | Remove context menu entries (deletes registry keys)         |

**Context menu entries installed:**
| Entry              | Action                              | Icon    |
| ------------------ | ----------------------------------- | ------- |
| Quick Clean        | `clean --level gentle --notify`     | app.ico |
| Standard Clean     | `clean --level moderate --notify`   | app.ico |
| Deep Clean         | `clean --level aggressive --notify` | app.ico |
| Purge Standby List | `purge-standby --notify`            | app.ico |
| Memory Status      | `status`                            | app.ico |

> **Note:** Nuclear is intentionally excluded from the context menu — it is too destructive for one-click access.

Cleaning entries use `--notify` mode: no terminal window appears (the binary uses `SUBSYSTEM:WINDOWS` and skips console attachment entirely), the operation runs silently, and a brief balloon notification shows the result (e.g. freed RAM, success/failure). The notification auto-dismisses after 2 seconds and is not saved in the Windows Action Center.

Memory Status opens a console window with full status output (no `--notify`) so the information is always visible regardless of whether the GUI is running.

**Examples:**
```powershell
# Install context menu entries (run as Administrator)
magicx-ram-cleaner context-menu install

# Uninstall context menu entries
magicx-ram-cleaner context-menu uninstall
```

After installing, right-click your Desktop or any folder background to see the "MagicX RAM Cleaner" submenu.

---

## Cleaning Levels Explained

| Level          | Operations                                                                            | Impact                                          | Best For                                       |
| -------------- | ------------------------------------------------------------------------------------- | ----------------------------------------------- | ---------------------------------------------- |
| **gentle**     | Purge ALL standby pages                                                               | Low — clears disk cache, no process impact      | Gaming, production servers                     |
| **moderate**   | Flush modified pages → Purge ALL standby                                              | Low-Medium — brief I/O spike from flushing      | Daily maintenance                              |
| **aggressive** | File cache → Registry flush → Empty working sets → Flush modified → Purge ALL standby | Medium — brief I/O spike as apps re-fault pages | Most users (default)                           |
| **nuclear**    | Everything + memory combining + second pass                                           | Highest — may cause temporary slowdown          | Before running demanding apps, troubleshooting |

### What happens at each level?

#### Gentle
```
1. Purge ALL standby pages (priorities 0–7)
```
Clears the entire standby list — cached copies of disk data that are already outside every process's working set. No running process is affected; the only cost is that files may need to be re-read from disk on next access. Safe to run at any time.

#### Moderate
```
1. Flush modified page list to disk
2. Purge ALL standby pages
```
First writes all dirty (modified) pages to disk, converting them to standby, then purges the entire standby list. Reclaims more memory than Gentle because it also drains the modified page list. No process working sets are touched — running apps are unaffected.

#### Aggressive (Default)
```
1. Flush file system cache
2. Flush registry cache
3. Empty all process working sets (kernel-level)
4. Flush modified page list to disk
5. Purge ALL standby pages
```
The full sequence. Flushes the file cache and registry cache first (can reclaim GBs on I/O-heavy systems), trims all processes, writes dirty pages to disk, then purges the entire standby list. This is what you want when you need RAM NOW.

#### Nuclear
```
1. Flush file system cache
2. Flush registry cache
3. Empty all process working sets (kernel-level)
4. Flush modified page list to disk
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
2. **Flush registry cache** → Writes dirty registry hive pages to disk
3. **Empty working sets** → Forces processes to give back unused pages (these move to Modified/Standby)
4. **Flush modified** → Writes dirty pages to disk (Modified → Standby)
5. **Purge standby** → Frees all cached pages (Standby → Free)

If you skip step 4, modified pages can't be freed by step 5. This is why MagicX cleans more effectively than just running `EmptyStandbyList standbylist`.

---

## Comparison with Other Tools

| Feature               | MagicX | EmptyStandbyList | RAMMap | Mem Reduct |
| --------------------- | :----: | :--------------: | :----: | :--------: |
| Standby purge         |   ✅    |        ✅         |   ✅    |     ✅      |
| Low-priority only     |   ✅    |        ✅         |   ❌    |     ❌      |
| Working set trim      |   ✅    |        ✅         |   ✅    |     ✅      |
| Modified flush        |   ✅    |        ✅         |   ❌    |     ❌      |
| File cache flush      |   ✅    |        ❌         |   ❌    |     ❌      |
| Registry cache flush  |   ✅    |        ❌         |   ❌    |     ❌      |
| Memory combining      |   ✅    |        ❌         |   ❌    |     ❌      |
| Smart levels          |   ✅    |        ❌         |   ❌    |     ❌      |
| Before/after stats    |   ✅    |        ❌         |   ❌    |  Partial   |
| Memory list details   |   ✅    |        ❌         |   ✅    |     ❌      |
| Auto-monitoring       |   ✅    |        ❌         |   ❌    |     ✅      |
| JSON output           |   ✅    |        ❌         |   ❌    |     ❌      |
| CLI support           |   ✅    |        ✅         |   ❌    |     ❌      |
| GUI with dashboard    |   ✅    |        ❌         |   ✅    |     ✅      |
| Portable (no install) |   ✅    |        ✅         |   ✅    |     ❌      |
| Open source           |   ✅    |        ❌         |   ❌    |     ✅      |

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

**Yes!** MagicX includes a full **built-in GUI**. Simply double-click the exe (or run it without arguments) to launch the graphical interface with:
- Real-time memory dashboard with usage bars and history chart
- One-click cleaning at all 4 levels (Gentle / Moderate / Aggressive / Nuclear)
- Continuous monitoring with automatic cleaning at configurable thresholds
- Process list sorted by memory usage
- Settings panel for dark mode, tray icon, and context menu integration

MagicX also includes Desktop context menu integration:
```powershell
magicx-ram-cleaner context-menu install
```
This adds a "MagicX RAM Cleaner" submenu to your Desktop and folder right-click menus with quick access to Quick Clean, Standard Clean, Deep Clean, Purge Standby List, and Memory Status — no terminal needed.

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

## Documentation

- [Contributing Guide](docs/CONTRIBUTING.md)
- [Rust Implementation Guide](docs/RUST_IMPLEMENTATION_GUIDE.md)
- [Windows Memory Internals](docs/WINDOWS_MEMORY_INTERNALS.md)
- [Security Policy](docs/SECURITY.md)

---

## Technical Architecture

```
src/
├── main.rs           # Thin entry point: mod declarations, main(), run(), dispatch
├── lib.rs            # Library crate root: module re-exports for benchmarks
├── cli.rs            # CLI definitions: clap Parser, Commands enum, help text
├── cleaner.rs        # Core cleaning operations + smart clean engine
├── console.rs        # Windows console management (dynamic attach/alloc, ANSI, notifications)
├── context_menu.rs   # Windows Desktop context menu integration (registry)
├── display.rs        # ALL terminal formatting: banner, status, clean output
├── gui/              # egui graphical interface module
│   ├── mod.rs        # Module entry point, run_gui() launcher
│   ├── app.rs        # Core app state, eframe::App impl, sidebar, layout routing
│   ├── persistence.rs # Settings file I/O, Win32 file dialogs, autostart registry
│   ├── theme.rs      # Colour palette, spacing, dark/light themes
│   ├── tray.rs       # System tray icon with context menu and Phosphor glyph icons
│   ├── widgets.rs    # Reusable UI components (cards, stat labels, toggle switch)
│   └── panels/       # One file per tab
│       ├── about.rs      # App info, developer profile, project details
│       ├── dashboard.rs  # Memory overview + one-click cleaning buttons
│       ├── monitor.rs    # Auto-clean configuration UI
│       ├── processes.rs  # Sortable grouped process memory table
│       └── settings.rs   # Appearance, integration, backup & restore
├── monitor.rs        # Continuous monitoring loop with auto-clean
├── ntapi.rs          # NT Native API FFI (NtSetSystemInformation)
├── privilege.rs      # Windows privilege management + admin elevation check
└── stats.rs          # Memory statistics (GlobalMemoryStatusEx, GetPerformanceInfo)
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
