# Windows Memory Management Internals for RAM Cleaners

## Complete Technical Reference

---

## 1. Windows Physical Memory Architecture

Windows manages physical memory (RAM) through a **Page Frame Number (PFN) database**. Each physical page (typically 4 KB) is tracked and belongs to exactly one of these lists:

### Memory Page Lists

| List                              | Description                                                                                                                                                                                  |
| --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Free List**                     | Pages with unknown/garbage content. Not zeroed. Can be allocated after zeroing.                                                                                                              |
| **Zero List**                     | Pages that have been zeroed by the zero-page thread. Ready for immediate allocation.                                                                                                         |
| **Standby List** (priorities 0-7) | Clean pages removed from working sets. Still contain valid data (soft fault = free reuse). Priority 0 = lowest, 7 = highest. The memory manager repurposes low-priority standby pages first. |
| **Modified List**                 | Dirty pages removed from working sets. Must be written to disk (pagefile or mapped file) before reuse.                                                                                       |
| **Modified-no-write List**        | Modified pages that should NOT be written to disk (e.g., filesystem metadata being held for consistency).                                                                                    |
| **Bad List**                      | Pages with hardware errors. Never used.                                                                                                                                                      |

### How Memory Flows Between Lists

```
Process Working Set
     │
     ├──(page trimmed, clean)──→ Standby List (priority based on access pattern)
     │                                │
     │                                ├──(repurposed)──→ Zero List / Free List
     │                                └──(soft fault)──→ Back to Working Set (cheap!)
     │
     └──(page trimmed, dirty)──→ Modified List
                                      │
                                      ├──(written to disk)──→ Standby List
                                      └──(no-write flag)──→ Modified-no-write List

Free List ──(zeroed by zero thread)──→ Zero List ──(allocated)──→ Working Set
```

### Standby List Priorities (0-7)

The standby list is actually 8 sub-lists (priority 0 through 7):
- **Priority 0**: Lowest - repurposed first (e.g., SuperFetch prefetched pages, low-priority I/O)
- **Priority 1-2**: Low priority pages
- **Priority 3-4**: Normal priority (typical file cache and working set pages)  
- **Priority 5-6**: Higher priority (frequently accessed pages)
- **Priority 7**: Highest - repurposed last

The memory manager's page replacement algorithm repurposes standby pages from lowest priority first.

---

## 2. NtSetSystemInformation with SystemMemoryListInformation

### System Information Class

```c
// SYSTEM_INFORMATION_CLASS enum value
#define SystemMemoryListInformation 0x50  // 80 decimal
```

### SYSTEM_MEMORY_LIST_COMMAND Enum

This is the **command enum** passed as data to `NtSetSystemInformation` when using `SystemMemoryListInformation`:

```c
typedef enum _SYSTEM_MEMORY_LIST_COMMAND {
    MemoryCaptureAccessedBits       = 0,  // Captures PTE accessed bits without resetting
    MemoryCaptureAndResetAccessedBits = 1, // Captures AND resets PTE accessed bits  
    MemoryEmptyWorkingSets          = 2,  // Empties all process working sets
    MemoryFlushModifiedList         = 3,  // Writes modified pages to disk → standby
    MemoryPurgeStandbyList          = 4,  // Purges ALL standby list pages → free list
    MemoryPurgeLowPriorityStandbyList = 5, // Purges only low-priority standby pages
    MemoryCommandMax                = 6   // Sentinel / count value
} SYSTEM_MEMORY_LIST_COMMAND;
```

#### Detailed Command Descriptions

| Value | Command                             | Effect                                                                                                                                                                                            | Required Privilege                |
| ----- | ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------- |
| 0     | `MemoryCaptureAccessedBits`         | Snapshots the Accessed bit from PTEs for all pages. Used by the working set manager to track page access patterns.                                                                                | `SeProfileSingleProcessPrivilege` |
| 1     | `MemoryCaptureAndResetAccessedBits` | Same as above but also clears the Accessed bits. This is what the working set trimmer uses internally to age pages.                                                                               | `SeProfileSingleProcessPrivilege` |
| 2     | `MemoryEmptyWorkingSets`            | Forces all processes to release their working set pages. Pages go to standby (if clean) or modified (if dirty) lists. Equivalent to calling `EmptyWorkingSet()` on every process.                 | `SeProfileSingleProcessPrivilege` |
| 3     | `MemoryFlushModifiedList`           | Forces the modified page writer to write all modified pages to their backing store (pagefile or mapped file). After writing, pages move to the standby list.                                      | `SeProfileSingleProcessPrivilege` |
| 4     | `MemoryPurgeStandbyList`            | **The "RAM cleaner" command.** Purges ALL standby list pages across all priority levels, converting them to free pages. This is what EmptyStandbyList.exe does. **Warning:** Destroys file cache! | `SeProfileSingleProcessPrivilege` |
| 5     | `MemoryPurgeLowPriorityStandbyList` | Purges only priority 0 standby pages. Gentler than full purge - preserves high-value cached data.                                                                                                 | `SeProfileSingleProcessPrivilege` |

### SYSTEM_MEMORY_LIST_INFORMATION Struct (for querying)

When you **query** `SystemMemoryListInformation`, you get this struct back:

```c
typedef struct _SYSTEM_MEMORY_LIST_INFORMATION {
    ULONGLONG ZeroPageCount;                    // Pages on the zero list
    ULONGLONG FreePageCount;                    // Pages on the free list
    ULONGLONG ModifiedPageCount;                // Pages on the modified list
    ULONGLONG ModifiedNoWritePageCount;         // Pages on the modified-no-write list
    ULONGLONG BadPageCount;                     // Pages on the bad list
    ULONGLONG PageCountByPriority[8];           // Standby pages per priority (0-7)
    ULONGLONG RepurposedPagesByPriority[8];     // Cumulative repurposed count per priority
    ULONGLONG ModifiedPageCountPageFile;        // Modified pages destined for pagefile
    // Note: Some versions have additional fields:
    // ULONGLONG StandbyRepurposedByPriority[8]; // In newer Windows versions
} SYSTEM_MEMORY_LIST_INFORMATION, *PSYSTEM_MEMORY_LIST_INFORMATION;
```

> **Note:** The exact struct layout varies by Windows version. The version shown in `theKorzh/Standby-RAM-Cleaner-service` includes `StandbyRepurposedByPriority[8]` as a third array. Always check against the System Informer (Process Hacker) `phnt` headers for the latest layout.

---

## 3. How EmptyStandbyList.exe Works Internally

EmptyStandbyList.exe (by Wen Jia Liu / wj32, creator of Process Hacker) is extremely simple. Here's the complete internal flow:

### Step 1: Enable Required Privilege
```c
// Open process token
OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &hToken);

// Enable SeProfileSingleProcessPrivilege
TOKEN_PRIVILEGES tp;
tp.PrivilegeCount = 1;
LookupPrivilegeValue(NULL, SE_PROF_SINGLE_PROCESS_NAME, &tp.Privileges[0].Luid);
tp.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
AdjustTokenPrivileges(hToken, FALSE, &tp, sizeof(tp), NULL, NULL);
```

### Step 2: Load NtSetSystemInformation
```c
typedef NTSTATUS (WINAPI* NtSetSystemInformation_t)(
    SYSTEM_INFORMATION_CLASS SystemInformationClass,
    PVOID SystemInformation,
    ULONG SystemInformationLength
);

NtSetSystemInformation_t NtSetSystemInformation = 
    (NtSetSystemInformation_t)GetProcAddress(
        GetModuleHandle("ntdll.dll"), 
        "NtSetSystemInformation"
    );
```

### Step 3: Issue the Command
```c
// The command is passed as a SYSTEM_MEMORY_LIST_COMMAND value
SYSTEM_MEMORY_LIST_COMMAND command = MemoryPurgeStandbyList; // = 4

NTSTATUS status = NtSetSystemInformation(
    SystemMemoryListInformation,  // 0x50
    &command,                      // pointer to the command value
    sizeof(command)                // sizeof(int) = 4
);
```

### Command-line arguments (typical implementation):
```
EmptyStandbyList.exe workingsets    → command = MemoryEmptyWorkingSets (2)
EmptyStandbyList.exe modifiedlist   → command = MemoryFlushModifiedList (3)
EmptyStandbyList.exe standbylist    → command = MemoryPurgeStandbyList (4)
EmptyStandbyList.exe priority0standbylist → command = MemoryPurgeLowPriorityStandbyList (5)
```

That's it. The entire tool is ~50 lines of C code.

---

## 4. Required Privileges

### SeProfileSingleProcessPrivilege (SE_PROF_SINGLE_PROCESS_NAME)
- **Constant:** `SE_PROF_SINGLE_PROCESS_NAME` = `"SeProfileSingleProcessPrivilege"`
- **Privilege LUID value:** 13
- **Required for:** All `SYSTEM_MEMORY_LIST_COMMAND` operations (empty working sets, purge standby, flush modified)
- **Default holders:** Administrators group
- **Notes:** Must be *enabled* in the token (it's present but disabled by default for admins)

### SeIncreaseQuotaPrivilege (SE_INCREASE_QUOTA_NAME)
- **Constant:** `SE_INCREASE_QUOTA_NAME` = `"SeIncreaseQuotaPrivilege"`  
- **Privilege LUID value:** 5
- **Required for:** `SetSystemFileCacheSize()` - trimming the filesystem cache working set
- **Default holders:** Administrators, Local Service, Network Service
- **Notes:** Also called "Adjust memory quotas for a process" in security policy

### SeDebugPrivilege (SE_DEBUG_NAME)
- **Not strictly required** for memory list commands, but useful for:
  - Opening handles to other processes to call `EmptyWorkingSet()` per-process
  - Reading memory info from protected processes

### How to Enable a Privilege (C)
```c
BOOL EnablePrivilege(LPCWSTR privilegeName) {
    HANDLE hToken;
    if (!OpenProcessToken(GetCurrentProcess(), 
                          TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &hToken))
        return FALSE;
    
    TOKEN_PRIVILEGES tp;
    tp.PrivilegeCount = 1;
    tp.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
    
    if (!LookupPrivilegeValueW(NULL, privilegeName, &tp.Privileges[0].Luid)) {
        CloseHandle(hToken);
        return FALSE;
    }
    
    BOOL result = AdjustTokenPrivileges(hToken, FALSE, &tp, sizeof(tp), NULL, NULL);
    DWORD err = GetLastError(); // Must check - AdjustTokenPrivileges succeeds even if not all adjusted
    CloseHandle(hToken);
    
    return result && (err == ERROR_SUCCESS);
}

// Usage:
EnablePrivilege(SE_PROF_SINGLE_PROCESS_NAME);   // For NtSetSystemInformation memory commands
EnablePrivilege(SE_INCREASE_QUOTA_NAME);         // For SetSystemFileCacheSize
```

---

## 5. Complete API Reference

### 5.1 File System Cache Trimming

#### SetSystemFileCacheSize
```c
BOOL SetSystemFileCacheSize(
    SIZE_T MinimumFileCacheSize,  // Minimum cache size in bytes; (SIZE_T)-1 to flush
    SIZE_T MaximumFileCacheSize,  // Maximum cache size in bytes; (SIZE_T)-1 to flush
    DWORD  Flags                  // FILE_CACHE_* flags
);

// Flags:
#define FILE_CACHE_MAX_HARD_ENABLE   0x00000001
#define FILE_CACHE_MAX_HARD_DISABLE  0x00000002
#define FILE_CACHE_MIN_HARD_ENABLE   0x00000004
#define FILE_CACHE_MIN_HARD_DISABLE  0x00000008

// To flush the file system cache (requires SeIncreaseQuotaPrivilege):
SetSystemFileCacheSize((SIZE_T)-1, (SIZE_T)-1, 0);
```

#### GetSystemFileCacheSize
```c
BOOL GetSystemFileCacheSize(
    PSIZE_T lpMinimumFileCacheSize,
    PSIZE_T lpMaximumFileCacheSize,
    PDWORD  lpFlags
);
```

**DLL:** kernel32.dll  
**Header:** memoryapi.h / Windows.h  
**Required privilege:** `SeIncreaseQuotaPrivilege`

### 5.2 Working Set Trimming (Per-Process)

#### EmptyWorkingSet
```c
// Removes as many pages as possible from the working set of the specified process
BOOL EmptyWorkingSet(HANDLE hProcess);
// DLL: psapi.dll or kernel32.dll (K32EmptyWorkingSet)
// Equivalent to: SetProcessWorkingSetSizeEx(hProcess, (SIZE_T)-1, (SIZE_T)-1, 0);
```

#### SetProcessWorkingSetSizeEx
```c
BOOL SetProcessWorkingSetSizeEx(
    HANDLE hProcess,
    SIZE_T dwMinimumWorkingSetSize,  // (SIZE_T)-1 to empty
    SIZE_T dwMaximumWorkingSetSize,  // (SIZE_T)-1 to empty
    DWORD  Flags                     // QUOTA_LIMITS_HARDWS_* flags
);

// Flags:
#define QUOTA_LIMITS_HARDWS_MIN_ENABLE   0x00000001
#define QUOTA_LIMITS_HARDWS_MIN_DISABLE  0x00000002
#define QUOTA_LIMITS_HARDWS_MAX_ENABLE   0x00000004
#define QUOTA_LIMITS_HARDWS_MAX_DISABLE  0x00000008
```

#### Enumerating All Processes to Trim
```c
// To trim ALL processes (like MemoryEmptyWorkingSets but manually):
DWORD pids[4096], bytesReturned;
EnumProcesses(pids, sizeof(pids), &bytesReturned);
int count = bytesReturned / sizeof(DWORD);

for (int i = 0; i < count; i++) {
    HANDLE hProc = OpenProcess(PROCESS_SET_QUOTA | PROCESS_QUERY_INFORMATION, FALSE, pids[i]);
    if (hProc) {
        EmptyWorkingSet(hProc);
        CloseHandle(hProc);
    }
}
```

### 5.3 Registry/System Cache Cleanup

There is no direct public API for "registry cache cleanup." However:

1. **CmSetLazyFlushState** (undocumented, kernel-mode) - controls registry lazy flushing
2. **RegFlushKey()** - forces a specific registry key to be written to disk
3. The system cache (filesystem cache) serves registry hive files too, so `SetSystemFileCacheSize` flush also impacts registry cache

### 5.4 Memory Combining/Compression Management

Windows 10+ has a **Memory Compression** system (the "Memory Compression" process in Task Manager):

#### Querying Memory Compression Info
```c
// SystemInformationClass = SystemMemoryListInformation (0x50) for page lists
// SystemInformationClass = SystemCombinePhysicalMemoryInformation (0x82 / 130) for combining

// Memory combining (deduplication) - NtSetSystemInformation:
#define SystemCombinePhysicalMemoryInformation 130  // 0x82

typedef struct _MEMORY_COMBINE_INFORMATION_EX {
    HANDLE Handle;
    ULONG_PTR PagesCombined;
} MEMORY_COMBINE_INFORMATION_EX;

// Trigger memory combining:
MEMORY_COMBINE_INFORMATION_EX combineInfo = {0};
NtSetSystemInformation(SystemCombinePhysicalMemoryInformation, 
                       &combineInfo, sizeof(combineInfo));
// Returns number of pages combined in combineInfo.PagesCombined
```

#### Memory Compression Store Info (Windows 10+)
```c
// SystemInformationClass values for compression:
#define SystemStoreInformation           180  // 0xB4
#define SystemMemoryTopology             159  // 0x9F
#define SystemMemoryChannelInformation   160  // 0xA0
#define SystemMemoryPartitionInformation 199  // 0xC7
```

### 5.5 Large Page and AWE Memory Management

#### Large Pages
```c
// Check large page support and get minimum size:
SIZE_T largePageMin = GetLargePageMinimum(); // Usually 2MB on x64

// Allocate large pages (requires SeLockMemoryPrivilege):
PVOID mem = VirtualAlloc(NULL, size, 
    MEM_COMMIT | MEM_RESERVE | MEM_LARGE_PAGES, PAGE_READWRITE);
```

#### AWE (Address Windowing Extensions)
```c
// Requires SeLockMemoryPrivilege
ULONG_PTR numberOfPages = ...;
ULONG_PTR *pfnArray = malloc(numberOfPages * sizeof(ULONG_PTR));

// Allocate physical pages
AllocateUserPhysicalPages(GetCurrentProcess(), &numberOfPages, pfnArray);

// Map into virtual address space
PVOID virtualAddr = VirtualAlloc(NULL, size, MEM_RESERVE | MEM_PHYSICAL, PAGE_READWRITE);
MapUserPhysicalPages(virtualAddr, numberOfPages, pfnArray);

// Free
FreeUserPhysicalPages(GetCurrentProcess(), &numberOfPages, pfnArray);
```

---

## 6. NtSetSystemInformation Function Signatures

### C/C++ (Dynamic Loading from ntdll.dll)

```c
// Function pointer types
typedef LONG NTSTATUS;
#define STATUS_SUCCESS ((NTSTATUS)0x00000000)

typedef NTSTATUS (NTAPI* PFN_NtSetSystemInformation)(
    ULONG SystemInformationClass,  // SYSTEM_INFORMATION_CLASS
    PVOID SystemInformation,        // Pointer to command/data
    ULONG SystemInformationLength   // Size of data
);

typedef NTSTATUS (NTAPI* PFN_NtQuerySystemInformation)(
    ULONG SystemInformationClass,
    PVOID SystemInformation,
    ULONG SystemInformationLength,
    PULONG ReturnLength
);

// Load at runtime:
HMODULE hNtdll = GetModuleHandleW(L"ntdll.dll");
PFN_NtSetSystemInformation NtSetSystemInformation = 
    (PFN_NtSetSystemInformation)GetProcAddress(hNtdll, "NtSetSystemInformation");
PFN_NtQuerySystemInformation NtQuerySystemInformation = 
    (PFN_NtQuerySystemInformation)GetProcAddress(hNtdll, "NtQuerySystemInformation");
```

### Rust (FFI with windows-sys or raw FFI)

#### Option A: Using `windows-sys` + manual ntdll FFI (recommended — used by this project)

`windows-sys` provides official Microsoft bindings for Win32 APIs. For undocumented
NT APIs (`NtSetSystemInformation`, `NtQuerySystemInformation`), declare them manually
via `#[link(name = "ntdll")]`.

```toml
# Cargo.toml
[dependencies]
windows-sys = { version = "0.61", features = [
    "Win32_Foundation",
    "Win32_System_Memory",
    "Win32_System_Threading",
    "Win32_Security",
    "Win32_Security_Authorization",
    "Win32_System_ProcessStatus",
    "Win32_System_SystemInformation",
    "Win32_System_Console",
    "Win32_System_Diagnostics_ToolHelp",
]}
```

```rust
// Manual FFI for undocumented NT APIs (not in windows-sys)
#[link(name = "ntdll")]
extern "system" {
    fn NtSetSystemInformation(
        SystemInformationClass: u32,
        SystemInformation: *mut std::ffi::c_void,
        SystemInformationLength: u32,
    ) -> i32; // NTSTATUS

    fn NtQuerySystemInformation(
        SystemInformationClass: u32,
        SystemInformation: *mut std::ffi::c_void,
        SystemInformationLength: u32,
        ReturnLength: *mut u32,
    ) -> i32; // NTSTATUS
}
```

> **Note:** The `ntapi` crate (stale since 2021) and `winapi` (legacy, replaced by
> `windows-sys`) are **not recommended**. `windows-sys` + manual FFI for the 2-3
> undocumented functions is cleaner and better maintained.

#### Option B: Raw FFI (no external crate dependencies)
```rust
#![allow(non_snake_case, non_camel_case_types)]

use std::ffi::c_void;
use std::os::raw::c_ulong;

type NTSTATUS = i32;
type PVOID = *mut c_void;
type ULONG = c_ulong;
type PULONG = *mut c_ulong;
type HANDLE = *mut c_void;
type BOOL = i32;
type LUID = u64;
type DWORD = u32;
type SIZE_T = usize;
type ULONGLONG = u64;

const STATUS_SUCCESS: NTSTATUS = 0;
const SYSTEM_MEMORY_LIST_INFORMATION: ULONG = 0x50;

// SYSTEM_MEMORY_LIST_COMMAND values
const MEMORY_CAPTURE_ACCESSED_BITS: i32 = 0;
const MEMORY_CAPTURE_AND_RESET_ACCESSED_BITS: i32 = 1;
const MEMORY_EMPTY_WORKING_SETS: i32 = 2;
const MEMORY_FLUSH_MODIFIED_LIST: i32 = 3;
const MEMORY_PURGE_STANDBY_LIST: i32 = 4;
const MEMORY_PURGE_LOW_PRIORITY_STANDBY_LIST: i32 = 5;
const MEMORY_COMMAND_MAX: i32 = 6;

#[repr(C)]
#[derive(Debug, Default)]
struct SystemMemoryListInformation {
    zero_page_count: ULONGLONG,
    free_page_count: ULONGLONG,
    modified_page_count: ULONGLONG,
    modified_no_write_page_count: ULONGLONG,
    bad_page_count: ULONGLONG,
    page_count_by_priority: [ULONGLONG; 8],       // Standby pages per priority 0-7
    repurposed_pages_by_priority: [ULONGLONG; 8],  // Cumulative repurposed count
    modified_page_count_page_file: ULONGLONG,
}

#[repr(C)]
struct TokenPrivileges {
    privilege_count: DWORD,
    privileges: [LuidAndAttributes; 1],
}

#[repr(C)]
struct LuidAndAttributes {
    luid: LUID,
    attributes: DWORD,
}

#[repr(C)]
struct MemoryStatusEx {
    dw_length: DWORD,
    dw_memory_load: DWORD,
    ull_total_phys: ULONGLONG,
    ull_avail_phys: ULONGLONG,
    ull_total_page_file: ULONGLONG,
    ull_avail_page_file: ULONGLONG,
    ull_total_virtual: ULONGLONG,
    ull_avail_virtual: ULONGLONG,
    ull_avail_extended_virtual: ULONGLONG,
}

#[repr(C)]
struct PerformanceInformation {
    cb: DWORD,
    commit_total: SIZE_T,
    commit_limit: SIZE_T,
    commit_peak: SIZE_T,
    physical_total: SIZE_T,
    physical_available: SIZE_T,
    system_cache: SIZE_T,
    kernel_total: SIZE_T,
    kernel_paged: SIZE_T,
    kernel_nonpaged: SIZE_T,
    page_size: SIZE_T,
    handle_count: DWORD,
    process_count: DWORD,
    thread_count: DWORD,
}

// Combine info for memory deduplication
#[repr(C)]
struct MemoryCombineInformationEx {
    handle: HANDLE,
    pages_combined: SIZE_T,
}

const SE_PRIVILEGE_ENABLED: DWORD = 0x00000002;
const TOKEN_ADJUST_PRIVILEGES: DWORD = 0x0020;
const TOKEN_QUERY: DWORD = 0x0008;
const SYSTEM_COMBINE_PHYSICAL_MEMORY_INFORMATION: ULONG = 130;

extern "system" {
    // ntdll.dll
    fn NtSetSystemInformation(
        system_information_class: ULONG,
        system_information: PVOID,
        system_information_length: ULONG,
    ) -> NTSTATUS;
    
    fn NtQuerySystemInformation(
        system_information_class: ULONG,
        system_information: PVOID,
        system_information_length: ULONG,
        return_length: PULONG,
    ) -> NTSTATUS;

    // kernel32.dll
    fn GetCurrentProcess() -> HANDLE;
    fn GetModuleHandleW(module_name: *const u16) -> HANDLE;
    fn GetProcAddress(module: HANDLE, proc_name: *const u8) -> PVOID;
    fn GetLastError() -> DWORD;
    fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> BOOL;
    fn SetSystemFileCacheSize(min: SIZE_T, max: SIZE_T, flags: DWORD) -> BOOL;
    fn GetSystemFileCacheSize(min: *mut SIZE_T, max: *mut SIZE_T, flags: *mut DWORD) -> BOOL;
    
    // advapi32.dll
    fn OpenProcessToken(process: HANDLE, access: DWORD, token: *mut HANDLE) -> BOOL;
    fn LookupPrivilegeValueW(
        system: *const u16, name: *const u16, luid: *mut LUID
    ) -> BOOL;
    fn AdjustTokenPrivileges(
        token: HANDLE, disable_all: BOOL, new_state: *const TokenPrivileges,
        buffer_length: DWORD, previous_state: PVOID, return_length: PULONG,
    ) -> BOOL;
    
    // psapi.dll / kernel32.dll
    fn K32GetPerformanceInfo(
        performance_info: *mut PerformanceInformation, cb: DWORD
    ) -> BOOL;
    fn K32EmptyWorkingSet(process: HANDLE) -> BOOL;
}

// Link directives for Rust
#[link(name = "ntdll")]
extern "system" {}

#[link(name = "kernel32")]
extern "system" {}

#[link(name = "advapi32")]
extern "system" {}
```

#### Complete Rust RAM Cleaner Example
```rust
use std::mem;
use std::ptr;

fn enable_privilege(privilege_name: &str) -> Result<(), String> {
    unsafe {
        let mut token: HANDLE = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), 
                           TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, 
                           &mut token) == 0 {
            return Err(format!("OpenProcessToken failed: {}", GetLastError()));
        }
        
        // Convert privilege name to wide string
        let wide_name: Vec<u16> = privilege_name.encode_utf16().chain(std::iter::once(0)).collect();
        
        let mut tp = TokenPrivileges {
            privilege_count: 1,
            privileges: [LuidAndAttributes {
                luid: 0,
                attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        
        if LookupPrivilegeValueW(ptr::null(), wide_name.as_ptr(), &mut tp.privileges[0].luid) == 0 {
            return Err(format!("LookupPrivilegeValue failed: {}", GetLastError()));
        }
        
        if AdjustTokenPrivileges(token, 0, &tp, 
                                 mem::size_of::<TokenPrivileges>() as u32,
                                 ptr::null_mut(), ptr::null_mut()) == 0 {
            return Err(format!("AdjustTokenPrivileges failed: {}", GetLastError()));
        }
        
        if GetLastError() != 0 {
            return Err("Privilege not held by token".to_string());
        }
        
        Ok(())
    }
}

fn purge_standby_list() -> Result<(), String> {
    enable_privilege("SeProfileSingleProcessPrivilege")?;
    
    unsafe {
        let mut command: i32 = MEMORY_PURGE_STANDBY_LIST;
        let status = NtSetSystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &mut command as *mut i32 as PVOID,
            mem::size_of::<i32>() as ULONG,
        );
        
        if status != STATUS_SUCCESS {
            return Err(format!("NtSetSystemInformation failed: 0x{:08X}", status));
        }
    }
    Ok(())
}

fn empty_working_sets() -> Result<(), String> {
    enable_privilege("SeProfileSingleProcessPrivilege")?;
    
    unsafe {
        let mut command: i32 = MEMORY_EMPTY_WORKING_SETS;
        let status = NtSetSystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &mut command as *mut i32 as PVOID,
            mem::size_of::<i32>() as ULONG,
        );
        
        if status != STATUS_SUCCESS {
            return Err(format!("NtSetSystemInformation failed: 0x{:08X}", status));
        }
    }
    Ok(())
}

fn flush_modified_list() -> Result<(), String> {
    enable_privilege("SeProfileSingleProcessPrivilege")?;
    
    unsafe {
        let mut command: i32 = MEMORY_FLUSH_MODIFIED_LIST;
        let status = NtSetSystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &mut command as *mut i32 as PVOID,
            mem::size_of::<i32>() as ULONG,
        );
        
        if status != STATUS_SUCCESS {
            return Err(format!("NtSetSystemInformation failed: 0x{:08X}", status));
        }
    }
    Ok(())
}

fn flush_file_system_cache() -> Result<(), String> {
    enable_privilege("SeIncreaseQuotaPrivilege")?;
    
    unsafe {
        if SetSystemFileCacheSize(usize::MAX, usize::MAX, 0) == 0 {
            return Err(format!("SetSystemFileCacheSize failed: {}", GetLastError()));
        }
    }
    Ok(())
}

fn combine_memory_pages() -> Result<u64, String> {
    enable_privilege("SeProfileSingleProcessPrivilege")?;
    
    unsafe {
        let mut info = MemoryCombineInformationEx {
            handle: ptr::null_mut(),
            pages_combined: 0,
        };
        
        let status = NtSetSystemInformation(
            SYSTEM_COMBINE_PHYSICAL_MEMORY_INFORMATION,
            &mut info as *mut _ as PVOID,
            mem::size_of::<MemoryCombineInformationEx>() as ULONG,
        );
        
        if status != STATUS_SUCCESS {
            return Err(format!("Memory combine failed: 0x{:08X}", status));
        }
        
        Ok(info.pages_combined as u64)
    }
}

fn query_memory_lists() -> Result<SystemMemoryListInformation, String> {
    unsafe {
        let mut info = SystemMemoryListInformation::default();
        let status = NtQuerySystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &mut info as *mut _ as PVOID,
            mem::size_of::<SystemMemoryListInformation>() as ULONG,
            ptr::null_mut(),
        );
        
        if status != STATUS_SUCCESS {
            return Err(format!("NtQuerySystemInformation failed: 0x{:08X}", status));
        }
        
        Ok(info)
    }
}

fn get_memory_status() -> Result<MemoryStatusEx, String> {
    unsafe {
        let mut status = MemoryStatusEx {
            dw_length: mem::size_of::<MemoryStatusEx>() as DWORD,
            ..mem::zeroed()
        };
        
        if GlobalMemoryStatusEx(&mut status) == 0 {
            return Err(format!("GlobalMemoryStatusEx failed: {}", GetLastError()));
        }
        
        Ok(status)
    }
}

fn get_performance_info() -> Result<PerformanceInformation, String> {
    unsafe {
        let mut info: PerformanceInformation = mem::zeroed();
        info.cb = mem::size_of::<PerformanceInformation>() as DWORD;
        
        if K32GetPerformanceInfo(&mut info, info.cb) == 0 {
            return Err(format!("GetPerformanceInfo failed: {}", GetLastError()));
        }
        
        Ok(info)
    }
}
```

---

## 7. GlobalMemoryStatusEx API

### Function Signature
```c
BOOL GlobalMemoryStatusEx(LPMEMORYSTATUSEX lpBuffer);
```

### MEMORYSTATUSEX Structure
```c
typedef struct _MEMORYSTATUSEX {
    DWORD     dwLength;              // Must be set to sizeof(MEMORYSTATUSEX) before calling
    DWORD     dwMemoryLoad;          // Approximate percentage of physical memory in use (0-100)
    DWORDLONG ullTotalPhys;          // Total physical memory in bytes
    DWORDLONG ullAvailPhys;          // Available physical memory = standby + free + zero lists
    DWORDLONG ullTotalPageFile;      // Committed memory limit (system or process, whichever smaller)
    DWORDLONG ullAvailPageFile;      // Max memory current process can commit
    DWORDLONG ullTotalVirtual;       // Size of user-mode virtual address space
    DWORDLONG ullAvailVirtual;       // Unreserved+uncommitted user-mode VA space
    DWORDLONG ullAvailExtendedVirtual; // Reserved, always 0
} MEMORYSTATUSEX, *LPMEMORYSTATUSEX;
```

**Key insight:** `ullAvailPhys` = standby + free + zero lists. This is what "Available" means in Task Manager.

**DLL:** kernel32.dll  
**Header:** sysinfoapi.h (include Windows.h)  
**No special privileges needed.**

---

## 8. GetPerformanceInfo / K32GetPerformanceInfo API

### Function Signature
```c
BOOL GetPerformanceInfo(
    PPERFORMANCE_INFORMATION pPerformanceInformation,
    DWORD cb  // sizeof(PERFORMANCE_INFORMATION)
);
```

### PERFORMANCE_INFORMATION Structure
```c
typedef struct _PERFORMANCE_INFORMATION {
    DWORD  cb;                // Size of this struct (must set before calling)
    SIZE_T CommitTotal;       // Currently committed pages
    SIZE_T CommitLimit;       // Max committable pages (can grow with pagefile)
    SIZE_T CommitPeak;        // Peak committed pages since boot
    SIZE_T PhysicalTotal;     // Total physical memory in PAGES (not bytes!)
    SIZE_T PhysicalAvailable; // Available physical pages = standby + free + zero
    SIZE_T SystemCache;       // System cache pages = standby list + system working set
    SIZE_T KernelTotal;       // Paged + nonpaged kernel pool (pages)
    SIZE_T KernelPaged;       // Paged kernel pool (pages)
    SIZE_T KernelNonpaged;    // Nonpaged kernel pool (pages)
    SIZE_T PageSize;          // Page size in BYTES (typically 4096)
    DWORD  HandleCount;       // Total open handles in the system
    DWORD  ProcessCount;      // Total processes
    DWORD  ThreadCount;       // Total threads
} PERFORMANCE_INFORMATION, *PPERFORMANCE_INFORMATION;
```

**IMPORTANT:** All SIZE_T memory values are in **pages**, not bytes. Multiply by `PageSize` to get bytes.

**DLL:** kernel32.dll (K32GetPerformanceInfo) or psapi.dll (GetPerformanceInfo)  
**Header:** psapi.h  
**No special privileges needed.**

---

## 9. Undocumented but Widely-Used APIs

### 9.1 NtSetSystemInformation / NtQuerySystemInformation

Not officially documented by Microsoft, but the most important undocumented APIs for memory management. Source of truth: [System Informer phnt headers](https://github.com/winsiderss/systeminformer/blob/master/phnt/include/ntexapi.h).

Key `SYSTEM_INFORMATION_CLASS` values for memory:

| Value | Name                                      | Use                                             |
| ----- | ----------------------------------------- | ----------------------------------------------- |
| 0x05  | `SystemProcessInformation`                | Process/thread info including working set sizes |
| 0x15  | `SystemFileCacheInformation`              | File cache stats                                |
| 0x2D  | `SystemPageFileInformation`               | Pagefile information                            |
| 0x50  | `SystemMemoryListInformation`             | **Memory list query & commands**                |
| 0x52  | `SystemFileCacheInformationEx`            | Extended file cache info                        |
| 0x66  | `SystemSuperfetchInformation`             | SuperFetch/SysMain info                         |
| 0x76  | `SystemMemoryUsageInformation`            | Memory usage info                               |
| 0x82  | `SystemCombinePhysicalMemoryInformation`  | Memory page combining/dedup                     |
| 0x9B  | `SystemRegistryReconciliationInformation` | Registry hive cache flush (155)                 |
| 0xB4  | `SystemStoreInformation`                  | Memory compression store                        |
| 0xC7  | `SystemMemoryPartitionInformation`        | Memory partitions (Win10+)                      |

### 9.2 NtQueryVirtualMemory (Undocumented Classes)

```c
typedef enum _MEMORY_INFORMATION_CLASS {
    MemoryBasicInformation = 0,          // Documented - VirtualQuery
    MemoryWorkingSetInformation = 1,     // Per-page working set info
    MemoryMappedFilenameInformation = 2, // Mapped file name
    MemoryRegionInformation = 3,         // Region info
    MemoryWorkingSetExInformation = 4,   // Extended working set info
    MemorySharedCommitInformation = 5,   // Shared commit info
    MemoryImageInformation = 6,          // Image info
    MemoryRegionInformationEx = 7,       // Extended region info  
    MemoryPrivilegedBasicInformation = 8,
    MemoryEnclaveImageInformation = 9,
    MemoryBasicInformationCapped = 10,
    MemoryPhysicalContiguityInformation = 11,
} MEMORY_INFORMATION_CLASS;
```

### 9.3 SystemFileCacheInformation (Query File Cache Stats)

```c
#define SystemFileCacheInformation 0x15  // 21

typedef struct _SYSTEM_FILECACHE_INFORMATION {
    SIZE_T CurrentSize;           // Current cache size in bytes
    SIZE_T PeakSize;              // Peak cache size
    ULONG PageFaultCount;         // Cache page faults
    SIZE_T MinimumWorkingSet;     // Min working set size
    SIZE_T MaximumWorkingSet;     // Max working set size
    SIZE_T CurrentSizeIncludingTransitionInPages;
    SIZE_T PeakSizeIncludingTransitionInPages;
    ULONG TransitionRePurposeCount;
    ULONG Flags;
} SYSTEM_FILECACHE_INFORMATION, *PSYSTEM_FILECACHE_INFORMATION;
```

### 9.4 SuperFetch / SysMain Information

```c
#define SystemSuperfetchInformation 0x66  // 102

// The SuperFetch info class is complex and version-dependent.
// Key sub-commands for memory prefetch management:
typedef enum _SUPERFETCH_INFORMATION_CLASS {
    SuperfetchRetrieveTrace = 1,
    SuperfetchSystemParameters = 2,
    SuperfetchLogEvent = 3,
    SuperfetchGenerateTrace = 4,
    SuperfetchPrefetch = 5,
    SuperfetchResPrefetchParameters = 6,
    SuperfetchBootStoreFile = 7,
    SuperfetchBootTrace = 8,
    SuperfetchScenarioTrace = 9,
    SuperfetchExeBitmapInfo = 10,
    SuperfetchMemoryListQuery = 11,
    SuperfetchMemoryRangesQuery = 12,
    SuperfetchTracingControl = 13,
    SuperfetchTrimWhileAgingControl = 14,
    SuperfetchInformationMax = 15,
} SUPERFETCH_INFORMATION_CLASS;
```

### 9.5 Memory Partition APIs (Windows 10 1607+)

```c
// Create memory partitions for isolation:
NTSTATUS NtCreatePartition(
    HANDLE ParentPartitionHandle,
    PHANDLE PartitionHandle,
    ACCESS_MASK DesiredAccess,
    POBJECT_ATTRIBUTES ObjectAttributes,
    ULONG PreferredNode  
);

NTSTATUS NtManagePartition(
    HANDLE TargetHandle,
    HANDLE SourceHandle,
    MEMORY_PARTITION_INFORMATION_CLASS PartitionInformationClass,
    PVOID PartitionInformation,
    ULONG PartitionInformationLength
);
```

---

## 10. Complete SYSTEM_INFORMATION_CLASS Enum (Memory-Related Subset)

From System Informer's phnt headers:

```c
typedef enum _SYSTEM_INFORMATION_CLASS {
    // ... (many entries) ...
    SystemPerformanceInformation             = 2,    // CPU, I/O stats
    SystemProcessInformation                 = 5,    // Process list
    SystemFileCacheInformation               = 21,   // 0x15 - File cache
    SystemPageFileInformation                = 18,   // 0x12 - Pagefile
    SystemMemoryListInformation              = 80,   // 0x50 - Memory lists ★
    SystemFileCacheInformationEx             = 82,   // 0x52
    SystemSuperfetchInformation              = 79,   // 0x4F
    SystemMemoryUsageInformation             = 118,  // 0x76
    SystemCombinePhysicalMemoryInformation   = 130,  // 0x82 - Memory combining ★
    SystemStoreInformation                   = 180,  // 0xB4 - Compression
    SystemMemoryPartitionInformation         = 199,  // 0xC7 - Partitions
    // ...
} SYSTEM_INFORMATION_CLASS;
```

---

## 11. NTSTATUS Return Codes

Common return codes when calling these APIs:

| Code         | Name                          | Meaning                        |
| ------------ | ----------------------------- | ------------------------------ |
| `0x00000000` | `STATUS_SUCCESS`              | Operation succeeded            |
| `0xC0000022` | `STATUS_ACCESS_DENIED`        | Missing required privilege     |
| `0xC0000004` | `STATUS_INFO_LENGTH_MISMATCH` | Buffer too small               |
| `0xC0000003` | `STATUS_INVALID_INFO_CLASS`   | Invalid SystemInformationClass |
| `0xC000000D` | `STATUS_INVALID_PARAMETER`    | Invalid command value          |
| `0xC0000008` | `STATUS_INVALID_HANDLE`       | Invalid handle                 |
| `0xC0000001` | `STATUS_UNSUCCESSFUL`         | Generic failure                |

---

## 12. Architecture Summary for a Complete RAM Cleaner

A comprehensive RAM cleaner should implement these operations:

### Operations & Their APIs

| Operation                      | API                                   | Privilege Required                | Aggressiveness |
| ------------------------------ | ------------------------------------- | --------------------------------- | -------------- |
| **Query memory stats**         | `GlobalMemoryStatusEx`                | None                              | Read-only      |
| **Query detailed page lists**  | `NtQuerySystemInformation(0x50)`      | None                              | Read-only      |
| **Query performance info**     | `K32GetPerformanceInfo`               | None                              | Read-only      |
| **Empty all working sets**     | `NtSetSystemInformation(0x50, cmd=2)` | `SeProfileSingleProcessPrivilege` | Medium         |
| **Flush modified pages**       | `NtSetSystemInformation(0x50, cmd=3)` | `SeProfileSingleProcessPrivilege` | Low            |
| **Purge low-priority standby** | `NtSetSystemInformation(0x50, cmd=5)` | `SeProfileSingleProcessPrivilege` | Medium         |
| **Purge ALL standby**          | `NtSetSystemInformation(0x50, cmd=4)` | `SeProfileSingleProcessPrivilege` | **HIGH**       |
| **Flush file system cache**    | `SetSystemFileCacheSize(-1, -1, 0)`   | `SeIncreaseQuotaPrivilege`        | High           |
| **Flush registry cache**       | `NtSetSystemInformation(0x9B)`        | `SeProfileSingleProcessPrivilege` | Low            |
| **Combine duplicate pages**    | `NtSetSystemInformation(0x82)`        | `SeProfileSingleProcessPrivilege` | Low            |
| **Trim single process**        | `EmptyWorkingSet(hProcess)`           | `PROCESS_SET_QUOTA`               | Per-process    |

### Recommended Cleaning Sequence (Least to Most Aggressive)

1. **Flush modified list** (cmd=3) - writes dirty pages to disk, converts to standby
2. **Combine memory pages** (0x82) - deduplicates identical pages
3. **Flush file system cache** - trims the filesystem cache working set
4. **Purge low-priority standby** (cmd=5) - frees low-value cache pages
5. **Empty working sets** (cmd=2) - trims all processes
6. **Purge ALL standby** (cmd=4) - nuclear option, destroys all cached data

### Monitoring Loop Strategy

```
loop {
    let mem = query_memory_lists();
    let total_standby: u64 = mem.page_count_by_priority.iter().sum();
    let free_available = mem.free_page_count + mem.zero_page_count;
    
    if free_available < threshold {
        // Graduated response:
        if free_available < critical_threshold {
            purge_all_standby();      // Emergency
        } else if free_available < warning_threshold {
            purge_low_priority();     // Moderate
            flush_modified();
        } else {
            combine_memory();         // Gentle
        }
    }
    
    sleep(check_interval);
}
```

---

## Sources

- **System Informer (Process Hacker) phnt headers:** https://github.com/winsiderss/systeminformer/blob/master/phnt/include/ntexapi.h
- **windows-sys crate (official Microsoft Rust bindings):** https://crates.io/crates/windows-sys
- **Standby RAM Cleaner Service (C reference):** https://github.com/theKorzh/Standby-RAM-Cleaner-service
- **Microsoft Docs - GlobalMemoryStatusEx:** https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/nf-sysinfoapi-globalmemorystatusex
- **Microsoft Docs - PERFORMANCE_INFORMATION:** https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-performance_information
- **Microsoft Docs - SetSystemFileCacheSize:** https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-setsystemfilecachesize
- **Windows Internals, 7th Edition** (Mark Russinovich, Alex Ionescu, David Solomon) - Chapters on Memory Management
