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

    /// Apply a single [`TrayAction`] from the tray icon or its menu.
    pub(super) fn handle_tray_action(&mut self, ctx: &egui::Context, action: &tray::TrayAction) {
        match *action {
            tray::TrayAction::Show => self.show_window(ctx),
            tray::TrayAction::Clean(level) => {
                self.show_window(ctx);
                self.active_panel = Panel::Overview;
                self.start_clean(level);
            }
            tray::TrayAction::Navigate(panel) => {
                self.show_window(ctx);
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
    /// The old handle is dropped first so its icon is gone and stops
    /// receiving events before the new one registers. A failure is kept in
    /// [`Self::tray_error`] for the Settings panel.
    pub(super) fn rebuild_tray(&mut self, ctx: &egui::Context) {
        self.tray_handle = None;
        match tray::TrayHandle::new(ctx.clone(), self.dark()) {
            Ok(handle) => {
                self.tray_handle = Some(handle);
                self.tray_error = None;
            }
            Err(e) => self.tray_error = Some(e),
        }
    }
}
