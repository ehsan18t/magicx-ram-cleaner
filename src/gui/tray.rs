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

use eframe::egui;
use tray_icon::{
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu},
};

use crate::cleaner::CleanLevel;

// ─── Public Types ─────────────────────────────────────────────────────────────

/// Actions dispatched by the system-tray icon or its context menu.
pub enum TrayAction {
    /// Bring the main window back to the foreground.
    Show,
    /// Start a cleaning operation at the given level.
    Clean(CleanLevel),
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
    /// (obtained via [`crate::console::find_app_window`]).  The watcher
    /// thread uses it to post a synthetic `WM_PAINT` message that wakes
    /// eframe's event loop even while the window is invisible — unlike
    /// `request_repaint`, `PostMessageW(WM_PAINT)` is not suppressed for
    /// windows with `WS_VISIBLE` cleared.
    ///
    /// # Errors
    ///
    /// Returns an error string on image-decode failure, menu-build failure, or
    /// if the `tray_icon` back-end or the watcher thread cannot be created.
    pub fn new(ctx: egui::Context, hwnd: isize) -> Result<Self, String> {
        let icon = load_icon()?;
        let (ids, menu) = build_menu()?;

        let tray = TrayIconBuilder::new()
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_tooltip("MagicX RAM Cleaner")
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
/// 1. Makes the window visible via Win32 `ShowWindow` so that eframe can
///    process repaints (Windows suppresses `RedrawWindow` for invisible
///    windows, defeating `ctx.request_repaint()`).
/// 2. Sends a [`TrayAction`] through the channel.
/// 3. Calls [`egui::Context::request_repaint`] to guarantee `update()` runs.
fn tray_watcher_thread(
    ids: &MenuIds,
    tx: &Sender<TrayAction>,
    shutdown: &Arc<AtomicBool>,
    ctx: &egui::Context,
    hwnd: isize,
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
            } else {
                continue;
            };

            // Make the window visible BEFORE requesting a repaint.
            // For Show / Clean: restore to foreground with focus.
            // For Quit: reveal silently so eframe can process the close.
            match action {
                TrayAction::Show | TrayAction::Clean(_) => {
                    crate::console::restore_window(hwnd);
                }
                TrayAction::Quit => crate::console::reveal_window(hwnd),
            }

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
                crate::console::restore_window(hwnd);
                if tx.send(TrayAction::Show).is_err() {
                    return;
                }
                ctx.request_repaint();
                had_event = true;
            }
        }

        // Sleep when idle to avoid spinning a full CPU core.
        if !had_event {
            std::thread::sleep(Duration::from_millis(50));
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
/// Layout:
/// ```text
/// Open MagicX RAM Cleaner
/// ────────────────────────
/// Clean RAM ▸
///   Gentle
///   Moderate
///   Aggressive
///   Nuclear
/// ────────────────────────
/// Quit
/// ```
fn build_menu() -> Result<(MenuIds, Menu), String> {
    let show_item = MenuItem::new("Open MagicX RAM Cleaner", true, None);
    let quit_item = MenuItem::new("Quit", true, None);

    let gentle_item = MenuItem::new("Gentle", true, None);
    let moderate_item = MenuItem::new("Moderate", true, None);
    let aggressive_item = MenuItem::new("Aggressive", true, None);
    let nuclear_item = MenuItem::new("Nuclear", true, None);

    let ids = MenuIds {
        show: show_item.id().clone(),
        quit: quit_item.id().clone(),
        clean_gentle: gentle_item.id().clone(),
        clean_moderate: moderate_item.id().clone(),
        clean_aggressive: aggressive_item.id().clone(),
        clean_nuclear: nuclear_item.id().clone(),
    };

    let clean_submenu = Submenu::new("Clean RAM", true);
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
        &quit_item,
    ])
    .map_err(|e| format!("Failed to build tray menu: {e}"))?;

    Ok((ids, menu))
}
