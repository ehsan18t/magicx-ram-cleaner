//! # `MagicX` RAM Cleaner — GUI Mode
//!
//! egui-based graphical user interface that provides a dashboard for
//! memory monitoring, one-click cleaning, process inspection, and settings.
//!
//! The GUI is launched when the binary is executed with no CLI subcommand
//! (e.g. double-click from Explorer). All cleaning and statistics logic
//! is delegated to the existing library modules (`cleaner`, `stats`, etc.).
//!
//! ## Architecture
//!
//! ```text
//! src/gui/
//! ├── mod.rs       — entry point (run_gui), module re-exports
//! ├── app.rs       — MagicXApp state, eframe::App impl, sidebar, layout
//! ├── theme.rs     — colour palette, spacing, custom Visuals
//! ├── widgets.rs   — reusable components (cards, stat labels, buttons)
//! └── panels/
//!     ├── mod.rs         — panel re-exports
//!     ├── dashboard.rs   — memory overview, cleaning buttons, quick info
//!     ├── monitor.rs     — auto-clean threshold / cooldown controls
//!     ├── processes.rs   — sortable process table
//!     └── settings.rs    — appearance, integration preferences
//! ```
//!
//! ## Threading Model
//!
//! ```text
//! ┌────────────────────────────────────────────┐
//! │  eframe (winit + glow)                     │
//! │  ├─ MagicXApp::update()  (UI thread)       │
//! │  │  └─ polls channels for results          │
//! │  ├─ stats_thread   (background, 1 Hz)      │
//! │  │  └─ captures MemorySnapshot             │
//! │  └─ clean_thread   (on demand)             │
//! │     └─ runs smart_clean, sends result      │
//! └────────────────────────────────────────────┘
//! ```

pub mod app;
mod panels;
pub(super) mod persistence;
pub mod theme;
mod widgets;

use anyhow::{Context, Result};
use eframe::egui;

/// Launch the egui GUI window.
///
/// This is the main entry point called from `main()` when no CLI subcommand
/// is specified. Blocks until the window is closed (or the user exits via
/// the system tray).
///
/// # Errors
///
/// Returns an error if eframe cannot initialise the window or OpenGL context,
/// or if the process lacks administrator privileges.
pub fn run_gui() -> Result<()> {
    // Ensure we have admin privileges before launching the GUI
    crate::privilege::check_admin()?;
    crate::privilege::enable_all_privileges()
        .context("Failed to enable privileges. Make sure you're running as Administrator.")?;

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 580.0])
            .with_min_inner_size([600.0, 460.0])
            .with_title("MagicX RAM Cleaner")
            // Start hidden and reveal on first frame to avoid flash.
            .with_visible(false),
        ..Default::default()
    };

    eframe::run_native(
        "MagicX RAM Cleaner",
        native_options,
        Box::new(|cc| Ok(Box::new(app::MagicXApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))
}
