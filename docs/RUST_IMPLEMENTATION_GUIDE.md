# Rust RAM Cleaner CLI — Complete Implementation Guide

## Table of Contents

1. [Crate Selection & Cargo.toml](#1-crate-selection--cargotoml)
2. [windows-sys vs windows Crate](#2-windows-sys-vs-windows-crate)
3. [Complete Cargo.toml](#3-complete-cargotoml)
4. [Privilege Elevation (SeProfileSingleProcessPrivilege)](#4-privilege-elevation)
5. [NtSetSystemInformation FFI Declaration](#5-ntset-system-information-ffi)
6. [GlobalMemoryStatusEx](#6-globalmemorystatusex)
7. [SetSystemFileCacheSize — File Cache Trimming](#7-setsystemfilecachesize)
8. [Registry Cache Flush — NtSetSystemInformation(SystemRegistryReconciliationInformation)](#7b-registry-cache-flush--ntsetsysteminformationsystemregistryreconciliationinformation)
9. [Process Enumeration & Working Set Trimming](#8-process-enumeration--working-set-trimming)
9. [Memory Combining (Windows 10+)](#9-memory-combining)
10. [Complete Cleaning Sequence](#10-complete-cleaning-sequence)
11. [Full Compilable Source (Simplified Prototype)](#11-full-compilable-source-simplified-prototype)
12. [Current Architecture & Features](#12-current-architecture--features)

---

## 1. Crate Selection & Cargo.toml

### Crate Recommendations

| Crate                              | Purpose               | Why                                                                                                                    |
| ---------------------------------- | --------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| **`windows-sys`**                  | Win32/NT API bindings | Zero-cost, no wrappers — just `extern "system"` declarations + types. Fastest compile times. Official Microsoft crate. |
| **`clap`** (with `derive` feature) | CLI argument parsing  | Industry standard. Derive macros give type-safe subcommands with zero boilerplate.                                     |
| **`colored`**                      | Terminal colors       | Lightweight, simple API for colored CLI output.                                                                        |
| **`anyhow`**                       | Error handling        | Ergonomic `Result<T>` with context chains.                                                                             |
| **`serde`** + **`serde_json`**     | JSON serialization    | Used for `--report` JSON output and `--json` status output.                                                            |

### Crates you do NOT need

| Crate                      | Why Skip                                                                                                                           |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| **`ntapi`**                | Stale (last updated 2021). Provides NT API bindings, but `windows-sys` + manual FFI for the 2-3 undocumented functions is cleaner. |
| **`winapi`**               | Legacy. `windows-sys` is Microsoft's official replacement.                                                                         |
| **`windows`** (high-level) | Adds smart pointers, RAII wrappers, COM support — overhead you don't need for a CLI tool calling C-style APIs.                     |
| **`sysinfo`**              | Cross-platform abstractions — unnecessary overhead when you already call `GlobalMemoryStatusEx` directly via `windows-sys`.        |

---

## 2. windows-sys vs windows Crate

### `windows-sys` (RECOMMENDED for this project)

```rust
// Raw C-style bindings. No wrappers, no overhead.
use windows_sys::Win32::System::Threading::OpenProcessToken;
// Returns BOOL (i32). You check == 0 for failure.
```

**Pros:**
- Zero abstraction cost — same as hand-written `extern "system"` blocks
- Fastest compile times (~3-5s incremental)
- All types are plain `repr(C)` structs and integer constants
- Official Microsoft crate, updated with every Windows SDK release
- Perfect for system-level tools that need raw control

**Cons:**
- Everything is `unsafe`
- No RAII — you must `CloseHandle` manually
- Error handling is manual (`GetLastError()`)

### `windows` (high-level)

```rust
// Smart wrappers with RAII and Result types.
use windows::Win32::System::Threading::OpenProcessToken;
// Returns windows::core::Result<()>. Handle auto-closes on drop.
```

**Pros:**
- RAII handles (auto-close)
- `Result` return types
- Better for COM-heavy or UI code

**Cons:**
- 2-3x longer compile times
- Hidden allocations and overhead
- More complex error types
- Overkill for a CLI tool making direct kernel calls

### Verdict

**Use `windows-sys`** for this project. A RAM cleaner calls ~10 Win32 functions total. The raw bindings are more predictable and you need `unsafe` anyway for `NtSetSystemInformation` (which isn't in any crate — you must declare it yourself).

---

## 3. Complete Cargo.toml

```toml
[package]
name = "magicx-ram-cleaner"
version = "1.0.0"
edition = "2024"
description = "World-class Windows RAM cleaner CLI"
license = "MIT"
repository = "https://github.com/ehsan18t/magicx-ram-cleaner"
rust-version = "1.93"

[profile.release]
opt-level = 3        # Optimize for speed
lto = true           # Link-time optimization
codegen-units = 1    # Single codegen unit for best optimization
strip = true         # Strip debug symbols
panic = "abort"      # No unwinding

[dependencies]
clap = { version = "4", features = ["derive", "color"] }
colored = "3"
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dependencies.windows-sys]
version = "0.61"
features = [
    # Foundation types (HANDLE, BOOL, LUID, CloseHandle, GetLastError)
    "Win32_Foundation",

    # Memory APIs (SetSystemFileCacheSize)
    "Win32_System_Memory",

    # Process/thread APIs (OpenProcess, GetCurrentProcess, OpenProcessToken)
    "Win32_System_Threading",

    # Security APIs (AdjustTokenPrivileges, LookupPrivilegeValueW)
    "Win32_Security",

    # Authorization APIs (SE_PRIVILEGE_ENABLED, TOKEN_PRIVILEGES)
    "Win32_Security_Authorization",

    # Process status APIs (K32EnumProcesses, K32EmptyWorkingSet, K32GetPerformanceInfo)
    "Win32_System_ProcessStatus",

    # System information (GlobalMemoryStatusEx)
    "Win32_System_SystemInformation",

    # Console APIs (GetConsoleProcessList)
    "Win32_System_Console",

    # Shell notification APIs (Shell_NotifyIconW, NOTIFYICONDATAW)
    "Win32_UI_Shell",

    # Icon loading and window messaging (LoadIconW, DestroyIcon)
    "Win32_UI_WindowsAndMessaging",

    # Module handle for icon loading (GetModuleHandleW)
    "Win32_System_LibraryLoader",

    # Toolhelp32 APIs (CreateToolhelp32Snapshot, Process32FirstW)
    "Win32_System_Diagnostics_ToolHelp",
]

[build-dependencies]
embed-manifest = "1"
winresource = "0.1"
```

### Feature Map — What Each Feature Unlocks

| Feature                             | Functions / Types It Provides                                                                                  |
| ----------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `Win32_Foundation`                  | `HANDLE`, `BOOL`, `LUID`, `LUID_AND_ATTRIBUTES`, `CloseHandle`, `GetLastError`, `TRUE`/`FALSE`, `NTSTATUS`     |
| `Win32_System_Memory`               | `SetSystemFileCacheSize`, `GetSystemFileCacheSize`                                                             |
| `Win32_System_Threading`            | `GetCurrentProcess`, `OpenProcess`, `PROCESS_SET_QUOTA`, `PROCESS_QUERY_INFORMATION`                           |
| `Win32_Security`                    | `TOKEN_ADJUST_PRIVILEGES`, `TOKEN_QUERY`, `AdjustTokenPrivileges`, `LookupPrivilegeValueW`, `OpenProcessToken` |
| `Win32_Security_Authorization`      | `TOKEN_PRIVILEGES`, `SE_PRIVILEGE_ENABLED`                                                                     |
| `Win32_System_ProcessStatus`        | `K32EnumProcesses`, `K32EmptyWorkingSet`, `K32GetPerformanceInfo`, `PERFORMANCE_INFORMATION`                   |
| `Win32_System_SystemInformation`    | `GlobalMemoryStatusEx`, `MEMORYSTATUSEX`, `GetSystemInfo`                                                      |
| `Win32_System_Console`              | `AttachConsole`/`AllocConsole` (dynamic console for `SUBSYSTEM:WINDOWS`), `SetStdHandle`, `GetConsoleProcessList` |
| `Win32_Storage_FileSystem`          | `CreateFileW` (open `CONOUT$`/`CONIN$` for std handle redirection after `AttachConsole`)                        |
| `Win32_System_Diagnostics_ToolHelp` | `CreateToolhelp32Snapshot`, `Process32FirstW`, `Process32NextW`, `PROCESSENTRY32W` (per-process enumeration)   |
| `Win32_UI_Shell`                    | `Shell_NotifyIconW`, `NOTIFYICONDATAW`, `NIM_ADD`, `NIM_DELETE`, `NIF_*` (balloon notifications)              |
| `Win32_UI_WindowsAndMessaging`      | `LoadIconW`, `DestroyIcon` (icon loading for notifications)                                                    |
| `Win32_System_LibraryLoader`        | `GetModuleHandleW` (module handle for icon resource loading)                                                   |

---

## 4. Privilege Elevation

All memory list commands require `SeProfileSingleProcessPrivilege`. File cache trimming requires `SeIncreaseQuotaPrivilege`. Both are held by Administrators but disabled by default — you must enable them in the token.

### Complete Compilable Code

```rust
use std::mem;
use std::ptr;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, LUID};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, OpenProcessToken,
    SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// Enables a named privilege in the current process token.
/// Returns Ok(()) on success, Err with Win32 error code on failure.
fn enable_privilege(privilege_name: &str) -> Result<(), u32> {
    unsafe {
        // Step 1: Open our own process token
        let mut token: HANDLE = 0;
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        ) == 0
        {
            return Err(GetLastError());
        }

        // Step 2: Look up the LUID for the privilege name
        let wide_name: Vec<u16> = privilege_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let mut luid: LUID = mem::zeroed();
        if LookupPrivilegeValueW(ptr::null(), wide_name.as_ptr(), &mut luid) == 0 {
            CloseHandle(token);
            return Err(GetLastError());
        }

        // Step 3: Build TOKEN_PRIVILEGES struct and adjust
        let mut tp: TOKEN_PRIVILEGES = mem::zeroed();
        tp.PrivilegeCount = 1;
        tp.Privileges[0].Luid = luid;
        tp.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;

        let success = AdjustTokenPrivileges(
            token,
            0, // FALSE — don't disable all
            &tp,
            mem::size_of::<TOKEN_PRIVILEGES>() as u32,
            ptr::null_mut(),
            ptr::null_mut(),
        );

        let error = GetLastError();
        CloseHandle(token);

        // AdjustTokenPrivileges returns success even if not all privileges
        // were adjusted. Must check GetLastError() == ERROR_SUCCESS (0).
        if success == 0 || error != 0 {
            return Err(error);
        }

        Ok(())
    }
}

// Convenience wrappers
fn enable_profile_privilege() -> Result<(), u32> {
    enable_privilege("SeProfileSingleProcessPrivilege")
}

fn enable_quota_privilege() -> Result<(), u32> {
    enable_privilege("SeIncreaseQuotaPrivilege")
}
```

### Important Notes

- The privilege name strings are **case-sensitive** — use exact casing as shown.
- `AdjustTokenPrivileges` has a quirk: it returns `TRUE` even when not all privileges were enabled. You **must** check `GetLastError() == ERROR_NOT_ALL_ASSIGNED (0x514)` to detect partial failure.
- The process must be run as **Administrator**. The privilege exists in admin tokens but is disabled by default.
- `TOKEN_PRIVILEGES` in `windows-sys` has `Privileges` as a `[LUID_AND_ATTRIBUTES; 1]` — a single-element array, which is exactly what we need.

---

## 5. NtSetSystemInformation FFI Declaration

`NtSetSystemInformation` is an **undocumented** NT API exported by `ntdll.dll`. It is NOT in `windows-sys`. You must declare it manually via `#[link]`.

### FFI Declaration

```rust
use std::ffi::c_void;

// ── NTSTATUS codes ──
const STATUS_SUCCESS: i32 = 0;
const STATUS_ACCESS_DENIED: i32 = 0xC0000022_u32 as i32;
const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC0000004_u32 as i32;
const STATUS_INVALID_PARAMETER: i32 = 0xC000000D_u32 as i32;

// ── SystemInformationClass values ──
const SYSTEM_MEMORY_LIST_INFORMATION: u32 = 0x50; // 80 decimal
const SYSTEM_COMBINE_PHYSICAL_MEMORY_INFORMATION: u32 = 0x82; // 130 decimal

// ── SYSTEM_MEMORY_LIST_COMMAND enum ──
#[repr(i32)]
#[derive(Debug, Clone, Copy)]
enum MemoryListCommand {
    CaptureAccessedBits = 0,
    CaptureAndResetAccessedBits = 1,
    EmptyWorkingSets = 2,
    FlushModifiedList = 3,
    PurgeStandbyList = 4,
    PurgeLowPriorityStandbyList = 5,
    CommandMax = 6,
}

// ── Struct for querying memory list info ──
#[repr(C)]
#[derive(Debug, Default, Clone)]
struct SystemMemoryListInformation {
    zero_page_count: u64,
    free_page_count: u64,
    modified_page_count: u64,
    modified_no_write_page_count: u64,
    bad_page_count: u64,
    page_count_by_priority: [u64; 8],
    repurposed_pages_by_priority: [u64; 8],
    modified_page_count_page_file: u64,
}

// ── Struct for memory combining ──
#[repr(C)]
#[derive(Debug)]
struct MemoryCombineInformationEx {
    handle: isize,       // HANDLE
    pages_combined: usize, // ULONG_PTR
}

// ── Link to ntdll.dll and declare the functions ──
#[link(name = "ntdll")]
extern "system" {
    fn NtSetSystemInformation(
        SystemInformationClass: u32,
        SystemInformation: *mut c_void,
        SystemInformationLength: u32,
    ) -> i32; // NTSTATUS

    fn NtQuerySystemInformation(
        SystemInformationClass: u32,
        SystemInformation: *mut c_void,
        SystemInformationLength: u32,
        ReturnLength: *mut u32,
    ) -> i32; // NTSTATUS
}
```

### Key Design Points

1. **`#[link(name = "ntdll")]`** — The Rust linker resolves `NtSetSystemInformation` and `NtQuerySystemInformation` from `ntdll.lib` at link time. No need for `LoadLibrary`/`GetProcAddress` — every Windows process has `ntdll.dll` loaded from the start.

2. **Calling convention is `"system"`** — On x86_64 Windows this is `__stdcall` (which is the same as `__cdecl` on x64, but `extern "system"` is the correct portable declaration).

3. **The command is passed as a pointer to an `i32`** — `NtSetSystemInformation` takes `PVOID SystemInformation` pointing to the command value, and `SystemInformationLength` = `sizeof(i32)` = 4.

### Issuing Memory Commands

```rust
use std::mem;

/// Issue a memory list command via NtSetSystemInformation.
/// Requires SeProfileSingleProcessPrivilege to be enabled.
fn issue_memory_command(command: MemoryListCommand) -> Result<(), i32> {
    unsafe {
        let mut cmd = command as i32;
        let status = NtSetSystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &mut cmd as *mut i32 as *mut c_void,
            mem::size_of::<i32>() as u32,
        );
        if status != STATUS_SUCCESS {
            Err(status)
        } else {
            Ok(())
        }
    }
}

// Convenience functions
fn purge_standby_list() -> Result<(), i32> {
    issue_memory_command(MemoryListCommand::PurgeStandbyList)
}

fn purge_low_priority_standby() -> Result<(), i32> {
    issue_memory_command(MemoryListCommand::PurgeLowPriorityStandbyList)
}

fn empty_all_working_sets() -> Result<(), i32> {
    issue_memory_command(MemoryListCommand::EmptyWorkingSets)
}

fn flush_modified_list() -> Result<(), i32> {
    issue_memory_command(MemoryListCommand::FlushModifiedList)
}
```

### Querying Memory List Information

```rust
/// Query detailed memory page list information from the kernel.
fn query_memory_lists() -> Result<SystemMemoryListInformation, i32> {
    unsafe {
        let mut info = SystemMemoryListInformation::default();
        let mut return_length: u32 = 0;
        let status = NtQuerySystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &mut info as *mut _ as *mut c_void,
            mem::size_of::<SystemMemoryListInformation>() as u32,
            &mut return_length,
        );
        if status != STATUS_SUCCESS {
            Err(status)
        } else {
            Ok(info)
        }
    }
}

// Usage:
fn print_memory_lists() {
    if let Ok(info) = query_memory_lists() {
        let page_size = 4096u64;
        let total_standby: u64 = info.page_count_by_priority.iter().sum();

        println!("Free pages:     {} ({} MB)", info.free_page_count,
                 info.free_page_count * page_size / 1024 / 1024);
        println!("Zero pages:     {} ({} MB)", info.zero_page_count,
                 info.zero_page_count * page_size / 1024 / 1024);
        println!("Modified pages: {} ({} MB)", info.modified_page_count,
                 info.modified_page_count * page_size / 1024 / 1024);
        println!("Standby total:  {} ({} MB)", total_standby,
                 total_standby * page_size / 1024 / 1024);

        for (i, &count) in info.page_count_by_priority.iter().enumerate() {
            if count > 0 {
                println!("  Standby P{}: {} ({} MB)", i, count,
                         count * page_size / 1024 / 1024);
            }
        }
    }
}
```

---

## 6. GlobalMemoryStatusEx

This is fully covered by `windows-sys`. No manual FFI needed.

```rust
use std::mem;
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

fn get_memory_status() -> Result<MEMORYSTATUSEX, u32> {
    unsafe {
        let mut status: MEMORYSTATUSEX = mem::zeroed();
        status.dwLength = mem::size_of::<MEMORYSTATUSEX>() as u32;

        if GlobalMemoryStatusEx(&mut status) == 0 {
            return Err(windows_sys::Win32::Foundation::GetLastError());
        }
        Ok(status)
    }
}

fn print_memory_status() {
    let s = get_memory_status().expect("GlobalMemoryStatusEx failed");
    let mb = |bytes: u64| bytes / 1024 / 1024;

    println!("Memory Load:    {}%", s.dwMemoryLoad);
    println!("Total Physical: {} MB", mb(s.ullTotalPhys));
    println!("Avail Physical: {} MB", mb(s.ullAvailPhys));
    println!("Total PageFile: {} MB", mb(s.ullTotalPageFile));
    println!("Avail PageFile: {} MB", mb(s.ullAvailPageFile));
    println!("Total Virtual:  {} MB", mb(s.ullTotalVirtual));
    println!("Avail Virtual:  {} MB", mb(s.ullAvailVirtual));
}
```

### MEMORYSTATUSEX Layout in windows-sys

The `MEMORYSTATUSEX` struct from `windows-sys` (in `Win32::System::SystemInformation`) is:

```rust
#[repr(C)]
pub struct MEMORYSTATUSEX {
    pub dwLength: u32,              // Must set to size_of::<Self>() before calling
    pub dwMemoryLoad: u32,          // 0-100% physical memory in use
    pub ullTotalPhys: u64,          // Total physical RAM (bytes)
    pub ullAvailPhys: u64,          // Available = free + zero + standby (bytes)
    pub ullTotalPageFile: u64,      // Commit limit (bytes)
    pub ullAvailPageFile: u64,      // Available commit (bytes)
    pub ullTotalVirtual: u64,       // Total user-mode VA (bytes)
    pub ullAvailVirtual: u64,       // Available user-mode VA (bytes)
    pub ullAvailExtendedVirtual: u64, // Always 0
}
```

**Key insight:** `ullAvailPhys` = standby + free + zero page lists. This matches "Available" in Task Manager. After purging the standby list, `ullAvailPhys` increases by the amount of standby pages freed.

---

## 7. SetSystemFileCacheSize — File Cache Trimming

### What It Does

The Windows file system cache (aka "System Cache Working Set") holds recently-read file data in physical memory. `SetSystemFileCacheSize` can flush this cache, forcing pages to the standby list (from where they can be purged).

### Code

```rust
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::System::Memory::SetSystemFileCacheSize;

/// Flush the file system cache by setting min/max to -1.
/// Requires SeIncreaseQuotaPrivilege to be enabled.
fn flush_file_cache() -> Result<(), u32> {
    // Enable SeIncreaseQuotaPrivilege first
    enable_privilege("SeIncreaseQuotaPrivilege").map_err(|e| e)?;

    unsafe {
        // Setting both to usize::MAX (which is (SIZE_T)-1) tells Windows
        // to flush the system file cache working set.
        let result = SetSystemFileCacheSize(
            usize::MAX, // MinimumFileCacheSize = (SIZE_T)-1
            usize::MAX, // MaximumFileCacheSize = (SIZE_T)-1
            0,          // Flags = 0 (no hard limits)
        );

        if result == 0 {
            Err(GetLastError())
        } else {
            Ok(())
        }
    }
}
```

### Function Signature (from windows-sys)

```rust
extern "system" {
    // In windows_sys::Win32::System::Memory
    pub fn SetSystemFileCacheSize(
        MinimumFileCacheSize: usize,  // SIZE_T
        MaximumFileCacheSize: usize,  // SIZE_T
        Flags: u32,                   // DWORD
    ) -> i32;                         // BOOL
}
```

### Parameters

| Parameter              | Value for Flushing            | Meaning                       |
| ---------------------- | ----------------------------- | ----------------------------- |
| `MinimumFileCacheSize` | `usize::MAX` (= `(SIZE_T)-1`) | Special sentinel: flush cache |
| `MaximumFileCacheSize` | `usize::MAX` (= `(SIZE_T)-1`) | Special sentinel: flush cache |
| `Flags`                | `0`                           | No hard limit enforcement     |

### Available Flags

```rust
const FILE_CACHE_MAX_HARD_ENABLE: u32  = 0x00000001; // Enforce a hard max cache size
const FILE_CACHE_MAX_HARD_DISABLE: u32 = 0x00000002; // Remove hard max limit
const FILE_CACHE_MIN_HARD_ENABLE: u32  = 0x00000004; // Enforce a hard min cache size
const FILE_CACHE_MIN_HARD_DISABLE: u32 = 0x00000008; // Remove hard min limit
```

---

## 7b. Registry Cache Flush — NtSetSystemInformation(SystemRegistryReconciliationInformation)

### What It Does

Windows caches registry hive modifications in memory before writing them to disk. `NtSetSystemInformation` with information class `SystemRegistryReconciliationInformation` (155 / 0x9B) forces all dirty registry hive pages to be written to disk, freeing the modified memory they occupy.

### Code

```rust
/// SystemRegistryReconciliationInformation = 155 (0x9B)
const SYSTEM_REGISTRY_RECONCILIATION_INFORMATION: u32 = 155;

/// Flush registry cache to disk. No input buffer required.
fn flush_registry_cache() -> Result<(), i32> {
    let status = unsafe {
        NtSetSystemInformation(
            SYSTEM_REGISTRY_RECONCILIATION_INFORMATION,
            std::ptr::null_mut(),
            0,
        )
    };
    if status == 0 { Ok(()) } else { Err(status) }
}
```

### Details

- **Information class**: 155 (`SystemRegistryReconciliationInformation`)
- **Input buffer**: NULL (no input data required)
- **Buffer length**: 0
- **Required privilege**: Administrator (no specific privilege token beyond elevation)
- **Source**: phnt headers from SystemInformer project; also used by Mem Reduct

This operation is included in MagicX's aggressive and nuclear cleaning levels, executed after the file cache flush and before working set trimming.

---

## 8. Process Enumeration & Working Set Trimming

### 8.1 EnumProcesses — Get All Process IDs

```rust
use windows_sys::Win32::System::ProcessStatus::K32EnumProcesses;

/// Returns a Vec of all process IDs currently running on the system.
fn enum_processes() -> Result<Vec<u32>, u32> {
    unsafe {
        // Start with space for 4096 PIDs (16 KB). Sufficient for most systems.
        let mut pids: Vec<u32> = vec![0u32; 4096];
        let mut bytes_returned: u32 = 0;

        let result = K32EnumProcesses(
            pids.as_mut_ptr(),
            (pids.len() * std::mem::size_of::<u32>()) as u32,
            &mut bytes_returned,
        );

        if result == 0 {
            return Err(windows_sys::Win32::Foundation::GetLastError());
        }

        let count = bytes_returned as usize / std::mem::size_of::<u32>();
        pids.truncate(count);
        Ok(pids)
    }
}
```

### 8.2 EmptyWorkingSet — Trim a Single Process

```rust
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
use windows_sys::Win32::System::ProcessStatus::K32EmptyWorkingSet;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_SET_QUOTA,
};

/// Empties the working set of a single process by PID.
/// Returns Ok(true) if trimmed, Ok(false) if process couldn't be opened.
fn empty_process_working_set(pid: u32) -> Result<bool, u32> {
    unsafe {
        // Need PROCESS_SET_QUOTA for EmptyWorkingSet
        // Need PROCESS_QUERY_INFORMATION to read process info
        let handle: HANDLE = OpenProcess(
            PROCESS_SET_QUOTA | PROCESS_QUERY_INFORMATION,
            0, // bInheritHandle = FALSE
            pid,
        );

        if handle == 0 {
            // Can't open — protected process, access denied, etc.
            return Ok(false);
        }

        let result = K32EmptyWorkingSet(handle);
        CloseHandle(handle);

        if result == 0 {
            // EmptyWorkingSet failed but we did open the process
            Ok(false)
        } else {
            Ok(true)
        }
    }
}

/// Empties working sets of ALL processes. Returns (trimmed_count, failed_count).
fn empty_all_process_working_sets() -> Result<(u32, u32), u32> {
    let pids = enum_processes()?;
    let mut trimmed = 0u32;
    let mut failed = 0u32;

    for &pid in &pids {
        if pid == 0 {
            continue; // System Idle Process
        }
        match empty_process_working_set(pid) {
            Ok(true) => trimmed += 1,
            Ok(false) => failed += 1,
            Err(_) => failed += 1,
        }
    }

    Ok((trimmed, failed))
}
```

### 8.3 SetProcessWorkingSetSizeEx — More Control

```rust
use windows_sys::Win32::System::Threading::SetProcessWorkingSetSizeEx;

/// Empty a process's working set using SetProcessWorkingSetSizeEx.
/// This is what EmptyWorkingSet calls internally.
fn trim_process_working_set(handle: HANDLE) -> Result<(), u32> {
    unsafe {
        // Passing -1 for both min and max empties the working set.
        let result = SetProcessWorkingSetSizeEx(
            handle,
            usize::MAX, // dwMinimumWorkingSetSize = (SIZE_T)-1
            usize::MAX, // dwMaximumWorkingSetSize = (SIZE_T)-1
            0,          // Flags = 0
        );

        if result == 0 {
            Err(GetLastError())
        } else {
            Ok(())
        }
    }
}
```

### Note: NtSetSystemInformation MemoryEmptyWorkingSets vs Per-Process

You have **two approaches** to emptying working sets:

| Approach                     | API                                         | Pros                                                                   | Cons                                                   |
| ---------------------------- | ------------------------------------------- | ---------------------------------------------------------------------- | ------------------------------------------------------ |
| **Kernel-level (one call)**  | `NtSetSystemInformation(0x50, cmd=2)`       | Single syscall, empties ALL processes including protected ones         | No per-process control, no error reporting per process |
| **User-level (per-process)** | `EnumProcesses` + `EmptyWorkingSet` per PID | Can skip critical processes, report per-process results, show progress | Many syscalls, can't touch protected processes         |

**Recommendation:** Use `NtSetSystemInformation(cmd=2)` for the bulk operation, then optionally use per-process trimming for targeted cleaning.

---

## 9. Memory Combining (Windows 10+)

Memory combining (page deduplication) finds identical physical pages and merges them into a single copy, freeing the duplicates. This is triggered by `NtSetSystemInformation` with `SystemCombinePhysicalMemoryInformation` (0x82 = 130).

```rust
/// Trigger memory page combining/deduplication.
/// Returns the number of pages that were combined.
fn combine_memory_pages() -> Result<usize, i32> {
    // Enable SeProfileSingleProcessPrivilege
    enable_profile_privilege().map_err(|_| STATUS_ACCESS_DENIED)?;

    unsafe {
        let mut info = MemoryCombineInformationEx {
            handle: 0,
            pages_combined: 0,
        };

        let status = NtSetSystemInformation(
            SYSTEM_COMBINE_PHYSICAL_MEMORY_INFORMATION, // 0x82 = 130
            &mut info as *mut _ as *mut c_void,
            std::mem::size_of::<MemoryCombineInformationEx>() as u32,
        );

        if status != STATUS_SUCCESS {
            Err(status)
        } else {
            Ok(info.pages_combined)
        }
    }
}
```

### Notes on Memory Combining

- The `MEMORY_COMBINE_INFORMATION_EX` struct is straightforward:
  - `handle`: Set to `0`/`NULL` on input
  - `pages_combined`: Output — number of pages that were deduplicated
- This operation can take **several seconds** on systems with lots of RAM
- It scans physical memory for identical 4K pages, then creates copy-on-write mappings
- Low risk — pages are marked copy-on-write, so any write creates a new copy
- Returns `STATUS_SUCCESS` even if 0 pages were combined

### Extended Version (Windows 10 1607+)

There's also `MEMORY_COMBINE_INFORMATION_EX2` with flags:

```rust
#[repr(C)]
struct MemoryCombineInformationEx2 {
    handle: isize,
    pages_combined: usize,
    flags: u32,  // New in Windows 10 1607+
}
```

For most purposes, `MemoryCombineInformationEx` (without flags) is sufficient and more compatible.

---

## 10. Complete Cleaning Sequence

### Recommended Order (Safest to Most Aggressive)

```rust
use std::time::Instant;

struct CleanResult {
    cache_flushed: bool,
    processes_trimmed: u32,
    modified_flushed: bool,
    low_priority_purged: bool,
    standby_purged: bool,
    pages_combined: usize,
    freed_mb: u64,
    duration_ms: u128,
}

fn full_clean(aggressive: bool) -> Result<CleanResult, anyhow::Error> {
    let start = Instant::now();

    // Snapshot memory before
    let before = get_memory_status()?;

    // ── Step 1: Flush file system cache ──
    // Moves file cache pages → standby list
    // Requires: SeIncreaseQuotaPrivilege
    let cache_flushed = flush_file_cache().is_ok();

    // ── Step 2: Empty process working sets ──
    // Forces all process pages → standby (clean) or modified (dirty)
    // Requires: SeProfileSingleProcessPrivilege
    let (processes_trimmed, _) = empty_all_process_working_sets().unwrap_or((0, 0));

    // ── Step 3: Flush modified page list ──
    // Writes dirty pages to disk → moves them to standby
    // This must happen BEFORE purging standby, so we don't lose data
    // Requires: SeProfileSingleProcessPrivilege
    let modified_flushed = flush_modified_list().is_ok();

    // ── Step 4a: Purge low-priority standby list ──
    // Frees low-value cache pages (SuperFetch prefetched, etc.)
    // Safe — these pages have minimal impact if lost
    let low_priority_purged = purge_low_priority_standby().is_ok();

    // ── Step 4b: Purge ALL standby list (aggressive mode only) ──
    // Nuclear option — destroys ALL cached file data
    let standby_purged = if aggressive {
        purge_standby_list().is_ok()
    } else {
        false
    };

    // ── Step 5: Memory combining/dedup ──
    // Scans for duplicate physical pages and merges them
    // Low impact, can be slow on large RAM systems
    let pages_combined = combine_memory_pages().unwrap_or(0);

    // Snapshot memory after
    let after = get_memory_status()?;
    let freed_mb = (after.ullAvailPhys.saturating_sub(before.ullAvailPhys)) / 1024 / 1024;

    Ok(CleanResult {
        cache_flushed,
        processes_trimmed,
        modified_flushed,
        low_priority_purged,
        standby_purged,
        pages_combined,
        freed_mb,
        duration_ms: start.elapsed().as_millis(),
    })
}
```

### Why This Order?

| Step                      | Why This Position                                                                 |
| ------------------------- | --------------------------------------------------------------------------------- |
| **1. File cache flush**   | Moves cache pages to standby so they can be purged in step 4                      |
| **2. Empty working sets** | Moves process pages to standby (clean) or modified (dirty)                        |
| **3. Flush modified**     | Writes dirty pages to disk so they become standby (clean) — prevents data loss    |
| **4. Purge standby**      | Now the standby list is maximally full — purge converts it all to free pages      |
| **5. Memory combine**     | Runs last because it's slow and benefits from the memory layout left by steps 1-4 |

### Cleaning Aggressiveness Levels

```rust
enum CleanLevel {
    /// Gentle: flush modified + combine only. No user impact.
    Gentle,
    /// Moderate: flush cache + low-priority standby. Minor cache loss.
    Moderate,
    /// Aggressive: empty working sets + purge ALL standby. Maximum free RAM.
    Aggressive,
    /// Nuclear: aggressive chain (4 ops) then full sequence again. Absolute maximum recovery.
    Nuclear,
}

fn clean_at_level(level: CleanLevel) -> Result<(), anyhow::Error> {
    enable_profile_privilege().ok();
    enable_quota_privilege().ok();

    match level {
        CleanLevel::Gentle => {
            flush_modified_list().ok();
            combine_memory_pages().ok();
        }
        CleanLevel::Moderate => {
            flush_file_cache().ok();
            flush_modified_list().ok();
            purge_low_priority_standby().ok();
            combine_memory_pages().ok();
        }
        CleanLevel::Aggressive => {
            flush_file_cache().ok();
            empty_all_working_sets().ok();
            flush_modified_list().ok();
            purge_low_priority_standby().ok();
            purge_standby_list().ok();
            combine_memory_pages().ok();
        }
        CleanLevel::Nuclear => {
            // Phase 1: aggressive chain (4 operations)
            flush_file_cache().ok();
            empty_all_working_sets().ok();
            flush_modified_list().ok();
            purge_standby_list().ok();
            // Phase 2: full sequence again for maximum recovery
            flush_file_cache().ok();
            empty_all_working_sets().ok();
            flush_modified_list().ok();
            purge_low_priority_standby().ok();
            purge_standby_list().ok();
            combine_memory_pages().ok();
        }
    }
    Ok(())
}
```

---

## 11. Full Compilable Source (Simplified Prototype)

> **Note:** The code below is a simplified single-file prototype for educational purposes.
> The actual codebase uses a multi-module architecture (`main.rs`, `cli.rs`, `cleaner.rs`,
> `stats.rs`, `display.rs`, `monitor.rs`, `ntapi.rs`, `privilege.rs`, `console.rs`,
> `context_menu.rs`) with
> additional features including smart settle detection, dry-run mode, JSON reports,
> per-process working set trimming with exclusion filters, continuous monitoring with
> cooldown, top-N process display, `--quiet`/`--no-color` output modes, and more.
> See [Section 12](#12-current-architecture--features) for the full feature set.

```rust
//! MagicX RAM Cleaner — World-class Windows Memory Cleaner CLI
//!
//! Requires: Windows 10/11, Administrator privileges
//! Build: cargo build --release

#![allow(non_snake_case)]

use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;

// ═══════════════════════════════════════════════════════════════════
// windows-sys imports
// ═══════════════════════════════════════════════════════════════════
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, LUID};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, OpenProcessToken,
    SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Memory::SetSystemFileCacheSize;
use windows_sys::Win32::System::ProcessStatus::{
    K32EmptyWorkingSet, K32EnumProcesses,
};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_SET_QUOTA,
};

// ═══════════════════════════════════════════════════════════════════
// NT API constants (not in windows-sys)
// ═══════════════════════════════════════════════════════════════════
const STATUS_SUCCESS: i32 = 0;

const SYSTEM_MEMORY_LIST_INFORMATION: u32 = 0x50;
const SYSTEM_COMBINE_PHYSICAL_MEMORY_INFORMATION: u32 = 0x82;

// ═══════════════════════════════════════════════════════════════════
// SYSTEM_MEMORY_LIST_COMMAND
// ═══════════════════════════════════════════════════════════════════
#[repr(i32)]
#[derive(Debug, Clone, Copy)]
enum MemoryListCommand {
    // CaptureAccessedBits = 0,
    // CaptureAndResetAccessedBits = 1,
    EmptyWorkingSets = 2,
    FlushModifiedList = 3,
    PurgeStandbyList = 4,
    PurgeLowPriorityStandbyList = 5,
}

// ═══════════════════════════════════════════════════════════════════
// NT API structs
// ═══════════════════════════════════════════════════════════════════
#[repr(C)]
#[derive(Debug, Default, Clone)]
struct SystemMemoryListInfo {
    zero_page_count: u64,
    free_page_count: u64,
    modified_page_count: u64,
    modified_no_write_page_count: u64,
    bad_page_count: u64,
    page_count_by_priority: [u64; 8],
    repurposed_pages_by_priority: [u64; 8],
    modified_page_count_page_file: u64,
}

#[repr(C)]
struct MemoryCombineInformationEx {
    handle: isize,
    pages_combined: usize,
}

// ═══════════════════════════════════════════════════════════════════
// ntdll.dll FFI
// ═══════════════════════════════════════════════════════════════════
#[link(name = "ntdll")]
extern "system" {
    fn NtSetSystemInformation(
        SystemInformationClass: u32,
        SystemInformation: *mut c_void,
        SystemInformationLength: u32,
    ) -> i32;

    fn NtQuerySystemInformation(
        SystemInformationClass: u32,
        SystemInformation: *mut c_void,
        SystemInformationLength: u32,
        ReturnLength: *mut u32,
    ) -> i32;
}

// ═══════════════════════════════════════════════════════════════════
// CLI definition
// ═══════════════════════════════════════════════════════════════════
#[derive(Parser)]
#[command(
    name = "magicx-ram-cleaner",
    version,
    about = "World-class Windows RAM Cleaner",
    long_about = "MagicX RAM Cleaner — Frees physical memory on Windows 10/11\n\
                  Requires Administrator privileges."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show detailed memory status
    Status,

    /// Clean RAM (default: moderate aggressiveness)
    Clean {
        /// Aggressiveness level: gentle, moderate, aggressive
        #[arg(short, long, default_value = "moderate")]
        level: String,
    },

    /// Purge the standby list only
    PurgeStandby {
        /// Only purge low-priority standby pages
        #[arg(short, long)]
        low_priority: bool,
    },

    /// Flush the modified page list to disk
    FlushModified,

    /// Empty all process working sets
    EmptyWorkingSets,

    /// Flush the file system cache
    FlushCache,

    /// Trigger memory page combining/deduplication
    Combine,
}

// ═══════════════════════════════════════════════════════════════════
// Privilege management
// ═══════════════════════════════════════════════════════════════════
fn enable_privilege(name: &str) -> Result<()> {
    unsafe {
        let mut token: HANDLE = 0;
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        ) == 0
        {
            return Err(anyhow!("OpenProcessToken failed: 0x{:08X}", GetLastError()));
        }

        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut luid: LUID = mem::zeroed();
        if LookupPrivilegeValueW(ptr::null(), wide.as_ptr(), &mut luid) == 0 {
            CloseHandle(token);
            return Err(anyhow!(
                "LookupPrivilegeValue('{}') failed: 0x{:08X}",
                name,
                GetLastError()
            ));
        }

        let mut tp: TOKEN_PRIVILEGES = mem::zeroed();
        tp.PrivilegeCount = 1;
        tp.Privileges[0].Luid = luid;
        tp.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;

        AdjustTokenPrivileges(
            token,
            0,
            &tp,
            mem::size_of::<TOKEN_PRIVILEGES>() as u32,
            ptr::null_mut(),
            ptr::null_mut(),
        );
        let err = GetLastError();
        CloseHandle(token);

        if err != 0 {
            Err(anyhow!(
                "AdjustTokenPrivileges('{}') failed: 0x{:08X}. Run as Administrator!",
                name,
                err
            ))
        } else {
            Ok(())
        }
    }
}

fn enable_all_privileges() -> Result<()> {
    enable_privilege("SeProfileSingleProcessPrivilege")
        .context("SeProfileSingleProcessPrivilege")?;
    enable_privilege("SeIncreaseQuotaPrivilege")
        .context("SeIncreaseQuotaPrivilege")?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
// Memory status queries
// ═══════════════════════════════════════════════════════════════════
fn get_memory_status() -> Result<MEMORYSTATUSEX> {
    unsafe {
        let mut status: MEMORYSTATUSEX = mem::zeroed();
        status.dwLength = mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut status) == 0 {
            return Err(anyhow!(
                "GlobalMemoryStatusEx failed: 0x{:08X}",
                GetLastError()
            ));
        }
        Ok(status)
    }
}

fn query_memory_lists() -> Result<SystemMemoryListInfo> {
    unsafe {
        let mut info = SystemMemoryListInfo::default();
        let mut ret_len: u32 = 0;
        let status = NtQuerySystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &mut info as *mut _ as *mut c_void,
            mem::size_of::<SystemMemoryListInfo>() as u32,
            &mut ret_len,
        );
        if status != STATUS_SUCCESS {
            Err(anyhow!(
                "NtQuerySystemInformation(MemoryList) failed: NTSTATUS 0x{:08X}",
                status as u32
            ))
        } else {
            Ok(info)
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Cleaning operations
// ═══════════════════════════════════════════════════════════════════
fn issue_memory_command(cmd: MemoryListCommand) -> Result<()> {
    unsafe {
        let mut val = cmd as i32;
        let status = NtSetSystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &mut val as *mut i32 as *mut c_void,
            mem::size_of::<i32>() as u32,
        );
        if status != STATUS_SUCCESS {
            Err(anyhow!(
                "NtSetSystemInformation({:?}) failed: NTSTATUS 0x{:08X}",
                cmd,
                status as u32
            ))
        } else {
            Ok(())
        }
    }
}

fn flush_file_cache() -> Result<()> {
    unsafe {
        if SetSystemFileCacheSize(usize::MAX, usize::MAX, 0) == 0 {
            Err(anyhow!(
                "SetSystemFileCacheSize failed: 0x{:08X}",
                GetLastError()
            ))
        } else {
            Ok(())
        }
    }
}

fn combine_memory() -> Result<usize> {
    unsafe {
        let mut info = MemoryCombineInformationEx {
            handle: 0,
            pages_combined: 0,
        };
        let status = NtSetSystemInformation(
            SYSTEM_COMBINE_PHYSICAL_MEMORY_INFORMATION,
            &mut info as *mut _ as *mut c_void,
            mem::size_of::<MemoryCombineInformationEx>() as u32,
        );
        if status != STATUS_SUCCESS {
            Err(anyhow!(
                "MemoryCombine failed: NTSTATUS 0x{:08X}",
                status as u32
            ))
        } else {
            Ok(info.pages_combined)
        }
    }
}

fn enum_processes_list() -> Result<Vec<u32>> {
    unsafe {
        let mut pids = vec![0u32; 4096];
        let mut bytes_returned: u32 = 0;
        if K32EnumProcesses(
            pids.as_mut_ptr(),
            (pids.len() * mem::size_of::<u32>()) as u32,
            &mut bytes_returned,
        ) == 0
        {
            return Err(anyhow!("EnumProcesses failed: 0x{:08X}", GetLastError()));
        }
        let count = bytes_returned as usize / mem::size_of::<u32>();
        pids.truncate(count);
        Ok(pids)
    }
}

fn empty_all_ws_per_process() -> Result<(u32, u32)> {
    let pids = enum_processes_list()?;
    let (mut ok, mut fail) = (0u32, 0u32);
    for &pid in &pids {
        if pid == 0 {
            continue;
        }
        unsafe {
            let h = OpenProcess(PROCESS_SET_QUOTA | PROCESS_QUERY_INFORMATION, 0, pid);
            if h == 0 {
                fail += 1;
                continue;
            }
            if K32EmptyWorkingSet(h) != 0 {
                ok += 1;
            } else {
                fail += 1;
            }
            CloseHandle(h);
        }
    }
    Ok((ok, fail))
}

// ═══════════════════════════════════════════════════════════════════
// Display helpers
// ═══════════════════════════════════════════════════════════════════
fn bytes_to_mb(b: u64) -> f64 {
    b as f64 / 1024.0 / 1024.0
}

fn pages_to_mb(pages: u64) -> f64 {
    (pages * 4096) as f64 / 1024.0 / 1024.0
}

fn print_status() -> Result<()> {
    let mem = get_memory_status()?;
    let lists = query_memory_lists().ok();

    println!("{}", "═══ Memory Status ═══".bold().cyan());
    println!(
        "Memory Load:      {}",
        format!("{}%", mem.dwMemoryLoad).yellow()
    );
    println!(
        "Total Physical:   {:.0} MB",
        bytes_to_mb(mem.ullTotalPhys)
    );
    println!(
        "Available:        {} MB",
        format!("{:.0}", bytes_to_mb(mem.ullAvailPhys)).green()
    );
    println!(
        "In Use:           {} MB",
        format!(
            "{:.0}",
            bytes_to_mb(mem.ullTotalPhys - mem.ullAvailPhys)
        )
        .red()
    );
    println!("Commit Total:     {:.0} MB", bytes_to_mb(mem.ullTotalPageFile));
    println!("Commit Available: {:.0} MB", bytes_to_mb(mem.ullAvailPageFile));

    if let Some(info) = lists {
        let total_standby: u64 = info.page_count_by_priority.iter().sum();
        println!();
        println!("{}", "═══ Page Lists ═══".bold().cyan());
        println!("Free pages:       {:.0} MB", pages_to_mb(info.free_page_count));
        println!("Zero pages:       {:.0} MB", pages_to_mb(info.zero_page_count));
        println!(
            "Modified pages:   {:.0} MB",
            pages_to_mb(info.modified_page_count)
        );
        println!(
            "Standby total:    {} MB",
            format!("{:.0}", pages_to_mb(total_standby)).yellow()
        );
        for (i, &count) in info.page_count_by_priority.iter().enumerate() {
            if count > 0 {
                println!("  Standby P{}:     {:.0} MB", i, pages_to_mb(count));
            }
        }
    }
    Ok(())
}

fn step_ok(name: &str) {
    println!("  {} {}", "[OK]".green().bold(), name);
}

fn step_fail(name: &str, e: &anyhow::Error) {
    println!("  {} {} — {}", "[FAIL]".red().bold(), name, e);
}

// ═══════════════════════════════════════════════════════════════════
// Main
// ═══════════════════════════════════════════════════════════════════
fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Status => {
            print_status()?;
        }

        Commands::Clean { ref level } => {
            enable_all_privileges().ok();

            let before = get_memory_status()?;
            let start = Instant::now();

            println!(
                "{} (level: {})",
                "═══ Cleaning RAM ═══".bold().cyan(),
                level.yellow()
            );

            match level.as_str() {
                "gentle" => {
                    match issue_memory_command(MemoryListCommand::FlushModifiedList) {
                        Ok(_) => step_ok("Flush modified list"),
                        Err(e) => step_fail("Flush modified list", &e),
                    }
                    match combine_memory() {
                        Ok(n) => step_ok(&format!("Memory combine ({} pages)", n)),
                        Err(e) => step_fail("Memory combine", &e),
                    }
                }
                "moderate" => {
                    match flush_file_cache() {
                        Ok(_) => step_ok("Flush file system cache"),
                        Err(e) => step_fail("Flush file system cache", &e),
                    }
                    match issue_memory_command(MemoryListCommand::FlushModifiedList) {
                        Ok(_) => step_ok("Flush modified list"),
                        Err(e) => step_fail("Flush modified list", &e),
                    }
                    match issue_memory_command(MemoryListCommand::PurgeLowPriorityStandbyList)
                    {
                        Ok(_) => step_ok("Purge low-priority standby"),
                        Err(e) => step_fail("Purge low-priority standby", &e),
                    }
                    match combine_memory() {
                        Ok(n) => step_ok(&format!("Memory combine ({} pages)", n)),
                        Err(e) => step_fail("Memory combine", &e),
                    }
                }
                "aggressive" => {
                    match flush_file_cache() {
                        Ok(_) => step_ok("Flush file system cache"),
                        Err(e) => step_fail("Flush file system cache", &e),
                    }
                    match issue_memory_command(MemoryListCommand::EmptyWorkingSets) {
                        Ok(_) => step_ok("Empty all working sets (kernel)"),
                        Err(e) => step_fail("Empty working sets", &e),
                    }
                    match issue_memory_command(MemoryListCommand::FlushModifiedList) {
                        Ok(_) => step_ok("Flush modified list"),
                        Err(e) => step_fail("Flush modified list", &e),
                    }
                    match issue_memory_command(MemoryListCommand::PurgeLowPriorityStandbyList)
                    {
                        Ok(_) => step_ok("Purge low-priority standby"),
                        Err(e) => step_fail("Purge low-priority standby", &e),
                    }
                    match issue_memory_command(MemoryListCommand::PurgeStandbyList) {
                        Ok(_) => step_ok("Purge ALL standby list"),
                        Err(e) => step_fail("Purge standby list", &e),
                    }
                    match combine_memory() {
                        Ok(n) => step_ok(&format!("Memory combine ({} pages)", n)),
                        Err(e) => step_fail("Memory combine", &e),
                    }
                }
                other => {
                    return Err(anyhow!(
                        "Unknown level '{}'. Use: gentle, moderate, aggressive",
                        other
                    ));
                }
            }

            let after = get_memory_status()?;
            let freed = after.ullAvailPhys.saturating_sub(before.ullAvailPhys);
            let elapsed = start.elapsed();

            println!();
            println!(
                "Freed: {} MB in {:.1}s",
                format!("{:.0}", bytes_to_mb(freed)).green().bold(),
                elapsed.as_secs_f64()
            );
            println!(
                "Available now: {} MB / {:.0} MB",
                format!("{:.0}", bytes_to_mb(after.ullAvailPhys))
                    .green()
                    .bold(),
                bytes_to_mb(after.ullTotalPhys)
            );
        }

        Commands::PurgeStandby { low_priority } => {
            enable_privilege("SeProfileSingleProcessPrivilege")?;
            if low_priority {
                issue_memory_command(MemoryListCommand::PurgeLowPriorityStandbyList)?;
                println!("{}", "Low-priority standby list purged.".green());
            } else {
                issue_memory_command(MemoryListCommand::PurgeStandbyList)?;
                println!("{}", "ALL standby list purged.".green());
            }
        }

        Commands::FlushModified => {
            enable_privilege("SeProfileSingleProcessPrivilege")?;
            issue_memory_command(MemoryListCommand::FlushModifiedList)?;
            println!("{}", "Modified page list flushed.".green());
        }

        Commands::EmptyWorkingSets => {
            enable_privilege("SeProfileSingleProcessPrivilege")?;
            issue_memory_command(MemoryListCommand::EmptyWorkingSets)?;
            println!("{}", "All working sets emptied.".green());
        }

        Commands::FlushCache => {
            enable_privilege("SeIncreaseQuotaPrivilege")?;
            flush_file_cache()?;
            println!("{}", "File system cache flushed.".green());
        }

        Commands::Combine => {
            enable_privilege("SeProfileSingleProcessPrivilege")?;
            let pages = combine_memory()?;
            println!(
                "Memory combining complete: {} pages ({:.1} MB) deduplicated.",
                pages,
                pages_to_mb(pages as u64)
            );
        }
    }

    Ok(())
}
```

---

## Appendix A: Embedding an Admin Manifest

To auto-request elevation (UAC prompt) when the exe is launched, embed a Windows application manifest.

### Create `app.manifest`:

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
```

### Create `build.rs`:

```rust
fn main() {
    // Embed the manifest for admin elevation
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", 
             std::path::Path::new("app.manifest").canonicalize().unwrap().display());
    
    // Alternative: use the embed-manifest crate
    // Or use winres crate:
}
```

Or use the **`embed-manifest`** crate (simpler):

```toml
[build-dependencies]
embed-manifest = "1.4"
```

```rust
// build.rs
use embed_manifest::{embed_manifest, new_manifest, ExecutionLevel};

fn main() {
    embed_manifest(new_manifest("MagicX RAM Cleaner")
        .requested_execution_level(ExecutionLevel::RequireAdministrator))
        .expect("Failed to embed manifest");
}
```

---

## Appendix B: NTSTATUS Error Code Reference

| NTSTATUS     | Name                          | Meaning                                                                 |
| ------------ | ----------------------------- | ----------------------------------------------------------------------- |
| `0x00000000` | `STATUS_SUCCESS`              | Operation succeeded                                                     |
| `0xC0000022` | `STATUS_ACCESS_DENIED`        | Missing required privilege — run as admin and call `enable_privilege()` |
| `0xC0000004` | `STATUS_INFO_LENGTH_MISMATCH` | Buffer too small for query result                                       |
| `0xC000000D` | `STATUS_INVALID_PARAMETER`    | Invalid command value passed                                            |
| `0xC0000003` | `STATUS_INVALID_INFO_CLASS`   | Invalid SystemInformationClass value                                    |

---

## Appendix C: windows-sys Feature Discovery

To find which feature flag provides a specific function or type:

1. Go to https://microsoft.github.io/windows-docs-rs/
2. Search for the function name (e.g., `GlobalMemoryStatusEx`)
3. The docs show the required feature at the top of the page

Or use `cargo doc --open` on windows-sys to browse locally.

---

## Appendix D: Deprecated

> This appendix previously documented the `sysinfo` crate as an alternative for
> display-only statistics. The project now uses `GlobalMemoryStatusEx` and
> `K32GetPerformanceInfo` directly via `windows-sys`, with `CreateToolhelp32Snapshot`
> for per-process enumeration. No third-party system info crate is needed.

---

## 12. Current Architecture & Features

The codebase has evolved from the simplified prototype in Section 11 into a
multi-module architecture with the following structure:

```
src/
  main.rs          — entry point: mod declarations, main(), run(), command dispatch
  cli.rs           — clap Parser, Commands enum, help text constants, STYLES
  cleaner.rs       — cleaning operations & orchestration (smart_clean, CleanLevel)
  console.rs       — Windows console management (dynamic attach/alloc for SUBSYSTEM:WINDOWS, ANSI, notifications)
  context_menu.rs  — Windows Desktop context menu integration (registry install/uninstall)
  display.rs       — ALL terminal formatting: banner, status, clean output, box drawing
  monitor.rs       — continuous monitoring loop, Ctrl+C handler, auto-clean
  ntapi.rs         — NT kernel FFI (NtSetSystemInformation, NtQuerySystemInformation)
  privilege.rs     — Windows privilege elevation (Se*Privilege) + admin check
  stats.rs         — memory statistics, Win32 API calls, MemorySnapshot
```

### Key Types

| Type                 | Module    | Purpose                                                               |
| -------------------- | --------- | --------------------------------------------------------------------- |
| `CleanLevel`         | `cleaner` | Enum: `Gentle`, `Moderate`, `Aggressive`, `Nuclear`                   |
| `CleanResult`        | `cleaner` | Per-operation result: `freed_bytes`, `message`, `success`, `elapsed`  |
| `SmartCleanResult`   | `cleaner` | Aggregate result: `level`, `results[]`, `before`/`after` snapshots    |
| `SettleMode`         | `cleaner` | Enum: `Full` (tight threshold) / `Quick` (faster, relaxed threshold)  |
| `MemoryListCommand`  | `cleaner` | Enum mapping NT kernel commands (with `display_info()` for dry-run)   |
| `MemorySnapshot`     | `stats`   | Full memory state (physical + page file + kernel memory lists)        |
| `QuickMemoryReading` | `stats`   | Lightweight snapshot (`available_bytes` + `load_percent` only)        |
| `MemoryListInfo`     | `stats`   | Kernel page list breakdown (standby priorities, modified, free, zero) |
| `ProcessMemoryInfo`  | `stats`   | Per-process memory info (PID, name, working set, private bytes)       |

### CLI Features

| Feature               | Flag / Option             | Description                                                                 |
| --------------------- | ------------------------- | --------------------------------------------------------------------------- |
| Smart clean           | `clean [-l LEVEL]`        | Context-aware cleaning at 4 levels with settle detection between operations |
| Dry-run mode          | `clean --dry-run`         | Preview what operations would run without executing them                    |
| JSON report           | `clean --report FILE`     | Write `SmartCleanResult` as pretty-printed JSON to a file                   |
| Status display        | `status [--detailed]`     | Show memory usage with optional kernel page list breakdown                  |
| JSON status           | `status --json`           | Machine-readable JSON output of `MemorySnapshot`                            |
| Top processes         | `status --top N`          | Show top N processes ranked by working set (physical RAM) usage             |
| Per-process trimming  | `empty-workingsets -p`    | Trim working sets process-by-process instead of kernel-wide                 |
| Exclusion filters     | `--exclude NAME`          | Case-insensitive process name exclusion (implies `--per-process`)           |
| Continuous monitoring | `monitor -t THRESHOLD`    | Auto-clean when RAM usage exceeds threshold percentage                      |
| Monitor cooldown      | `monitor --cooldown SECS` | Minimum seconds between auto-cleans (default: 2× interval)                  |
| Quiet mode            | `-q` / `--quiet`          | Suppress banner and non-essential output (global flag)                      |
| No colour             | `--no-color`              | Disable coloured terminal output (also respects `NO_COLOR` env var)         |
| Notify mode           | `--notify`                | Skip console attachment, run silently, show balloon notification with results |
| Context menu install  | `context-menu install`    | Add Desktop right-click submenu for quick cleaning access                   |
| Context menu remove   | `context-menu uninstall`  | Remove Desktop right-click submenu                                          |

### Settle Detection

After each kernel memory operation, `wait_for_settle()` polls `QuickMemoryReading`
at 50 ms intervals to detect when the available memory has stabilised. This prevents
measuring freed memory before the kernel has finished reclaiming pages. Two modes:

- **`SettleMode::Full`** — tight jitter threshold (0.1% of total RAM), used after
  aggressive/nuclear operations
- **`SettleMode::Quick`** — relaxed threshold (0.3% of total RAM) and shorter timeout,
  used after gentle operations

### DRY Cleaning Pattern

All kernel memory operations share a common `execute_kernel_memory_op()` helper that:
1. Captures a `QuickMemoryReading` before and after the kernel call
2. Calls `wait_for_settle()` with the appropriate `SettleMode`
3. Computes `freed_bytes` and elapsed time
4. Returns a structured `CleanResult`

This eliminates code duplication across `flush_modified_list`, `purge_standby_list`,
`empty_working_sets_kernel`, etc.
