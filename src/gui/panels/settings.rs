//! # Settings Panel
//!
//! User preferences: appearance (dark/light theme), integration toggles
//! (minimize-to-tray, autostart, Desktop context menu), display options, and settings backup.

use eframe::egui;
use egui_phosphor::regular as ph;

use super::super::app::MagicXApp;
use super::super::persistence::SettingsManager;
use super::super::{theme, widgets};

use crate::strings;

/// Draw the settings panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;
    widgets::page_title(ui, ph::GEAR, strings::gui::settings::TITLE, dark);

    draw_appearance(ui, app);
    ui.add_space(theme::SECTION_SPACING);
    draw_integration(ui, app);
    ui.add_space(theme::SECTION_SPACING);
    draw_context_menu(ui, app);
    ui.add_space(theme::SECTION_SPACING);
    draw_display(ui, app);
    ui.add_space(theme::SECTION_SPACING);
    draw_backup(ui, app);
}

/// Appearance section: theme toggle buttons.
fn draw_appearance(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;

    widgets::card(ui, dark, |ui| {
        widgets::section_header(ui, strings::gui::settings::SECTION_APPEARANCE);

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(strings::gui::settings::LABEL_THEME)
                    .size(12.0)
                    .color(theme::text_color(dark)),
            );
            ui.add_space(8.0);

            let dark_btn = egui::Button::new(
                egui::RichText::new(strings::gui::settings::THEME_DARK)
                    .size(12.0)
                    .color(if dark {
                        theme::ACCENT
                    } else {
                        theme::muted_color(dark)
                    }),
            )
            .min_size(egui::vec2(60.0, 28.0))
            .corner_radius(egui::CornerRadius::same(6))
            .fill(if dark {
                theme::ACCENT.gamma_multiply(0.15)
            } else {
                theme::surface_color(dark)
            });

            if ui.add(dark_btn).clicked() {
                app.settings.dark_mode = true;
            }

            ui.add_space(4.0);

            let light_btn = egui::Button::new(
                egui::RichText::new(strings::gui::settings::THEME_LIGHT)
                    .size(12.0)
                    .color(if dark {
                        theme::muted_color(dark)
                    } else {
                        theme::ACCENT
                    }),
            )
            .min_size(egui::vec2(60.0, 28.0))
            .corner_radius(egui::CornerRadius::same(6))
            .fill(if dark {
                theme::surface_color(dark)
            } else {
                theme::ACCENT.gamma_multiply(0.15)
            });

            if ui.add(light_btn).clicked() {
                app.settings.dark_mode = false;
            }
        });
    });
}

/// Integration section: tray and autostart toggles.
fn draw_integration(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;

    widgets::card(ui, dark, |ui| {
        widgets::section_header(ui, strings::gui::settings::SECTION_INTEGRATION);

        // ── Minimize to Tray ──────────────────────────────────────
        ui.horizontal(|ui| {
            ui.checkbox(&mut app.settings.minimize_to_tray, "");
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(strings::gui::settings::LABEL_MINIMIZE_TO_TRAY)
                        .size(12.0)
                        .color(theme::text_color(dark)),
                );
                ui.label(
                    egui::RichText::new(strings::gui::settings::DESC_MINIMIZE_TO_TRAY)
                        .size(10.0)
                        .color(theme::muted_color(dark)),
                );
            });
        });

        ui.add_space(6.0);

        // ── Launch at Startup ─────────────────────────────────────
        let prev_auto_start = app.settings.auto_start;
        ui.horizontal(|ui| {
            ui.checkbox(&mut app.settings.auto_start, "");
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(strings::gui::settings::LABEL_AUTOSTART)
                        .size(12.0)
                        .color(theme::text_color(dark)),
                );
                ui.label(
                    egui::RichText::new(strings::gui::settings::DESC_AUTOSTART)
                        .size(10.0)
                        .color(theme::muted_color(dark)),
                );
            });
        });

        // Immediate registry sync when the user toggles autostart.
        if app.settings.auto_start != prev_auto_start {
            match SettingsManager::set_autostart(app.settings.auto_start) {
                Ok(()) => {
                    app.settings_status = Some((
                        if app.settings.auto_start {
                            "Autostart enabled — registered in HKCU\\Run".to_owned()
                        } else {
                            "Autostart disabled — removed from HKCU\\Run".to_owned()
                        },
                        false,
                        std::time::Instant::now(),
                    ));
                }
                Err(e) => {
                    // Roll back the toggle so the checkbox reflects reality.
                    app.settings.auto_start = prev_auto_start;
                    app.settings_status = Some((
                        format!("Autostart change failed: {e}"),
                        true,
                        std::time::Instant::now(),
                    ));
                }
            }
        }
    });
}

/// Desktop context menu section: install / remove the Windows Explorer integration.
///
/// Writes (or deletes) cascading submenu entries under both
/// `HKCR\DesktopBackground\Shell` and `HKCR\Directory\Background\Shell` so the
/// `MagicX RAM Cleaner` submenu appears when right-clicking the Desktop or any
/// folder background.  Requires administrator rights (already enforced by
/// [`crate::gui::run_gui`]).
fn draw_context_menu(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;

    widgets::card(ui, dark, |ui| {
        widgets::section_header(ui, strings::gui::settings::SECTION_CONTEXT_MENU);

        ui.label(
            egui::RichText::new(strings::gui::settings::DESC_CONTEXT_MENU)
                .size(11.0)
                .color(theme::muted_color(dark)),
        );
        ui.add_space(8.0);

        // Status badge
        let (status_text, status_color) = if app.context_menu_installed {
            (strings::gui::settings::STATUS_INSTALLED, theme::GREEN)
        } else {
            (
                strings::gui::settings::STATUS_NOT_INSTALLED,
                theme::muted_color(dark),
            )
        };
        ui.label(
            egui::RichText::new(status_text)
                .size(11.0)
                .color(status_color),
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            draw_context_menu_install_btn(ui, dark, app);
            ui.add_space(8.0);
            draw_context_menu_remove_btn(ui, dark, app);
        });
    });
}

/// "Install" button for the Desktop context menu card.
fn draw_context_menu_install_btn(ui: &mut egui::Ui, dark: bool, app: &mut MagicXApp) {
    let btn = egui::Button::new(
        egui::RichText::new(format!("{} Install", ph::PLUG))
            .size(12.0)
            .color(if app.context_menu_installed {
                theme::muted_color(dark)
            } else {
                theme::ACCENT
            }),
    )
    .min_size(egui::vec2(110.0, 30.0))
    .corner_radius(egui::CornerRadius::same(6))
    .fill(if app.context_menu_installed {
        theme::surface_color(dark)
    } else {
        theme::ACCENT.gamma_multiply(0.12)
    });

    if ui
        .add_enabled(!app.context_menu_installed, btn)
        .on_hover_text(strings::gui::settings::TOOLTIP_INSTALL)
        .clicked()
    {
        match crate::context_menu::current_exe_path().and_then(|p| crate::context_menu::install(&p))
        {
            Ok(()) => {
                app.context_menu_installed = true;
                app.settings_status = Some((
                    strings::gui::settings::MSG_CTX_INSTALLED.to_owned(),
                    false,
                    std::time::Instant::now(),
                ));
            }
            Err(e) => {
                app.settings_status = Some((
                    format!("Install failed: {e:#}"),
                    true,
                    std::time::Instant::now(),
                ));
            }
        }
    }
}

/// "Remove" button for the Desktop context menu card.
fn draw_context_menu_remove_btn(ui: &mut egui::Ui, dark: bool, app: &mut MagicXApp) {
    let btn = egui::Button::new(
        egui::RichText::new(format!("{} Remove", ph::PLUG_CHARGING))
            .size(12.0)
            .color(if app.context_menu_installed {
                theme::RED
            } else {
                theme::muted_color(dark)
            }),
    )
    .min_size(egui::vec2(110.0, 30.0))
    .corner_radius(egui::CornerRadius::same(6))
    .fill(if app.context_menu_installed {
        theme::RED.gamma_multiply(0.12)
    } else {
        theme::surface_color(dark)
    });

    if ui
        .add_enabled(app.context_menu_installed, btn)
        .on_hover_text(strings::gui::settings::TOOLTIP_REMOVE)
        .clicked()
    {
        match crate::context_menu::uninstall() {
            Ok(()) => {
                app.context_menu_installed = false;
                app.settings_status = Some((
                    strings::gui::settings::MSG_CTX_REMOVED.to_owned(),
                    false,
                    std::time::Instant::now(),
                ));
            }
            Err(e) => {
                app.settings_status = Some((
                    format!("Removal failed: {e:#}"),
                    true,
                    std::time::Instant::now(),
                ));
            }
        }
    }
}

/// Preferences section: tooltip visibility.
fn draw_display(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;

    widgets::card(ui, dark, |ui| {
        widgets::section_header(ui, strings::gui::settings::SECTION_PREFERENCES);

        ui.horizontal(|ui| {
            ui.checkbox(&mut app.settings.show_level_tooltips, "");
            ui.label(
                egui::RichText::new(strings::gui::settings::LABEL_TOOLTIPS)
                    .size(12.0)
                    .color(theme::text_color(dark)),
            );
        });
    });
}

/// Backup section: export and import settings.
fn draw_backup(ui: &mut egui::Ui, app: &mut MagicXApp) {
    use std::time::Duration;

    let dark = app.settings.dark_mode;

    // Auto-dismiss stale feedback before rendering so the card never shows
    // an outdated message on re-entry.
    if let Some((_, _, shown_at)) = app.settings_status
        && shown_at.elapsed() > Duration::from_secs(8)
    {
        app.settings_status = None;
    }

    widgets::card(ui, dark, |ui| {
        widgets::section_header(ui, strings::gui::settings::SECTION_BACKUP);

        ui.label(
            egui::RichText::new(strings::gui::settings::DESC_BACKUP)
                .size(11.0)
                .color(theme::muted_color(dark)),
        );
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            // ── Export ───────────────────────────────────────────────
            let export_btn = egui::Button::new(
                egui::RichText::new(format!("{} Export", ph::UPLOAD_SIMPLE))
                    .size(12.0)
                    .color(theme::ACCENT),
            )
            .min_size(egui::vec2(110.0, 30.0))
            .corner_radius(egui::CornerRadius::same(6))
            .fill(theme::ACCENT.gamma_multiply(0.12));

            if ui
                .add(export_btn)
                .on_hover_text(strings::gui::settings::TOOLTIP_EXPORT)
                .clicked()
            {
                match SettingsManager::export(&app.settings) {
                    Ok(Some(path)) => {
                        let name = path.file_name().map_or_else(
                            || path.to_string_lossy().into_owned(),
                            |n| n.to_string_lossy().into_owned(),
                        );
                        app.settings_status = Some((
                            format!("Exported to \u{201c}{name}\u{201d}"),
                            false,
                            std::time::Instant::now(),
                        ));
                    }
                    Ok(None) => {} // user cancelled
                    Err(e) => {
                        app.settings_status = Some((
                            format!("Export failed: {e}"),
                            true,
                            std::time::Instant::now(),
                        ));
                    }
                }
            }

            ui.add_space(8.0);

            // ── Import ───────────────────────────────────────────────
            let import_btn = egui::Button::new(
                egui::RichText::new(format!("{} Import", ph::DOWNLOAD_SIMPLE))
                    .size(12.0)
                    .color(theme::ACCENT),
            )
            .min_size(egui::vec2(110.0, 30.0))
            .corner_radius(egui::CornerRadius::same(6))
            .fill(theme::ACCENT.gamma_multiply(0.12));

            if ui
                .add(import_btn)
                .on_hover_text(strings::gui::settings::TOOLTIP_IMPORT)
                .clicked()
            {
                match SettingsManager::import() {
                    Ok(Some(new_settings)) => {
                        app.settings = new_settings;
                        app.settings_status = Some((
                            strings::gui::settings::MSG_IMPORT_OK.to_owned(),
                            false,
                            std::time::Instant::now(),
                        ));
                    }
                    Ok(None) => {} // user cancelled
                    Err(e) => {
                        app.settings_status = Some((
                            format!("Import failed: {e}"),
                            true,
                            std::time::Instant::now(),
                        ));
                    }
                }
            }
        });

        // ── Feedback banner ──────────────────────────────────────────
        if let Some((ref msg, is_err, _)) = app.settings_status {
            ui.add_space(8.0);
            let color = if is_err { theme::RED } else { theme::GREEN };
            ui.label(egui::RichText::new(msg.as_str()).size(11.0).color(color));
        }
    });
}
