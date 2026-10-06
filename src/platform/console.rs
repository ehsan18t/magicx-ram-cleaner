//! Console lifecycle for the CLI: sharing the parent terminal, keeping
//! redirected handles, allocating a console on demand, ANSI colours and the
//! pause-before-exit prompt.
//!
//! The binary is a console program (`SUBSYSTEM:CONSOLE`), so cmd and
//! `PowerShell` wait for it and see its exit code. Its manifest sets
//! `consoleAllocationPolicy` to `detached`, so on Windows 11 24H2 and later a
//! launch without a parent console (Explorer, context menu, Task Scheduler)
//! gets no console window at all. Older Windows ignores that setting and
//! creates one; [`release_private_console`] frees it immediately so those
//! launches behave the same apart from a brief flash.

use super::process::parent_process_name;

use colored::Colorize;

/// How the process was launched.
///
/// Returned by [`setup_cli_console`] so the caller can decide whether
/// to pause before exit (standalone) or return immediately (terminal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleMode {
    /// Sharing a console with a parent shell (launched from cmd / `PowerShell`).
    Terminal,
    /// No console, but the parent handed us redirected standard handles
    /// (pipes or files), e.g. a scheduled task, a service wrapper or a script
    /// host capturing output. Output goes to those handles and nothing waits
    /// for the user.
    Redirected,
    /// Own console allocated for a launch someone is watching: from Explorer
    /// (double-clicked the `.exe`, a shortcut or the Run dialog), or from a
    /// shell whose console the elevated process could not share (a
    /// non-elevated terminal starting this admin-only exe). The window would
    /// vanish on exit, so the caller should [`pause_before_exit`].
    Standalone,
    /// Own console allocated for any other launcher without a console, such
    /// as Task Scheduler. Nobody may be there to press Enter (or the session
    /// may not even be interactive), so the caller must not pause.
    Allocated,
}

/// Attach to the parent terminal or allocate a fresh console for CLI mode.
///
/// This function:
/// 0. Returns [`ConsoleMode::Terminal`] straight away when the process already
///    shares its parent shell's console; Windows has then set up the standard
///    handles (including any redirection) itself. A private console created
///    by older Windows is released first (see [`release_private_console`]).
/// 1. Records which standard handles the parent passed in (redirected to a
///    file or pipe). Those are always kept, so `status --json > out.json`
///    and pipelines work.
/// 2. Tries `AttachConsole(ATTACH_PARENT_PROCESS)` - succeeds when launched
///    from cmd / `PowerShell` / Windows Terminal.
/// 3. Without a parent console, keeps the inherited handles if there are any
///    ([`ConsoleMode::Redirected`]); otherwise allocates a brand-new console,
///    reported as [`ConsoleMode::Standalone`] only when Explorer or a shell
///    launched us, so someone is watching the window.
/// 4. Points every standard handle that was not inherited at the console.
#[must_use]
pub fn setup_cli_console() -> ConsoleMode {
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE,
    };

    release_private_console();
    if shares_parent_console() {
        return ConsoleMode::Terminal;
    }

    let inherited_in = inherited_std_handle(STD_INPUT_HANDLE);
    let inherited_out = inherited_std_handle(STD_OUTPUT_HANDLE);
    let inherited_err = inherited_std_handle(STD_ERROR_HANDLE);

    // SAFETY: AttachConsole is a standard Win32 call. ATTACH_PARENT_PROCESS
    // tells Windows to attach to the console of the process that launched us.
    // Returns non-zero on success (we were launched from a terminal).
    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0;

    let mode = if attached {
        ConsoleMode::Terminal
    } else if inherited_out.is_some() || inherited_err.is_some() {
        // Launched without a console but with captured output: stay
        // console-less so no window pops up and nothing blocks on input.
        ConsoleMode::Redirected
    } else {
        // SAFETY: AllocConsole is a standard Win32 call with no preconditions.
        unsafe {
            AllocConsole();
        }
        if launched_interactively() {
            ConsoleMode::Standalone
        } else {
            ConsoleMode::Allocated
        }
    };

    if mode != ConsoleMode::Redirected {
        bind_std_handle(STD_INPUT_HANDLE, inherited_in, b"CONIN$\0");
        bind_std_handle(STD_OUTPUT_HANDLE, inherited_out, b"CONOUT$\0");
        bind_std_handle(STD_ERROR_HANDLE, inherited_err, b"CONOUT$\0");
    }
    mode
}

/// Number of processes attached to this process's console (0 = no console).
fn console_process_count() -> u32 {
    use windows_sys::Win32::System::Console::GetConsoleProcessList;

    let mut pids = [0u32; 2];
    // SAFETY: `pids` is a writable buffer of the stated length. The call
    // returns the total count even when the buffer is too small, and 0 when
    // the process has no console.
    unsafe { GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32) }
}

/// Whether this process shares its console with a parent (started from a
/// terminal). The shell is then waiting for us to exit.
#[must_use]
pub fn shares_parent_console() -> bool {
    console_process_count() >= 2
}

/// Free a console that Windows created just for this process.
///
/// Windows before 11 24H2 ignores the manifest's `detached` console policy
/// and gives every launch without a parent console (Explorer, context menu,
/// Task Scheduler) its own console window. Releasing it right away makes
/// those launches behave like 24H2 apart from a brief flash. A console shared
/// with a parent shell is left alone.
pub fn release_private_console() {
    use windows_sys::Win32::System::Console::{
        FreeConsole, GetConsoleMode, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE, SetStdHandle,
    };

    if console_process_count() != 1 {
        return;
    }
    // Forget standard handles that belong to the console being freed, so
    // later code does not mistake them for handles inherited from a parent.
    for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        let mut mode = 0u32;
        // SAFETY: GetStdHandle/GetConsoleMode only inspect the handle;
        // SetStdHandle stores a null handle.
        unsafe {
            if GetConsoleMode(GetStdHandle(which), &raw mut mode) != 0 {
                SetStdHandle(which, std::ptr::null_mut());
            }
        }
    }
    // SAFETY: FreeConsole detaches this process from its console.
    unsafe {
        FreeConsole();
    }
}

/// Relaunch this executable with the same arguments as a detached process
/// (no console) and return whether the new process started.
///
/// Used when the GUI is started from a terminal: as a console program, the
/// shell would otherwise stay blocked until the window is closed.
#[must_use]
pub fn relaunch_detached() -> bool {
    use std::os::windows::process::CommandExt;

    /// `DETACHED_PROCESS`: the new process gets no console.
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .creation_flags(DETACHED_PROCESS)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// Whether the parent process is Explorer or a shell, i.e. the user started
/// us (double-click, shortcut, Run dialog, a terminal) and is watching.
fn launched_interactively() -> bool {
    /// Launchers with a person in front of them. A shell lands here when it
    /// is not elevated: UAC starts this admin-only exe outside its console.
    const INTERACTIVE: &[&str] = &[
        "explorer.exe",
        "cmd.exe",
        "powershell.exe",
        "pwsh.exe",
        "windowsterminal.exe",
    ];
    parent_process_name().is_some_and(|name| {
        INTERACTIVE
            .iter()
            .any(|known| name.eq_ignore_ascii_case(known))
    })
}

/// Return the standard handle `which` if the parent passed in a usable one
/// (a file, pipe or console), or `None` if it is missing.
fn inherited_std_handle(
    which: windows_sys::Win32::System::Console::STD_HANDLE,
) -> Option<windows_sys::Win32::Foundation::HANDLE> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{FILE_TYPE_UNKNOWN, GetFileType};
    use windows_sys::Win32::System::Console::GetStdHandle;

    // SAFETY: GetStdHandle has no preconditions and returns null or
    // INVALID_HANDLE_VALUE when there is no handle.
    let handle = unsafe { GetStdHandle(which) };
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return None;
    }
    // SAFETY: GetFileType only inspects the handle; an invalid handle yields
    // FILE_TYPE_UNKNOWN.
    (unsafe { GetFileType(handle) } != FILE_TYPE_UNKNOWN).then_some(handle)
}

/// Set standard handle `which` to `inherited` if the parent provided one,
/// otherwise to the console device `device` (`CONIN$` or `CONOUT$`).
///
/// Re-applying the inherited handle matters because attaching to a console
/// may replace the standard handles with console ones.
fn bind_std_handle(
    which: windows_sys::Win32::System::Console::STD_HANDLE,
    inherited: Option<windows_sys::Win32::Foundation::HANDLE>,
    device: &[u8],
) {
    use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileA, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::SetStdHandle;

    let handle = if let Some(handle) = inherited {
        handle
    } else {
        // SAFETY: `device` is a null-terminated ASCII device name. CreateFileA
        // opens the console input/output buffer of the attached console.
        let console = unsafe {
            CreateFileA(
                device.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if console.is_null() || console == INVALID_HANDLE_VALUE {
            return;
        }
        console
    };

    // SAFETY: `handle` is a valid open handle; SetStdHandle only stores it.
    unsafe {
        SetStdHandle(which, handle);
    }
}

/// Wait for the user to press Enter before the console window closes.
///
/// Prints a dimmed prompt to `stderr` and blocks on `stdin`.  Intended for
/// [`ConsoleMode::Standalone`] sessions where the console would vanish
/// immediately after the program exits.
pub fn pause_before_exit() {
    use std::io::Write;
    eprint!("\n  {}", crate::strings::cli::PAUSE_PROMPT.dimmed());
    drop(std::io::stderr().flush());
    drop(std::io::stdin().read_line(&mut String::new()));
}

/// Enable ANSI virtual terminal processing on the standard output console.
///
/// Sets `ENABLE_VIRTUAL_TERMINAL_PROCESSING` on the `STD_OUTPUT_HANDLE` so
/// that ANSI escape sequences (used by the `colored` crate) render correctly
/// in legacy `conhost.exe` terminals.  Does nothing if the handle is invalid
/// (e.g. output is fully redirected with no console attached).
pub fn enable_ansi_colors() {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE,
        SetConsoleMode,
    };
    // SAFETY: GetStdHandle/GetConsoleMode/SetConsoleMode are standard Win32 calls
    // with no preconditions beyond a valid handle. STD_OUTPUT_HANDLE is always valid.
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        // Guard against INVALID_HANDLE_VALUE (e.g. output fully redirected with no console)
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return;
        }
        let mut mode: u32 = 0;
        if GetConsoleMode(handle, &raw mut mode) != 0 {
            let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}

// ─── Ctrl+C handling ─────────────────────────────────────────────────────────

/// Set by the console control handler once Ctrl+C, Ctrl+Break or a console
/// close has been requested.
///
/// Must be a `static`: `SetConsoleCtrlHandler` callbacks are `extern
/// "system"` functions that cannot capture any state.
static INTERRUPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Console control handler: records the interrupt instead of letting Windows
/// terminate the process.
///
/// Handles `CTRL_C_EVENT` (0), `CTRL_BREAK_EVENT` (1) and `CTRL_CLOSE_EVENT`
/// (2). For `CTRL_CLOSE_EVENT` Windows terminates the process shortly after
/// the handler returns, so only Ctrl+C / Ctrl+Break allow a graceful stop.
unsafe extern "system" fn ctrl_handler(ctrl_type: u32) -> i32 {
    if ctrl_type <= 2 {
        INTERRUPTED.store(true, std::sync::atomic::Ordering::Release);
        1 // TRUE: handled, prevent default process termination
    } else {
        0 // FALSE: not handled, pass to the next handler
    }
}

/// Start turning Ctrl+C / Ctrl+Break into a flag polled with [`interrupted`]
/// instead of terminating the process. Clears any earlier interrupt.
///
/// # Errors
///
/// Fails if the handler cannot be registered.
pub fn watch_interrupts() -> anyhow::Result<()> {
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

    INTERRUPTED.store(false, std::sync::atomic::Ordering::Release);
    // SAFETY: `ctrl_handler` is an `extern "system"` fn with the signature
    // SetConsoleCtrlHandler expects, valid for the whole process lifetime.
    let ok = unsafe { SetConsoleCtrlHandler(Some(ctrl_handler), 1) };
    anyhow::ensure!(
        ok != 0,
        "SetConsoleCtrlHandler failed - cannot guarantee graceful shutdown"
    );
    Ok(())
}

/// Whether an interrupt arrived since [`watch_interrupts`] was called.
#[must_use]
pub fn interrupted() -> bool {
    INTERRUPTED.load(std::sync::atomic::Ordering::Acquire)
}
