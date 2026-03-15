//! # System Tray Icon
//!
//! Provides a persistent Windows notification-area icon with a context menu while
//! minimize-to-tray is enabled.
//!
//! ## Architecture
//!
//! The [`TrayHandle`] spawns a dedicated `tray-watcher` background thread on
//! creation.  That thread is the **sole consumer** of the global
//! [`tray_icon`] event channels (one per-channel `try_recv` from a single thread
//! avoids concurrent access to the `!Sync` static receivers).  When an event
//! arrives the thread:
//!
//! 1. Decodes it into a [`TrayAction`].
//! 2. Sends it through our own [`std::sync::mpsc`] channel.
//! 3. Calls [`egui::Context::request_repaint`] so eframe wakes up and calls
//!    `update()` even while the window is hidden.
//!
//! [`TrayHandle::poll`] drains our channel and returns at most one action per
//! call; it never touches the global tray channels.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use eframe::egui;
use tray_icon::{
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Icon, IconMenuItem, Menu, MenuEvent, MenuId, PredefinedMenuItem, Submenu},
};

use crate::cleaner::CleanLevel;
use crate::strings;

use super::app::Panel;

// ─── Public Types ─────────────────────────────────────────────────────────────

/// Actions dispatched by the system-tray icon or its context menu.
pub enum TrayAction {
    /// Bring the main window back to the foreground.
    Show,
    /// Start a cleaning operation at the given level.
    Clean(CleanLevel),
    /// Navigate to a specific panel in the GUI.
    Navigate(Panel),
    /// Exit the application cleanly.
    Quit,
}

/// Collected [`MenuId`]s for every actionable item in the tray context menu.
///
/// Cloned from the menu items before they are moved into the [`Menu`] and
/// passed to the watcher thread for event decoding.
struct MenuIds {
    /// "Open `MagicX` RAM Cleaner" item.
    show: MenuId,
    /// "Quit" item.
    quit: MenuId,
    /// Clean → Gentle item.
    clean_gentle: MenuId,
    /// Clean → Moderate item.
    clean_moderate: MenuId,
    /// Clean → Aggressive item.
    clean_aggressive: MenuId,
    /// Clean → Nuclear item.
    clean_nuclear: MenuId,
    /// Navigate → Dashboard item.
    nav_dashboard: MenuId,
    /// Navigate → Monitor item.
    nav_monitor: MenuId,
    /// Navigate → Processes item.
    nav_processes: MenuId,
    /// Navigate → Settings item.
    nav_settings: MenuId,
}

/// Live system-tray icon handle.
///
/// While this value is alive the `MagicX` RAM Cleaner icon appears in the
/// Windows notification area.  Dropping the handle removes the icon and stops
/// the background watcher thread.
pub struct TrayHandle {
    /// The underlying tray icon; kept alive for its [`Drop`] side-effect.
    _icon: TrayIcon,
    /// Receives decoded [`TrayAction`]s from the background watcher thread.
    rx: Receiver<TrayAction>,
    /// Set to `true` on drop to signal the watcher thread to exit.
    shutdown: Arc<AtomicBool>,
}

impl TrayHandle {
    /// Create and register a system-tray icon with a context menu.
    ///
    /// Loads the application icon from the embedded `assets/app.ico` file,
    /// builds a two-item menu ("Open" + separator + "Quit"), registers the
    /// icon with Windows, and spawns the background event-watcher thread.
    ///
    /// `ctx` is cloned into the watcher thread so it can call
    /// [`egui::Context::request_repaint`] when a tray event arrives.
    ///
    /// `hwnd` is the Win32 window handle of the main application window
    /// (obtained via [`crate::console::find_app_window`]).  Currently
    /// unused by the watcher thread but retained in the API for future
    /// use (e.g. posting synthetic messages for edge-case wakeups).
    ///
    /// `dark` controls the glyph colour in menu icons: white glyphs for
    /// dark menus, charcoal for light menus.  The caller should pass the
    /// in-app theme preference, which **must** match the process-wide menu
    /// theme forced by [`crate::console::set_process_dark_mode`].
    ///
    /// # Errors
    ///
    /// Returns an error string on image-decode failure, menu-build failure, or
    /// if the `tray_icon` back-end or the watcher thread cannot be created.
    pub fn new(ctx: egui::Context, hwnd: isize, dark: bool) -> Result<Self, String> {
        let icon = load_icon()?;
        // The process-wide menu theme is forced by set_process_dark_mode()
        // before this call, so glyph colours should match the app's theme,
        // not the OS theme.
        let (ids, menu) = build_menu(dark)?;

        let tray = TrayIconBuilder::new()
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_tooltip(strings::tray::TOOLTIP)
            .build()
            .map_err(|e| format!("Failed to register tray icon: {e}"))?;

        let (tx, rx) = std::sync::mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_ref = Arc::clone(&shutdown);

        std::thread::Builder::new()
            .name("tray-watcher".into())
            .spawn(move || {
                tray_watcher_thread(&ids, &tx, &shutdown_ref, &ctx, hwnd);
            })
            .map_err(|e| format!("Failed to spawn tray watcher thread: {e}"))?;

        Ok(Self {
            _icon: tray,
            rx,
            shutdown,
        })
    }

    /// Return the next pending [`TrayAction`] without blocking, or [`None`].
    ///
    /// Always call this from the egui `update()` callback; the action is
    /// delivered by the background watcher thread via an internal channel.
    pub fn poll(&self) -> Option<TrayAction> {
        self.rx.try_recv().ok()
    }
}

impl Drop for TrayHandle {
    fn drop(&mut self) {
        // Signal the watcher thread to exit on its next iteration.
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

// ─── Background Watcher Thread ────────────────────────────────────────────────

/// Long-running thread that polls the global `tray_icon` event channels.
///
/// This is the **only** place [`MenuEvent::receiver()`] and
/// [`TrayIconEvent::receiver()`] are called, ensuring no concurrent access to
/// the non-`Sync` static receivers.
///
/// When an event is decoded the function:
///
/// 1. Sends a [`TrayAction`] through the channel.
/// 2. Calls [`egui::Context::request_repaint`] to guarantee `update()` runs.
///
/// Window state management (uncloaking, restoring extended styles, bringing
/// to foreground) is handled by the main thread in `poll_tray_events`.
fn tray_watcher_thread(
    ids: &MenuIds,
    tx: &Sender<TrayAction>,
    shutdown: &Arc<AtomicBool>,
    ctx: &egui::Context,
    _hwnd: isize,
) {
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        let mut had_event = false;

        // Drain all pending menu-item click events.
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let action = if event.id == ids.show {
                TrayAction::Show
            } else if event.id == ids.quit {
                TrayAction::Quit
            } else if event.id == ids.clean_gentle {
                TrayAction::Clean(CleanLevel::Gentle)
            } else if event.id == ids.clean_moderate {
                TrayAction::Clean(CleanLevel::Moderate)
            } else if event.id == ids.clean_aggressive {
                TrayAction::Clean(CleanLevel::Aggressive)
            } else if event.id == ids.clean_nuclear {
                TrayAction::Clean(CleanLevel::Nuclear)
            } else if event.id == ids.nav_dashboard {
                TrayAction::Navigate(Panel::Dashboard)
            } else if event.id == ids.nav_monitor {
                TrayAction::Navigate(Panel::Monitor)
            } else if event.id == ids.nav_processes {
                TrayAction::Navigate(Panel::Processes)
            } else if event.id == ids.nav_settings {
                TrayAction::Navigate(Panel::Settings)
            } else {
                continue;
            };

            // Window state management (uncloak/restore) is handled by the
            // main thread in poll_tray_events.  We only send the action and
            // request a repaint to wake the event loop.

            if tx.send(action).is_err() {
                return; // app receiver dropped — exit cleanly
            }
            ctx.request_repaint();
            had_event = true;
        }

        // Drain all pending raw tray-icon interaction events.
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            // Left-click (button-up) on the icon opens the window.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if tx.send(TrayAction::Show).is_err() {
                    return;
                }
                ctx.request_repaint();
                had_event = true;
            }
        }

        // Sleep when idle to avoid spinning a full CPU core.
        // 200 ms is responsive enough for tray menu interactions while
        // keeping thread wakeups minimal (5 Hz instead of 20 Hz).
        if !had_event {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

// ─── Private Helpers ──────────────────────────────────────────────────────────

/// Decode the embedded application icon into a [`tray_icon::Icon`].
fn load_icon() -> Result<tray_icon::Icon, String> {
    let bytes = include_bytes!("../../assets/app.ico");
    let img = image::load_from_memory(bytes)
        .map_err(|e| format!("Failed to decode tray icon image: {e}"))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    tray_icon::Icon::from_rgba(rgba.into_raw(), w, h)
        .map_err(|e| format!("Failed to build tray icon handle: {e}"))
}

/// Build the tray context menu and return [`MenuIds`] alongside the [`Menu`].
///
/// Each item gets a Phosphor icon glyph rendered to a 16×16 bitmap so the
/// menu looks polished on every Windows theme.
///
/// Layout:
/// ```text
/// 🚀  Open MagicX RAM Cleaner
/// ────────────────────────────
/// 🧹  Clean RAM ▸
///     🌿  Gentle
///     ⚡  Moderate
///     🔥  Aggressive
///     ☢   Nuclear
/// ────────────────────────────
/// 📊  Dashboard
/// 📈  Monitor
/// 🖥   Processes
/// ⚙   Settings
/// ────────────────────────────
/// ⏻   Quit
/// ```
fn build_menu(dark: bool) -> Result<(MenuIds, Menu), String> {
    // Phosphor Regular codepoints (from egui_phosphor::regular).
    let show_item = icon_menu_item(strings::tray::OPEN, '\u{E3FE}', dark); // ROCKET_LAUNCH
    let quit_item = icon_menu_item(strings::tray::QUIT, '\u{E3DA}', dark); // POWER

    let gentle_item = icon_menu_item(strings::levels::GENTLE_NAME, '\u{E2DA}', dark); // LEAF
    let moderate_item = icon_menu_item(strings::levels::MODERATE_NAME, '\u{E2DE}', dark); // LIGHTNING
    let aggressive_item = icon_menu_item(strings::levels::AGGRESSIVE_NAME, '\u{E242}', dark); // FIRE
    let nuclear_item = icon_menu_item(strings::levels::NUCLEAR_NAME, '\u{E9DC}', dark); // RADIOACTIVE

    let nav_dashboard = icon_menu_item(strings::tray::NAV_DASHBOARD, '\u{E628}', dark); // GAUGE
    let nav_monitor = icon_menu_item(strings::tray::NAV_MONITOR, '\u{E000}', dark); // ACTIVITY
    let nav_processes = icon_menu_item(strings::tray::NAV_PROCESSES, '\u{E610}', dark); // CPU
    let nav_settings = icon_menu_item(strings::tray::NAV_SETTINGS, '\u{E270}', dark); // GEAR

    let ids = MenuIds {
        show: show_item.id().clone(),
        quit: quit_item.id().clone(),
        clean_gentle: gentle_item.id().clone(),
        clean_moderate: moderate_item.id().clone(),
        clean_aggressive: aggressive_item.id().clone(),
        clean_nuclear: nuclear_item.id().clone(),
        nav_dashboard: nav_dashboard.id().clone(),
        nav_monitor: nav_monitor.id().clone(),
        nav_processes: nav_processes.id().clone(),
        nav_settings: nav_settings.id().clone(),
    };

    let clean_submenu = Submenu::new(strings::tray::SUBMENU_CLEAN, true);
    // Give the submenu itself a broom icon.
    if let Some(broom) = rasterize_glyph('\u{EC54}', dark) {
        // SAFETY: set_icon cannot fail on Windows; errors are silently ignored.
        clean_submenu.set_icon(Some(broom));
    }
    clean_submenu
        .append_items(&[
            &gentle_item,
            &moderate_item,
            &aggressive_item,
            &nuclear_item,
        ])
        .map_err(|e| format!("Failed to build clean submenu: {e}"))?;

    let menu = Menu::new();
    menu.append_items(&[
        &show_item,
        &PredefinedMenuItem::separator(),
        &clean_submenu,
        &PredefinedMenuItem::separator(),
        &nav_dashboard,
        &nav_monitor,
        &nav_processes,
        &nav_settings,
        &PredefinedMenuItem::separator(),
        &quit_item,
    ])
    .map_err(|e| format!("Failed to build tray menu: {e}"))?;

    Ok((ids, menu))
}

// ─── Phosphor Glyph Rendering ─────────────────────────────────────────────────

/// Menu icon size in logical pixels.
const ICON_SIZE: u32 = 16;

/// Create an [`IconMenuItem`] with a Phosphor icon glyph.
///
/// Falls back to a text-only item if glyph rasterization fails.
/// `dark` controls the glyph colour: white for dark OS menus, dark for light.
fn icon_menu_item(label: &str, glyph: char, dark: bool) -> IconMenuItem {
    IconMenuItem::new(label, true, rasterize_glyph(glyph, dark), None)
}

/// Rasterize a single Phosphor Regular glyph into a 16×16 RGBA
/// [`tray_icon::menu::Icon`].
///
/// Uses `ab_glyph` (already a transitive dependency of `epaint`) to render
/// the glyph from the embedded Phosphor font.  Returns `None` on any
/// failure so callers degrade gracefully to text-only menu items.
///
/// When `dark` is `true` the glyph is rendered white (for dark OS menu
/// backgrounds); when `false` it is rendered in a dark charcoal colour so
/// it remains visible on light menu backgrounds.
fn rasterize_glyph(codepoint: char, dark: bool) -> Option<Icon> {
    let font_bytes = egui_phosphor::Variant::Regular.font_bytes();
    let font = FontRef::try_from_slice(font_bytes).ok()?;

    let glyph_id = font.glyph_id(codepoint);
    // Return None if the font doesn't contain this codepoint.
    if glyph_id.0 == 0 {
        return None;
    }

    let scale = PxScale::from(ICON_SIZE as f32);
    let scaled = font.as_scaled(scale);
    let positioned = glyph_id.with_scale_and_position(scale, point(0.0, scaled.ascent()));

    let outlined = font.outline_glyph(positioned)?;
    let bounds = outlined.px_bounds();

    // Glyph pixel dimensions (may be smaller than ICON_SIZE).
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "px_bounds dimensions are small positive values for 16px glyphs"
    )]
    let (gw, gh) = (bounds.width() as u32, bounds.height() as u32);

    if gw == 0 || gh == 0 {
        return None;
    }

    // Centre the rasterized glyph inside a ICON_SIZE×ICON_SIZE canvas.
    let canvas = ICON_SIZE;
    let off_x = (canvas.saturating_sub(gw)) / 2;
    let off_y = (canvas.saturating_sub(gh)) / 2;

    let mut rgba = vec![0u8; (canvas * canvas * 4) as usize];

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "coverage is 0.0..=1.0; (py * canvas + px) * 4 fits in usize for 16px icons"
    )]
    outlined.draw(|x, y, coverage| {
        let px = x + off_x;
        let py = y + off_y;
        if px < canvas && py < canvas {
            let idx = ((py * canvas + px) * 4) as usize;
            let alpha = (coverage * 255.0) as u8;
            // Glyph colour adapts to the theme so icons remain visible
            // on both dark and light OS context menu backgrounds.
            let (r, g, b) = if dark {
                (255_u8, 255_u8, 255_u8) // white on dark
            } else {
                (30_u8, 33_u8, 36_u8) // charcoal on light
            };
            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = alpha;
        }
    });

    Icon::from_rgba(rgba, canvas, canvas).ok()
}
