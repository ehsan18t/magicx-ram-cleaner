//! # Settings Panel
//!
//! Preferences laid out like Windows Settings: grouped cards of rows, each
//! with an icon, a title, a description and its control on the right.
//! Covers the theme, Windows integration (tray, autostart, Desktop context
//! menu) and backing settings up to a file.

use std::time::{Duration, Instant};

use eframe::egui;

use crate::gui::icons::regular as ph;
use crate::strings::gui::settings as text;

use super::super::app::MagicXApp;
use super::super::persistence::SettingsManager;
use super::super::settings::ThemeMode;
use super::super::theme::{self, Palette};
use super::super::widgets;

/// How long a status message stays under the cards.
const STATUS_SECS: u64 = 8;

/// Draw the settings panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let p = theme::palette();
    widgets::page_title(ui, text::TITLE);

    widgets::section_header(ui, text::SECTION_APPEARANCE);
    widgets::card(ui, app.dark(), |ui| draw_theme_row(ui, app));

    ui.add_space(theme::SECTION_SPACING);
    widgets::section_header(ui, text::SECTION_INTEGRATION);
    widgets::card(ui, app.dark(), |ui| {
        draw_tray_row(ui, app, &p);
        widgets::divider(ui);
        draw_autostart_row(ui, app);
        widgets::divider(ui);
        draw_context_menu_row(ui, app);
    });

    ui.add_space(theme::SECTION_SPACING);
    widgets::section_header(ui, text::SECTION_BACKUP);
    widgets::card(ui, app.dark(), |ui| draw_backup_row(ui, app));

    draw_status(ui, app, &p);
}

/// The theme choice.
fn draw_theme_row(ui: &mut egui::Ui, app: &mut MagicXApp) {
    const MODES: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];
    widgets::settings_row(ui, ph::PALETTE, text::LABEL_THEME, text::DESC_THEME, |ui| {
        ui.allocate_ui(egui::vec2(240.0, theme::CONTROL_HEIGHT), |ui| {
            let selected = MODES
                .iter()
                .position(|m| *m == app.settings.theme)
                .unwrap_or(0);
            let labels = [text::THEME_SYSTEM, text::THEME_LIGHT, text::THEME_DARK];
            if let Some(i) = widgets::segmented(ui, &labels, selected, true) {
                app.settings.theme = MODES[i];
            }
        });
    });
}

/// Minimize to tray on close, with the reason when the tray icon failed.
fn draw_tray_row(ui: &mut egui::Ui, app: &mut MagicXApp, p: &Palette) {
    widgets::settings_row(
        ui,
        ph::TRAY,
        text::LABEL_MINIMIZE_TO_TRAY,
        text::DESC_MINIMIZE_TO_TRAY,
        |ui| {
            widgets::toggle_switch(ui, &mut app.settings.minimize_to_tray);
        },
    );
    // Tray icon creation can fail (e.g. Explorer not running); without an
    // icon the close button quits instead of hiding, so say why.
    if app.settings.minimize_to_tray
        && let Some(err) = &app.tray_error
    {
        ui.label(
            egui::RichText::new(format!(
                "The tray icon couldn\u{2019}t be created, so closing quits the app: {err}"
            ))
            .size(theme::CAPTION)
            .color(p.critical),
        );
        ui.add_space(6.0);
    }
}

/// Start with Windows, synced to the logon task the moment it is flipped.
fn draw_autostart_row(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let mut wanted = app.settings.auto_start;
    let mut changed = false;
    widgets::settings_row(
        ui,
        ph::ROCKET_LAUNCH,
        text::LABEL_AUTOSTART,
        text::DESC_AUTOSTART,
        |ui| {
            changed = widgets::toggle_switch(ui, &mut wanted).changed();
        },
    );
    if !changed {
        return;
    }
    match crate::integration::autostart::set_enabled(wanted).map_err(|e| format!("{e:#}")) {
        Ok(()) => {
            app.settings.auto_start = wanted;
            let msg = if wanted {
                text::MSG_AUTOSTART_ON
            } else {
                text::MSG_AUTOSTART_OFF
            };
            set_status(app, msg.to_owned(), false);
        }
        // The switch stays where it was, so it keeps telling the truth.
        Err(e) => set_status(app, format!("Couldn\u{2019}t change autostart: {e}"), true),
    }
}

/// Desktop context menu: a switch that installs or removes it.
fn draw_context_menu_row(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let installed = app.context_menu_installed;
    let mut wanted = installed;
    let mut changed = false;
    widgets::settings_row(
        ui,
        ph::MOUSE_RIGHT_CLICK,
        text::LABEL_CONTEXT_MENU,
        text::DESC_CONTEXT_MENU,
        |ui| {
            let tooltip = if installed {
                text::TOOLTIP_REMOVE
            } else {
                text::TOOLTIP_INSTALL
            };
            changed = widgets::toggle_switch(ui, &mut wanted)
                .on_hover_text(tooltip)
                .changed();
        },
    );
    // The switch reflects the registry, so it only moves once the change
    // succeeds.
    if !changed {
        return;
    }
    if installed {
        match crate::integration::context_menu::uninstall() {
            Ok(_) => {
                app.context_menu_installed = false;
                set_status(app, text::MSG_CTX_REMOVED.to_owned(), false);
            }
            Err(e) => set_status(
                app,
                format!("Couldn\u{2019}t remove the context menu: {e:#}"),
                true,
            ),
        }
    } else {
        match crate::integration::context_menu::current_exe_path()
            .and_then(|path| crate::integration::context_menu::install(&path))
        {
            Ok(()) => {
                app.context_menu_installed = true;
                set_status(app, text::MSG_CTX_INSTALLED.to_owned(), false);
            }
            Err(e) => set_status(
                app,
                format!("Couldn\u{2019}t install the context menu: {e:#}"),
                true,
            ),
        }
    }
}

/// Export and import the settings file.
fn draw_backup_row(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let mut export = false;
    let mut import = false;
    widgets::settings_row(
        ui,
        ph::FLOPPY_DISK,
        text::LABEL_BACKUP,
        text::DESC_BACKUP,
        |ui| {
            import = widgets::secondary_button(ui, text::BTN_IMPORT)
                .on_hover_text(text::TOOLTIP_IMPORT)
                .clicked();
            export = widgets::secondary_button(ui, text::BTN_EXPORT)
                .on_hover_text(text::TOOLTIP_EXPORT)
                .clicked();
        },
    );
    if export {
        export_settings(app);
    }
    if import {
        import_settings(app);
    }
}

/// The outcome of the last action, under the cards, for a few seconds.
fn draw_status(ui: &mut egui::Ui, app: &mut MagicXApp, p: &Palette) {
    if let Some((_, _, shown_at)) = app.settings_status
        && shown_at.elapsed() > Duration::from_secs(STATUS_SECS)
    {
        app.settings_status = None;
    }
    if let Some((msg, is_err, _)) = &app.settings_status {
        ui.add_space(12.0);
        let (icon, color) = if *is_err {
            (ph::WARNING_CIRCLE, p.critical)
        } else {
            (ph::CHECK, p.success)
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(icon).color(color));
            ui.add(egui::Label::new(egui::RichText::new(msg).color(p.text)).wrap());
        });
        ui.ctx().request_repaint_after(Duration::from_secs(1));
    }
}

/// Show `msg` under the cards.
fn set_status(app: &mut MagicXApp, msg: String, is_err: bool) {
    app.settings_status = Some((msg, is_err, Instant::now()));
}

/// Export settings to a user-chosen file.
fn export_settings(app: &mut MagicXApp) {
    match SettingsManager::export(app.hwnd(), &app.settings) {
        Ok(Some(path)) => {
            let name = path.file_name().map_or_else(
                || path.to_string_lossy().into_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
            set_status(
                app,
                format!("Settings exported to \u{201c}{name}\u{201d}"),
                false,
            );
        }
        Ok(None) => {} // cancelled
        Err(e) => set_status(app, format!("Couldn\u{2019}t export settings: {e}"), true),
    }
}

/// Import settings from a user-chosen file and apply them.
///
/// Also syncs the autostart task and the monitor state, which only follow
/// direct UI toggles otherwise.
fn import_settings(app: &mut MagicXApp) {
    match SettingsManager::import(app.hwnd()) {
        Ok(Some(new_settings)) => {
            let sync = crate::integration::autostart::set_enabled(new_settings.auto_start)
                .map_err(|e| format!("{e:#}"));
            app.settings = new_settings;
            app.monitor_active = app.settings.auto_clean_enabled;
            match sync {
                Ok(()) => set_status(app, text::MSG_IMPORT_OK.to_owned(), false),
                Err(e) => {
                    // Keep the switch truthful about the task.
                    app.settings.auto_start = crate::integration::autostart::is_enabled();
                    set_status(
                        app,
                        format!("Settings imported, but autostart couldn\u{2019}t be changed: {e}"),
                        true,
                    );
                }
            }
        }
        Ok(None) => {} // cancelled
        Err(e) => set_status(app, format!("Couldn\u{2019}t import settings: {e}"), true),
    }
}
