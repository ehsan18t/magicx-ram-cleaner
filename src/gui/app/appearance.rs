//! Following the Windows appearance: the light/dark app theme (when the user
//! picked System) and the accent colour.
//!
//! Both are re-read every [`POLL_INTERVAL`] while the window is visible, so a
//! change in Windows Settings shows up within a couple of seconds without a
//! restart. Nothing is polled while the window is hidden.

use std::time::{Duration, Instant};

use eframe::egui;

use super::MagicXApp;
use crate::gui::settings::ThemeMode;
use crate::gui::theme::{self, Palette};
use crate::platform::appearance as system;

/// How often the Windows theme and accent are re-read while visible.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// The Windows appearance as last read, and the palette built from it.
pub(super) struct Appearance {
    /// The theme mode the palette was built for.
    mode: ThemeMode,
    /// The palette currently applied.
    pub palette: Palette,
    /// When the Windows settings were last read.
    last_read: Instant,
}

impl Appearance {
    /// Read the Windows appearance and build the palette for `mode`.
    pub fn read(mode: ThemeMode) -> Self {
        let system_light = system::apps_use_light_theme().unwrap_or(false);
        let accent = system::accent_palette().unwrap_or(theme::DEFAULT_ACCENT);
        Self {
            mode,
            palette: Palette::new(mode.is_dark(system_light), &accent),
            last_read: Instant::now(),
        }
    }
}

impl MagicXApp {
    /// Whether the app currently shows the dark theme.
    #[must_use]
    pub const fn dark(&self) -> bool {
        self.appearance.palette.dark
    }

    /// Re-read the Windows appearance when due (or when the theme setting
    /// changed) and apply any change: palette, title bar, native menu theme
    /// and tray menu glyphs.
    pub(super) fn refresh_appearance(&mut self, ctx: &egui::Context) {
        let mode_changed = self.settings.theme != self.appearance.mode;
        if !mode_changed && self.appearance.last_read.elapsed() < POLL_INTERVAL {
            return;
        }
        let was_dark = self.dark();
        let previous = self.appearance.palette;
        self.appearance = Appearance::read(self.settings.theme);
        if self.appearance.palette != previous {
            self.apply_appearance(ctx);
            if self.dark() != was_dark && self.settings.minimize_to_tray {
                self.rebuild_tray(ctx);
            }
        }
    }

    /// Push the current palette to egui and the native window.
    pub(super) fn apply_appearance(&self, ctx: &egui::Context) {
        let palette = self.appearance.palette;
        theme::apply(ctx, palette);
        crate::platform::window::set_process_dark_mode(palette.dark);
        crate::platform::window::set_title_bar_dark_mode(self.hwnd, palette.dark);
        crate::platform::window::set_caption_color(self.hwnd, palette.bg_rgb());
    }
}
