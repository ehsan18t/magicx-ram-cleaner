//! # Settings Panel
//!
//! User preferences: appearance (dark/light theme), integration toggles
//! (minimize-to-tray, autostart), display options, and settings backup.

use eframe::egui;
use egui_phosphor::regular as ph;

use super::super::app::MagicXApp;
use super::super::persistence::SettingsManager;
use super::super::{theme, widgets};

/// Draw the settings panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;
    widgets::page_title(ui, ph::GEAR, "Settings", dark);

    draw_appearance(ui, app);
    ui.add_space(theme::SECTION_SPACING);
    draw_integration(ui, app);
    ui.add_space(theme::SECTION_SPACING);
    draw_display(ui, app);
    ui.add_space(theme::SECTION_SPACING);
    draw_backup(ui, app);
}

/// Appearance section: theme toggle buttons.
fn draw_appearance(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;

    widgets::card(ui, dark, |ui| {
        widgets::section_header(ui, "Appearance");

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Theme:")
                    .size(12.0)
                    .color(theme::text_color(dark)),
            );
            ui.add_space(8.0);

            let dark_btn =
                egui::Button::new(egui::RichText::new("Dark").size(12.0).color(if dark {
                    theme::ACCENT
                } else {
                    theme::muted_color(dark)
                }))
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

            let light_btn =
                egui::Button::new(egui::RichText::new("Light").size(12.0).color(if dark {
                    theme::muted_color(dark)
                } else {
                    theme::ACCENT
                }))
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
        widgets::section_header(ui, "Integration");

        // ── Minimize to Tray ──────────────────────────────────────
        ui.horizontal(|ui| {
            ui.checkbox(&mut app.settings.minimize_to_tray, "");
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("Minimize to Tray on Close")
                        .size(12.0)
                        .color(theme::text_color(dark)),
                );
                ui.label(
                    egui::RichText::new(
                        "Clicking \u{00d7} hides to the notification area instead of quitting",
                    )
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
                    egui::RichText::new("Launch at Windows Startup")
                        .size(12.0)
                        .color(theme::text_color(dark)),
                );
                ui.label(
                    egui::RichText::new("Registers in HKCU\\Run for the current user")
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

/// Preferences section: tooltip visibility.
fn draw_display(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;

    widgets::card(ui, dark, |ui| {
        widgets::section_header(ui, "Preferences");

        ui.horizontal(|ui| {
            ui.checkbox(&mut app.settings.show_level_tooltips, "");
            ui.label(
                egui::RichText::new("Show clean-level tooltips on hover")
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
        widgets::section_header(ui, "Backup & Restore");

        ui.label(
            egui::RichText::new(
                "Export your settings to a JSON file or restore from a previous backup.",
            )
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
                .on_hover_text("Save all settings to a JSON file")
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
                    .color(theme::text_color(dark)),
            )
            .min_size(egui::vec2(110.0, 30.0))
            .corner_radius(egui::CornerRadius::same(6))
            .fill(theme::surface_color(dark));

            if ui
                .add(import_btn)
                .on_hover_text("Load settings from a JSON backup file")
                .clicked()
            {
                match SettingsManager::import() {
                    Ok(Some(new_settings)) => {
                        app.settings = new_settings;
                        app.settings_status = Some((
                            "Settings imported successfully".to_owned(),
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
            let color = if is_err {
                egui::Color32::from_rgb(220, 80, 80)
            } else {
                egui::Color32::from_rgb(80, 190, 110)
            };
            ui.label(egui::RichText::new(msg.as_str()).size(11.0).color(color));
        }
    });
}
