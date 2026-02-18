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
    menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
};

// ─── Public Types ─────────────────────────────────────────────────────────────

/// Actions dispatched by the system-tray icon or its context menu.
pub enum TrayAction {
    /// Bring the main window back to the foreground.
    Show,
    /// Exit the application cleanly.
    Quit,
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
    /// The `ctx` is cloned into the watcher thread so it can call
    /// [`egui::Context::request_repaint`] when a tray event arrives — this
    /// is necessary to wake eframe's event loop while the window is hidden.
    ///
    /// # Errors
    ///
    /// Returns an error string on image-decode failure, menu-build failure, or
    /// if the `tray_icon` back-end or the watcher thread cannot be created.
    pub fn new(ctx: egui::Context) -> Result<Self, String> {
        let icon = load_icon()?;
        let (show_item, quit_item, menu) = build_menu()?;
        let show_id = show_item.id().clone();
        let quit_id = quit_item.id().clone();

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
            .spawn(move || tray_watcher_thread(&show_id, &quit_id, &tx, &shutdown_ref, &ctx))
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
/// When an event is decoded the function sends a [`TrayAction`] to the
/// app and calls [`egui::Context::request_repaint`] to guarantee `update()`
/// runs even while the window is hidden.
fn tray_watcher_thread(
    show_id: &MenuId,
    quit_id: &MenuId,
    tx: &Sender<TrayAction>,
    shutdown: &Arc<AtomicBool>,
    ctx: &egui::Context,
) {
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        let mut had_event = false;

        // Drain all pending menu-item click events.
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let action = if event.id == show_id {
                TrayAction::Show
            } else if event.id == quit_id {
                TrayAction::Quit
            } else {
                continue;
            };
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

/// Build the tray context menu and return item handles alongside the [`Menu`].
///
/// Returns `(show_item, quit_item, menu)` so that the caller can clone the
/// item IDs before the items are moved into the menu.
fn build_menu() -> Result<(MenuItem, MenuItem, Menu), String> {
    let show_item = MenuItem::new("Open MagicX RAM Cleaner", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    menu.append_items(&[&show_item, &PredefinedMenuItem::separator(), &quit_item])
        .map_err(|e| format!("Failed to build tray menu: {e}"))?;
    Ok((show_item, quit_item, menu))
}
