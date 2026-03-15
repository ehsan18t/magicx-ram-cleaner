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
//! ├── tray.rs      — system-tray icon handle and action routing
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
//! │  │  ├─ polls channels for clean results    │
//! │  │  └─ polls tray icon event queue         │
//! │  ├─ stats_thread   (background, 1 Hz)      │
//! │  │  └─ captures MemorySnapshot             │
//! │  └─ clean_thread   (on demand)             │
//! │     └─ runs smart_clean, sends result      │
//! └────────────────────────────────────────────┘
//! ```

pub mod app;
mod panels;
#[allow(unsafe_code)] // native Win32 file-picker dialogs (GetOpenFileNameW / GetSaveFileNameW)
pub(super) mod persistence;
pub mod theme;
mod tray;
mod widgets;

use anyhow::{Context, Result};
use eframe::egui;

use crate::strings;

/// Load the application icon from the embedded PNG for use as the window icon.
fn load_window_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../../assets/app.png");
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    })
}

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
    // ── Single-instance guard ────────────────────────────────────────
    // Acquire a system-wide named mutex. If another instance is already
    // running, its window is restored and we exit silently.
    let Some(_instance_guard) = crate::console::try_acquire_single_instance() else {
        return Ok(());
    };

    // Ensure we have admin privileges before launching the GUI
    crate::privilege::check_admin()?;
    crate::privilege::enable_all_privileges()
        .context("Failed to enable privileges. Make sure you're running as Administrator.")?;

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([620.0, 500.0])
        .with_min_inner_size([620.0, 500.0])
        .with_title(strings::gui::WINDOW_TITLE)
        // Start hidden and reveal on first frame to avoid flash.
        .with_visible(false);

    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "MagicX RAM Cleaner",
        native_options,
        Box::new(|cc| Ok(Box::new(app::MagicXApp::new(cc)?))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))
}
