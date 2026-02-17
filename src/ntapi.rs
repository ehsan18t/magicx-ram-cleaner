//! # MagicX RAM Cleaner — NT Native API Bindings
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

/// Memory list commands passed to NtSetSystemInformation(SystemMemoryListInformation).
///
/// These are the core operations that EmptyStandbyList uses.
/// MagicX supports ALL of them with finer control.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MemoryListCommand {
    /// Capture PTE accessed bits (diagnostic only).
    CaptureAccessedBits = 0,
    /// Capture and reset PTE accessed bits (diagnostic).
    CaptureAndResetAccessedBits = 1,
    /// Empty working sets of ALL processes system-wide (kernel-level).
    /// More powerful than per-process EmptyWorkingSet — hits all processes
    /// including ones you can't open with PROCESS_SET_QUOTA.
    EmptyWorkingSets = 2,
    /// Flush modified page list — write dirty pages to disk/pagefile.
    /// Must be done BEFORE purging standby for maximum effect.
    FlushModifiedList = 3,
    /// Purge ALL standby pages (priorities 0–7).
    /// This is what EmptyStandbyList's "standbylist" command does.
    PurgeStandbyList = 4,
    /// Purge only low-priority (priority 0) standby pages.
    /// Gentler — preserves high-priority cached data.
    PurgeLowPriorityStandbyList = 5,
}

#[link(name = "ntdll")]
unsafe extern "system" {
    /// Set system information via the NT kernel.
    /// For memory commands: class = 80 (SystemMemoryListInformation),
    /// buffer points to a SYSTEM_MEMORY_LIST_COMMAND (i32),
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

/// Safe wrapper around NtSetSystemInformation for memory commands.
pub fn execute_memory_command(command: MemoryListCommand) -> Result<(), NtStatus> {
    let mut cmd = command as i32;
    let status = unsafe {
        NtSetSystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &mut cmd as *mut i32 as *mut std::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        )
    };
    if status == STATUS_SUCCESS {
        Ok(())
    } else {
        Err(status)
    }
}

/// Safe wrapper around NtQuerySystemInformation.
pub fn nt_query_system_information(
    class: u32,
    buffer: *mut std::ffi::c_void,
    length: u32,
    return_length: *mut u32,
) -> NtStatus {
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

/// Translate an NTSTATUS code to a human-readable message.
pub fn ntstatus_message(status: NtStatus) -> &'static str {
    match status {
        STATUS_SUCCESS => "SUCCESS",
        STATUS_PENDING => "STATUS_PENDING",
        STATUS_ACCESS_DENIED => {
            "STATUS_ACCESS_DENIED - Run as Administrator with required privileges"
        }
        STATUS_INVALID_HANDLE => "STATUS_INVALID_HANDLE",
        STATUS_INVALID_PARAMETER => "STATUS_INVALID_PARAMETER",
        STATUS_NOT_IMPLEMENTED => {
            "STATUS_NOT_IMPLEMENTED - This command may not be supported on your Windows version"
        }
        STATUS_UNSUCCESSFUL => "STATUS_UNSUCCESSFUL",
        STATUS_PRIVILEGE_NOT_HELD => {
            "STATUS_PRIVILEGE_NOT_HELD - Enable SeProfileSingleProcessPrivilege"
        }
        STATUS_INFO_LENGTH_MISMATCH => {
            "STATUS_INFO_LENGTH_MISMATCH - Buffer too small or struct size mismatch"
        }
        _ => "Unknown NTSTATUS code",
    }
}
