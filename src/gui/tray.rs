//! # System Tray Icon
//!
//! Provides a persistent Windows notification-area icon with a context menu while
//! minimize-to-tray is enabled.
//!
//! [`TrayHandle`] wraps a [`tray_icon::TrayIcon`] and keeps it alive for as long
//! as the struct exists.  Dropping the handle automatically removes the icon from
//! the notification area.
//!
//! This module is a private sibling of [`super::app`] and is not part of the
//! public API.

use tray_icon::{
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

// ─── Public Types ─────────────────────────────────────────────────────────────

/// Actions that can be dispatched by the system-tray icon or its context menu.
pub enum TrayAction {
    /// Bring the main window back to the foreground.
    Show,
    /// Exit the application cleanly.
    Quit,
}

/// Live system-tray icon handle.
///
/// While this value is alive the `MagicX` RAM Cleaner icon appears in the Windows
/// notification area.  Dropping the handle removes it automatically.
pub struct TrayHandle {
    /// The underlying tray icon; kept alive for its [`Drop`] side-effect.
    _icon: TrayIcon,
    /// ID of the "Open" menu item, used to route [`MenuEvent`]s.
    show_id: tray_icon::menu::MenuId,
    /// ID of the "Quit" menu item, used to route [`MenuEvent`]s.
    quit_id: tray_icon::menu::MenuId,
}

impl TrayHandle {
    /// Create and register a system-tray icon with a context menu.
    ///
    /// Loads the application icon from the embedded `assets/app.ico` file,
    /// builds a two-item menu ("Open" + separator + "Quit"), and registers the
    /// icon with Windows.
    ///
    /// # Errors
    ///
    /// Returns an error string on image-decode failure, menu-build failure, or
    /// if the `tray_icon` back-end itself returns an error.
    pub fn new() -> Result<Self, String> {
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

        Ok(Self {
            _icon: tray,
            show_id,
            quit_id,
        })
    }

    /// Poll for a pending tray interaction without blocking.
    ///
    /// Checks the menu-event channel first, then the raw tray-icon event channel.
    /// Returns [`Some(TrayAction)`] on the first recognisable event, or [`None`]
    /// when both queues are empty.
    pub fn poll(&self) -> Option<TrayAction> {
        // Menu-item clicks take priority over raw icon clicks.
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.show_id {
                return Some(TrayAction::Show);
            } else if event.id == self.quit_id {
                return Some(TrayAction::Quit);
            }
        }

        // Left-button-up click on the tray icon restores the window.
        if let Ok(TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        }) = TrayIconEvent::receiver().try_recv()
        {
            return Some(TrayAction::Show);
        }

        None
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
/// Returns `(show_item, quit_item, menu)` so that the caller can clone the item
/// IDs before the items are moved into the menu storage.
fn build_menu() -> Result<(MenuItem, MenuItem, Menu), String> {
    let show_item = MenuItem::new("Open MagicX RAM Cleaner", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    menu.append_items(&[&show_item, &PredefinedMenuItem::separator(), &quit_item])
        .map_err(|e| format!("Failed to build tray menu: {e}"))?;
    Ok((show_item, quit_item, menu))
}
