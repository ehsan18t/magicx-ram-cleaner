//! # Platform layer: every Win32 and NT call the application makes.
//!
//! This is the only module tree allowed to contain `unsafe` code. Each
//! submodule wraps one area of the operating system behind safe functions
//! with documented failure modes, so the layers above (memory, engine,
//! integration, CLI and GUI) are written in safe Rust only.
//!
//! Dependency rule: `platform` depends on nothing else in this crate except
//! `strings` (for user-facing text it renders itself, such as the pause
//! prompt or balloon tooltip).

pub mod console;
pub mod handle;
pub mod instance;
pub mod memory;
pub mod notify;
pub mod nt;
pub mod privilege;
pub mod process;
pub mod shell;
pub mod time;
pub mod wide;
pub mod window;
