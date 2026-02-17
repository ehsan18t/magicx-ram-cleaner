//! # `MagicX` RAM Cleaner — Console Utilities
//!
//! Windows console platform utilities: ANSI virtual terminal processing,
//! standalone console detection, and pause-before-exit for double-click launches.
//!
//! These are isolated from business logic so that platform-specific console
//! quirks don't leak into the application layer.

use colored::Colorize;

/// Detect whether this process owns its console (i.e. launched by double-clicking).
///
/// Uses `GetConsoleProcessList` — if only our process is attached, the console
/// was created just for us and will vanish when we exit.
pub fn is_standalone_console() -> bool {
    use windows_sys::Win32::System::Console::GetConsoleProcessList;
    let mut pids = [0u32; 2];
    // SAFETY: `GetConsoleProcessList` is a standard Win32 console API.
    // We pass a valid stack-allocated buffer and its length.
    let count = unsafe { GetConsoleProcessList(pids.as_mut_ptr(), 2) };
    count <= 1
}

/// Wait for the user to press Enter before the console window closes.
pub fn pause_before_exit() {
    use std::io::Write;
    eprint!("\n  {}", "Press Enter to exit...".dimmed());
    drop(std::io::stderr().flush());
    drop(std::io::stdin().read_line(&mut String::new()));
}

/// Enable ANSI virtual terminal processing on Windows consoles.
pub fn enable_ansi_colors() {
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE,
        SetConsoleMode,
    };
    // SAFETY: GetStdHandle/GetConsoleMode/SetConsoleMode are standard Win32 calls
    // with no preconditions beyond a valid handle. STD_OUTPUT_HANDLE is always valid.
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut mode: u32 = 0;
        if GetConsoleMode(handle, &raw mut mode) != 0 {
            let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}
