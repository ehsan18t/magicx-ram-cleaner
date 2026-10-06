//! # Windows integration
//!
//! How the app hooks into Windows: the Explorer context menu and the
//! autostart logon task. These modules decide *what* to register; the
//! registry and Task Scheduler calls themselves live in [`crate::platform`].

pub mod autostart;
pub mod context_menu;
mod menu_icons;
