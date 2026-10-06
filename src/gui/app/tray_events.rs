//! Reacting to tray icon events, and rebuilding the tray icon.

use eframe::egui;

use super::{MagicXApp, Panel};
use crate::gui::tray;

impl MagicXApp {
    /// Poll the tray icon event queues and dispatch every pending [`TrayAction`].
    ///
    /// Called every frame from [`eframe::App::logic`].
    pub(super) fn poll_tray_events(&mut self, ctx: &egui::Context) {
        while let Some(action) = self.tray_handle.as_ref().and_then(tray::TrayHandle::poll) {
            self.handle_tray_action(ctx, &action);
        }
    }

    /// Apply a single [`TrayAction`] received from the tray watcher thread.
    pub(super) fn handle_tray_action(&mut self, ctx: &egui::Context, action: &tray::TrayAction) {
        match *action {
            tray::TrayAction::Show => {
                crate::platform::window::uncloak_window(self.hwnd);
                self.hidden_to_tray = false;
            }
            tray::TrayAction::Clean(level) => {
                crate::platform::window::uncloak_window(self.hwnd);
                self.hidden_to_tray = false;
                self.active_panel = Panel::Dashboard;
                self.start_clean(level);
            }
            tray::TrayAction::Navigate(panel) => {
                crate::platform::window::uncloak_window(self.hwnd);
                self.hidden_to_tray = false;
                self.active_panel = panel;
            }
            tray::TrayAction::Quit => {
                self.quit_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// (Re)create the tray icon for the current theme.
    ///
    /// The old handle is dropped first so its icon and watcher thread are
    /// gone before the new ones register. A failure is kept in
    /// [`Self::tray_error`] for the Settings panel.
    pub(super) fn rebuild_tray(&mut self, ctx: &egui::Context) {
        self.tray_handle = None;
        match tray::TrayHandle::new(ctx.clone(), self.hwnd, self.settings.dark_mode) {
            Ok(handle) => {
                self.tray_handle = Some(handle);
                self.tray_error = None;
            }
            Err(e) => self.tray_error = Some(e),
        }
    }
}
