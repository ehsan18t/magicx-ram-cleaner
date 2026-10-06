//! `MagicX` RAM Cleaner - binary entry point.
//!
//! All behaviour lives in the library; see [`magicx_ram_cleaner::app`].

// SUBSYSTEM:CONSOLE (the default) so cmd and PowerShell wait for the CLI and
// see its exit code. The manifest's `consoleAllocationPolicy = detached`
// keeps Explorer / context-menu launches console-free on Windows 11 24H2+.

use std::process::ExitCode;

fn main() -> ExitCode {
    magicx_ram_cleaner::app::run()
}
