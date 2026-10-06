//! # `MagicX` RAM Cleaner - GUI Mode
//!
//! egui interface in the Windows 11 style described in `docs/UI_DESIGN.md`:
//! an Overview built around the memory map, a Monitor with a history chart
//! and auto-clean, a Processes list with per-program Trim, Settings and
//! About, behind a Task Manager style navigation pane.
//!
//! The GUI is launched when the binary is executed with no CLI subcommand
//! (e.g. double-click from Explorer). All cleaning and measuring is done by
//! the engine and memory layers; the GUI only presents them.
//!
//! ## Architecture
//!
//! ```text
//! src/gui/
//! ├── mod.rs          - entry point (run_gui), the Phosphor icon subset
//! ├── app/            - MagicXApp state and eframe loop: appearance,
//! │                     background threads, cleaning, chart history,
//! │                     trims, tray events
//! ├── nav.rs          - navigation pane and page routing
//! ├── theme.rs        - palette, type scale, spacing, egui visuals
//! ├── fonts.rs        - Segoe UI from the system fonts folder
//! ├── widgets.rs      - cards, Settings rows, switches, segmented
//! │                     controls, sliders, buttons
//! ├── settings.rs     - persisted settings, defaults, valid ranges
//! ├── persistence.rs  - settings file I/O, import/export, migration
//! ├── tray.rs         - system-tray icon and menu
//! └── panels/         - overview, monitor, processes, settings, about
//! ```
//!
//! ## Threading Model
//!
//! ```text
//! ┌────────────────────────────────────────────┐
//! │  eframe (winit + glow)                     │
//! │  ├─ MagicXApp::logic()   (UI thread)       │
//! │  │  ├─ polls channels for clean results    │
//! │  │  └─ polls tray icon event queue         │
//! │  ├─ MagicXApp::ui()     (UI thread)        │
//! │  │  └─ renders the pane + active page      │
//! │  ├─ stats_thread   (background, 1 Hz)      │
//! │  │  └─ captures MemorySnapshot + history   │
//! │  ├─ clean_thread   (on demand)             │
//! │  │  └─ runs smart_clean, reports progress  │
//! │  └─ trim_thread    (on demand)             │
//! │     └─ trims one program's processes       │
//! └────────────────────────────────────────────┘
//! ```

pub mod app;
mod fonts;
mod nav;
mod panels;

egui_phosphor::subset! {
    /// The Phosphor icons the GUI and tray menu use, built into a font that
    /// holds only these glyphs instead of the full ~490 KB icon font.
    mod icons {
        use regular::{
            ACTIVITY, ARROW_RIGHT, BROOM, CARET_DOWN, CARET_UP, CHECK, CLOCK, CODE, CPU,
            DOWNLOAD_SIMPLE, FIRE, FLOPPY_DISK, GAUGE, GEAR, GITHUB_LOGO, GLOBE, HEART, INFO,
            LEAF, LIGHTNING, LINKEDIN_LOGO, LIST, MAGNIFYING_GLASS, MOUSE_RIGHT_CLICK, PALETTE,
            POWER, RADIOACTIVE, ROCKET_LAUNCH, SCALES, SLIDERS, TELEGRAM_LOGO, TRAY,
            UPLOAD_SIMPLE, WARNING_CIRCLE, WINDOWS_LOGO, X,
        };
    }
}
pub(super) mod persistence;
pub mod settings;
pub mod theme;
mod tray;
mod widgets;

use anyhow::{Context, Result};
use eframe::egui;

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
pub fn run_gui(start_in_tray: bool) -> Result<()> {
    // ── Single-instance guard ────────────────────────────────────────
    // Acquire a system-wide named mutex. If another instance is already
    // running, its window is restored and we exit silently.
    let Some(_instance_guard) = crate::platform::instance::SingleInstance::acquire() else {
        return Ok(());
    };

    // Ensure we have admin privileges before launching the GUI
    crate::platform::privilege::check_admin()?;
    crate::platform::privilege::enable_all_privileges()
        .context("Failed to enable privileges. Make sure you're running as Administrator.")?;

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([860.0, 600.0])
        .with_min_inner_size([620.0, 500.0])
        .with_title(crate::ids::WINDOW_TITLE)
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
        crate::ids::WINDOW_TITLE,
        native_options,
        Box::new(move |cc| Ok(Box::new(app::MagicXApp::new(cc, start_in_tray)?))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))
}
