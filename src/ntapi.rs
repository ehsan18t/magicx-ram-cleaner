//! # `MagicX` RAM Cleaner — NT Native API Bindings
//!
//! Raw FFI declarations for undocumented/semi-documented NT kernel APIs
//! used for advanced memory management. These are loaded directly from ntdll.dll.

/// NTSTATUS type alias.
pub type NtStatus = i32;

// NTSTATUS success
pub const STATUS_SUCCESS: NtStatus = 0;

// SystemMemoryListInformation information class for NtSetSystemInformation / NtQuerySystemInformation
pub const SYSTEM_MEMORY_LIST_INFORMATION: u32 = 80; // 0x50

// SystemCombinePhysicalMemoryInformation information class for NtSetSystemInformation
pub const SYSTEM_COMBINE_PHYSICAL_MEMORY_INFORMATION: u32 = 130; // 0x82

// SystemRegistryReconciliationInformation information class for NtSetSystemInformation
// Flushes the registry cache (dirty hive pages) to disk.
pub const SYSTEM_REGISTRY_RECONCILIATION_INFORMATION: u32 = 155; // 0x9B

// SystemFileCacheInformation information class for NtQuerySystemInformation
// Returns current and peak file system cache sizes.
pub const SYSTEM_FILE_CACHE_INFORMATION: u32 = 21; // 0x15

/// File system cache information returned by
/// `NtQuerySystemInformation(SystemFileCacheInformation)`.
///
/// Maps to `SYSTEM_FILECACHE_INFORMATION`. Only the base fields are included;
/// newer Windows 10 builds may return additional fields (transition pages, flags)
/// which are safely ignored by querying with this smaller struct.
#[repr(C)]
pub struct SystemFileCacheInfo {
    /// Current size of the system file cache working set (bytes).
    pub current_size: usize,
    /// Peak size of the system file cache working set since boot (bytes).
    pub peak_size: usize,
    /// Total page faults incurred by the file cache.
    pub page_fault_count: u32,
    // 4 bytes padding on x86-64 (u32 before usize alignment)
    /// Minimum configured working set for the file cache (bytes).
    pub minimum_working_set: usize,
    /// Maximum configured working set for the file cache (bytes).
    pub maximum_working_set: usize,
}

/// Memory list commands passed to NtSetSystemInformation(SystemMemoryListInformation).
///
/// These are the core operations that `EmptyStandbyList` uses.
/// `MagicX` supports ALL of them with finer control.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MemoryListCommand {
    /// Capture PTE accessed bits (diagnostic only).
    CaptureAccessedBits = 0,
    /// Capture and reset PTE accessed bits (diagnostic).
    CaptureAndResetAccessedBits = 1,
    /// Empty working sets of ALL processes system-wide (kernel-level).
    /// More powerful than per-process `EmptyWorkingSet` — hits all processes
    /// including ones you can't open with `PROCESS_SET_QUOTA`.
    EmptyWorkingSets = 2,
    /// Flush modified page list — write dirty pages to disk/pagefile.
    /// Must be done BEFORE purging standby for maximum effect.
    FlushModifiedList = 3,
    /// Purge ALL standby pages (priorities 0–7).
    /// This is what `EmptyStandbyList`'s "standbylist" command does.
    PurgeStandbyList = 4,
    /// Purge only low-priority (priority 0) standby pages.
    /// Gentler — preserves high-priority cached data.
    PurgeLowPriorityStandbyList = 5,
}

#[link(name = "ntdll")]
unsafe extern "system" {
    /// Set system information via the NT kernel.
    /// For memory commands: class = 80 (`SystemMemoryListInformation`),
    /// buffer points to a `SYSTEM_MEMORY_LIST_COMMAND` (i32),
    /// length = 4.
    pub fn NtSetSystemInformation(
        system_information_class: u32,
        system_information: *mut std::ffi::c_void,
        system_information_length: u32,
    ) -> NtStatus;

    /// Query system information from the NT kernel.
    /// Used to get detailed memory list stats (page counts per list).
    pub fn NtQuerySystemInformation(
        system_information_class: u32,
        system_information: *mut std::ffi::c_void,
        system_information_length: u32,
        return_length: *mut u32,
    ) -> NtStatus;
}

/// Safe wrapper around `NtSetSystemInformation` for memory commands.
pub fn execute_memory_command(command: MemoryListCommand) -> Result<(), NtStatus> {
    let mut cmd = command as i32;
    // SAFETY: `cmd` is a valid i32 on the stack. NtSetSystemInformation reads
    // exactly `size_of::<i32>()` bytes from the pointer.
    let status = unsafe {
        NtSetSystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            (&raw mut cmd).cast::<std::ffi::c_void>(),
            std::mem::size_of::<i32>() as u32,
        )
    };
    if status == STATUS_SUCCESS {
        Ok(())
    } else {
        Err(status)
    }
}

/// FFI struct for `NtSetSystemInformation(SystemCombinePhysicalMemoryInformation)`.
///
/// FFI struct for `NtSetSystemInformation(SystemCombinePhysicalMemoryInformation)`.
///
/// Maps to `MEMORY_COMBINE_INFORMATION_EX` — the extended variant that includes
/// the `Flags` field. This is the struct Windows 10+ expects and allows passing
/// `MEMORY_COMBINE_FLAGS_COMMON_PAGES_ONLY` (0x4) to restrict combining to
/// common-only pages.
#[repr(C)]
struct CombinePhysicalMemoryInfoEx {
    /// Process handle — 0 for system-wide scan.
    handle: usize,
    /// Output: number of pages combined.
    pages_combined: usize,
    /// Combination flags (e.g. `MEMORY_COMBINE_FLAGS_COMMON_PAGES_ONLY`).
    flags: u32,
}

/// Execute a physical memory combine operation via the NT kernel.
///
/// Returns the number of pages combined on success.
pub fn execute_combine_memory() -> Result<usize, NtStatus> {
    let mut info = CombinePhysicalMemoryInfoEx {
        handle: 0,
        pages_combined: 0,
        flags: 0, // 0 = combine all duplicates (default behaviour)
    };

    // SAFETY: `info` is a valid repr(C) struct on the stack with correct size.
    // NtSetSystemInformation reads/writes exactly `size_of::<CombinePhysicalMemoryInfoEx>()` bytes.
    let status = unsafe {
        NtSetSystemInformation(
            SYSTEM_COMBINE_PHYSICAL_MEMORY_INFORMATION,
            (&raw mut info).cast::<std::ffi::c_void>(),
            std::mem::size_of::<CombinePhysicalMemoryInfoEx>() as u32,
        )
    };

    if status == STATUS_SUCCESS {
        Ok(info.pages_combined)
    } else {
        Err(status)
    }
}

/// Flush the Windows registry cache to disk.
///
/// Calls `NtSetSystemInformation(SystemRegistryReconciliationInformation)` with
/// a null buffer and zero length. This forces all dirty registry hive pages to
/// be written to disk, freeing the modified memory they occupied.
pub fn execute_registry_flush() -> Result<(), NtStatus> {
    // SAFETY: SystemRegistryReconciliationInformation takes no input buffer.
    // Passing null pointer and zero length is the documented usage (see Mem Reduct,
    // SystemInformer/phnt headers).
    let status = unsafe {
        NtSetSystemInformation(
            SYSTEM_REGISTRY_RECONCILIATION_INFORMATION,
            std::ptr::null_mut(),
            0,
        )
    };
    if status == STATUS_SUCCESS {
        Ok(())
    } else {
        Err(status)
    }
}

/// Safe wrapper around `NtQuerySystemInformation`.
pub fn nt_query_system_information(
    class: u32,
    buffer: *mut std::ffi::c_void,
    length: u32,
    return_length: *mut u32,
) -> NtStatus {
    // SAFETY: Caller provides valid buffer/length. Forwarded directly to NT API.
    unsafe { NtQuerySystemInformation(class, buffer, length, return_length) }
}

// NTSTATUS constants for match arms
const STATUS_PENDING: NtStatus = 0x0000_0103_u32 as i32;
const STATUS_ACCESS_DENIED: NtStatus = 0xC000_0022_u32 as i32;
const STATUS_INVALID_HANDLE: NtStatus = 0xC000_0008_u32 as i32;
const STATUS_INVALID_PARAMETER: NtStatus = 0xC000_000D_u32 as i32;
const STATUS_NOT_IMPLEMENTED: NtStatus = 0xC000_0002_u32 as i32;
const STATUS_UNSUCCESSFUL: NtStatus = 0xC000_0001_u32 as i32;
const STATUS_PRIVILEGE_NOT_HELD: NtStatus = 0xC000_0061_u32 as i32;
pub const STATUS_INFO_LENGTH_MISMATCH: NtStatus = 0xC000_0004_u32 as i32;
const STATUS_BUFFER_TOO_SMALL: NtStatus = 0xC000_0023_u32 as i32;
const STATUS_INSUFFICIENT_RESOURCES: NtStatus = 0xC000_009A_u32 as i32;
const STATUS_NOT_SUPPORTED: NtStatus = 0xC000_00BB_u32 as i32;
const STATUS_INVALID_DEVICE_REQUEST: NtStatus = 0xC000_0010_u32 as i32;

/// Translate an NTSTATUS code to a human-readable message.
pub const fn ntstatus_message(status: NtStatus) -> &'static str {
    match status {
        STATUS_SUCCESS => "SUCCESS",
        STATUS_PENDING => "STATUS_PENDING",
        STATUS_ACCESS_DENIED => {
            "STATUS_ACCESS_DENIED - Run as Administrator with required privileges"
        }
        STATUS_INVALID_HANDLE => "STATUS_INVALID_HANDLE - Invalid handle passed to NT API",
        STATUS_INVALID_PARAMETER => "STATUS_INVALID_PARAMETER - Invalid command or parameter value",
        STATUS_NOT_IMPLEMENTED => {
            "STATUS_NOT_IMPLEMENTED - This command may not be supported on your Windows version"
        }
        STATUS_UNSUCCESSFUL => {
            "STATUS_UNSUCCESSFUL - The operation failed (the kernel could not complete it)"
        }
        STATUS_PRIVILEGE_NOT_HELD => {
            "STATUS_PRIVILEGE_NOT_HELD - Enable SeProfileSingleProcessPrivilege"
        }
        STATUS_INFO_LENGTH_MISMATCH => {
            "STATUS_INFO_LENGTH_MISMATCH - Buffer too small or struct size mismatch"
        }
        STATUS_BUFFER_TOO_SMALL => {
            "STATUS_BUFFER_TOO_SMALL - Provided buffer is too small for the requested data"
        }
        STATUS_INSUFFICIENT_RESOURCES => {
            "STATUS_INSUFFICIENT_RESOURCES - System has insufficient resources to complete the call"
        }
        STATUS_NOT_SUPPORTED => {
            "STATUS_NOT_SUPPORTED - This operation is not supported on your Windows version"
        }
        STATUS_INVALID_DEVICE_REQUEST => {
            "STATUS_INVALID_DEVICE_REQUEST - Invalid request for this device/subsystem"
        }
        _ => "Unknown NTSTATUS code (check Microsoft documentation for this code)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntstatus_success() {
        assert_eq!(ntstatus_message(STATUS_SUCCESS), "SUCCESS");
    }

    #[test]
    fn ntstatus_known_errors() {
        assert!(ntstatus_message(STATUS_ACCESS_DENIED).contains("ACCESS_DENIED"));
        assert!(ntstatus_message(STATUS_PRIVILEGE_NOT_HELD).contains("PRIVILEGE_NOT_HELD"));
        assert!(ntstatus_message(STATUS_NOT_IMPLEMENTED).contains("NOT_IMPLEMENTED"));
        assert!(ntstatus_message(STATUS_INFO_LENGTH_MISMATCH).contains("LENGTH_MISMATCH"));
        assert!(ntstatus_message(STATUS_BUFFER_TOO_SMALL).contains("BUFFER_TOO_SMALL"));
        assert!(ntstatus_message(STATUS_INSUFFICIENT_RESOURCES).contains("INSUFFICIENT_RESOURCES"));
        assert!(ntstatus_message(STATUS_NOT_SUPPORTED).contains("NOT_SUPPORTED"));
    }

    #[test]
    fn ntstatus_unknown_code() {
        let msg = ntstatus_message(0x1234_5678_u32 as i32);
        assert!(
            msg.contains("Unknown"),
            "unknown code should return 'Unknown' message"
        );
    }

    #[test]
    fn memory_list_command_values() {
        // Verify that the enum discriminants match the Windows API constants
        assert_eq!(MemoryListCommand::CaptureAccessedBits as i32, 0);
        assert_eq!(MemoryListCommand::CaptureAndResetAccessedBits as i32, 1);
        assert_eq!(MemoryListCommand::EmptyWorkingSets as i32, 2);
        assert_eq!(MemoryListCommand::FlushModifiedList as i32, 3);
        assert_eq!(MemoryListCommand::PurgeStandbyList as i32, 4);
        assert_eq!(MemoryListCommand::PurgeLowPriorityStandbyList as i32, 5);
    }

    #[test]
    fn system_info_class_constants() {
        assert_eq!(SYSTEM_MEMORY_LIST_INFORMATION, 80);
        assert_eq!(SYSTEM_COMBINE_PHYSICAL_MEMORY_INFORMATION, 130);
        assert_eq!(SYSTEM_REGISTRY_RECONCILIATION_INFORMATION, 155);
        assert_eq!(SYSTEM_FILE_CACHE_INFORMATION, 21);
    }
}
