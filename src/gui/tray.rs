//! # System Tray Icon
//!
//! Provides a persistent Windows notification-area icon with a context menu while
//! minimize-to-tray is enabled.
//!
//! ## Architecture
//!
//! `tray-icon` delivers clicks through callbacks that run on the UI thread,
//! inside the message loop eframe already pumps, so no polling thread is
//! needed. The callbacks can only be installed once per process, while the
//! icon is rebuilt whenever the theme changes, so they forward each event to
//! whichever [`TrayHandle`] is live (through [`SINK`]) and wake eframe with
//! [`egui::Context::request_repaint`], which runs `logic()` even while the
//! window is hidden. [`TrayHandle::poll`] then decodes the events into
//! [`TrayAction`]s against its own menu.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Mutex, Once};

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use eframe::egui;
use tray_icon::{
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Icon, IconMenuItem, Menu, MenuEvent, MenuId, PredefinedMenuItem, Submenu},
};

use crate::engine::CleanLevel;
use crate::strings;

use super::app::Panel;
use super::icons::regular as ph;

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
/// Cloned from the menu items before they are moved into the [`Menu`], so
/// events can be decoded after the menu is handed to the icon.
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

impl MenuIds {
    /// The action for a click on menu item `id`, or `None` for an item of
    /// another (older) menu.
    fn action(&self, id: &MenuId) -> Option<TrayAction> {
        let action = if *id == self.show {
            TrayAction::Show
        } else if *id == self.quit {
            TrayAction::Quit
        } else if *id == self.clean_gentle {
            TrayAction::Clean(CleanLevel::Gentle)
        } else if *id == self.clean_moderate {
            TrayAction::Clean(CleanLevel::Moderate)
        } else if *id == self.clean_aggressive {
            TrayAction::Clean(CleanLevel::Aggressive)
        } else if *id == self.clean_nuclear {
            TrayAction::Clean(CleanLevel::Nuclear)
        } else if *id == self.nav_dashboard {
            TrayAction::Navigate(Panel::Overview)
        } else if *id == self.nav_monitor {
            TrayAction::Navigate(Panel::Monitor)
        } else if *id == self.nav_processes {
            TrayAction::Navigate(Panel::Processes)
        } else if *id == self.nav_settings {
            TrayAction::Navigate(Panel::Settings)
        } else {
            return None;
        };
        Some(action)
    }
}

/// A tray event as the callbacks forward it.
enum RawEvent {
    /// A menu item was clicked.
    Menu(MenuId),
    /// The icon was left-clicked.
    LeftClick,
}

/// Where the process-wide tray callbacks send events: the live
/// [`TrayHandle`]'s channel and the context to wake. `None` while there is no
/// tray icon.
static SINK: Mutex<Option<(Sender<RawEvent>, egui::Context)>> = Mutex::new(None);

/// Forward `event` to the live tray handle, if any, and wake the UI.
fn forward(event: RawEvent) {
    if let Ok(sink) = SINK.lock()
        && let Some((tx, ctx)) = sink.as_ref()
        && tx.send(event).is_ok()
    {
        ctx.request_repaint();
    }
}

/// Install the tray callbacks, once per process (`tray-icon` keeps the first
/// ones set).
fn install_callbacks() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        MenuEvent::set_event_handler(Some(|event: MenuEvent| forward(RawEvent::Menu(event.id))));
        TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
            // Left-click (button-up) on the icon opens the window.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                forward(RawEvent::LeftClick);
            }
        }));
    });
}

/// Live system-tray icon handle.
///
/// While this value is alive the `MagicX` RAM Cleaner icon appears in the
/// Windows notification area. Dropping the handle removes the icon and stops
/// events reaching it.
pub struct TrayHandle {
    /// The underlying tray icon; kept alive for its [`Drop`] side-effect.
    _icon: TrayIcon,
    /// IDs of this icon's menu items, to decode clicks.
    ids: MenuIds,
    /// Events forwarded by the tray callbacks.
    rx: Receiver<RawEvent>,
}

impl TrayHandle {
    /// Create and register a system-tray icon with its context menu.
    ///
    /// `ctx` is woken (with [`egui::Context::request_repaint`]) for every
    /// tray event, so `logic()` handles it even while the window is hidden.
    ///
    /// `dark` controls the glyph colour in menu icons: white glyphs for
    /// dark menus, charcoal for light menus. The caller should pass the
    /// in-app theme, which **must** match the process-wide menu theme set by
    /// [`crate::platform::window::set_process_dark_mode`].
    ///
    /// # Errors
    ///
    /// Returns an error string on image-decode failure, menu-build failure, or
    /// if the `tray_icon` back-end cannot create the icon.
    pub fn new(ctx: egui::Context, dark: bool) -> Result<Self, String> {
        let icon = load_icon()?;
        let (ids, menu) = build_menu(dark)?;

        let tray = TrayIconBuilder::new()
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_tooltip(strings::tray::TOOLTIP)
            // Left-click restores the window; the menu is for right-click only.
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| format!("Failed to register tray icon: {e}"))?;

        install_callbacks();
        let (tx, rx) = std::sync::mpsc::channel();
        if let Ok(mut sink) = SINK.lock() {
            *sink = Some((tx, ctx));
        }

        Ok(Self {
            _icon: tray,
            ids,
            rx,
        })
    }

    /// Return the next pending [`TrayAction`] without blocking, or [`None`].
    pub fn poll(&self) -> Option<TrayAction> {
        while let Ok(event) = self.rx.try_recv() {
            let action = match event {
                RawEvent::Menu(id) => self.ids.action(&id),
                RawEvent::LeftClick => Some(TrayAction::Show),
            };
            if action.is_some() {
                return action;
            }
        }
        None
    }
}

impl Drop for TrayHandle {
    fn drop(&mut self) {
        // Stop forwarding events here; a replacement icon registers its own.
        if let Ok(mut sink) = SINK.lock() {
            *sink = None;
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
    let show_item = icon_menu_item(strings::tray::OPEN, ph::ROCKET_LAUNCH, dark);
    let quit_item = icon_menu_item(strings::tray::QUIT, ph::POWER, dark);

    let gentle_item = icon_menu_item(strings::levels::GENTLE_NAME, ph::LEAF, dark);
    let moderate_item = icon_menu_item(strings::levels::MODERATE_NAME, ph::LIGHTNING, dark);
    let aggressive_item = icon_menu_item(strings::levels::AGGRESSIVE_NAME, ph::FIRE, dark);
    let nuclear_item = icon_menu_item(strings::levels::NUCLEAR_NAME, ph::RADIOACTIVE, dark);

    let nav_dashboard = icon_menu_item(strings::tray::NAV_OVERVIEW, ph::GAUGE, dark);
    let nav_monitor = icon_menu_item(strings::tray::NAV_MONITOR, ph::ACTIVITY, dark);
    let nav_processes = icon_menu_item(strings::tray::NAV_PROCESSES, ph::CPU, dark);
    let nav_settings = icon_menu_item(strings::tray::NAV_SETTINGS, ph::GEAR, dark);

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
    if let Some(broom) = rasterize_glyph(ph::BROOM, dark) {
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
fn icon_menu_item(label: &str, icon: &str, dark: bool) -> IconMenuItem {
    IconMenuItem::new(label, true, rasterize_glyph(icon, dark), None)
}

/// Rasterize a single Phosphor Regular icon into a 16×16 RGBA
/// [`tray_icon::menu::Icon`].
///
/// Uses `ab_glyph` (already a transitive dependency of `epaint`) to render
/// the glyph from the app's Phosphor icon subset.  Returns `None` on any
/// failure so callers degrade gracefully to text-only menu items.
///
/// When `dark` is `true` the glyph is rendered white (for dark OS menu
/// backgrounds); when `false` it is rendered in a dark charcoal colour so
/// it remains visible on light menu backgrounds.
fn rasterize_glyph(icon: &str, dark: bool) -> Option<Icon> {
    let font = FontRef::try_from_slice(&ph::FONT).ok()?;

    let glyph_id = font.glyph_id(icon.chars().next()?);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The tray menu renders its icons from the icon subset with `ab_glyph`,
    /// so the subset must parse and hold every glyph the menu asks for.
    #[test]
    fn every_tray_icon_rasterizes_from_the_subset() {
        for icon in [
            ph::ROCKET_LAUNCH,
            ph::POWER,
            ph::BROOM,
            ph::LEAF,
            ph::LIGHTNING,
            ph::FIRE,
            ph::RADIOACTIVE,
            ph::GAUGE,
            ph::ACTIVITY,
            ph::CPU,
            ph::GEAR,
        ] {
            for dark in [true, false] {
                assert!(
                    rasterize_glyph(icon, dark).is_some(),
                    "{icon:?} did not render"
                );
            }
        }
    }
}
