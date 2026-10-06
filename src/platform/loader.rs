//! DLL loading restricted to the System32 folder.
//!
//! The app always runs elevated and usually sits in a user-writable folder
//! such as Downloads. Windows looks for a DLL in the exe's own folder before
//! System32, so a DLL with a system name dropped next to the exe would run
//! with admin rights. Load-time imports are restricted by the
//! `/DEPENDENTLOADFLAG` linker flag (see `build.rs`); this module covers
//! DLLs loaded at run time.

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleHandleW, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW, SetDefaultDllDirectories,
};

use super::wide::to_wide;

/// Make every later run-time DLL load in this process search System32 only.
///
/// Call this first thing at startup, before any library loads a DLL by name.
/// A failure is ignored: the process then keeps the default search order.
pub fn restrict_dll_search_to_system32() {
    // SAFETY: SetDefaultDllDirectories takes a flags value and touches no
    // memory owned by the caller.
    unsafe {
        SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32);
    }
}

/// Handle to a system DLL: the already-loaded module if there is one,
/// otherwise a fresh load from System32. Returns `None` if neither works.
///
/// A module loaded here is never freed; callers use it for the life of the
/// process.
#[must_use]
pub fn system_module(name: &str) -> Option<HMODULE> {
    let wide = to_wide(name);
    // SAFETY: `wide` is a null-terminated UTF-16 string that outlives the
    // call. GetModuleHandleW does not add a reference.
    let loaded = unsafe { GetModuleHandleW(wide.as_ptr()) };
    if !loaded.is_null() {
        return Some(loaded);
    }
    // SAFETY: as above; the reserved file handle must be null, and
    // LOAD_LIBRARY_SEARCH_SYSTEM32 limits the search to System32.
    let module = unsafe {
        LoadLibraryExW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    };
    (!module.is_null()).then_some(module)
}
