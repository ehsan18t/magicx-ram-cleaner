//! # Settings Panel
//!
//! User preferences: appearance (dark/light theme), integration toggles
//! (minimize-to-tray, autostart, Desktop context menu), display options, and settings backup.

use crate::gui::icons::regular as ph;
use eframe::egui;

use super::super::app::MagicXApp;
use super::super::settings::ThemeMode;
use super::super::persistence::SettingsManager;
use super::super::{theme, widgets};

use crate::strings;

/// Draw the settings panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    widgets::page_title(ui, strings::gui::settings::TITLE);

    draw_appearance(ui, app);
    ui.add_space(theme::SECTION_SPACING);
    draw_integration(ui, app);
    ui.add_space(theme::SECTION_SPACING);
    draw_context_menu(ui, app);
    ui.add_space(theme::SECTION_SPACING);
    draw_backup(ui, app);
}

/// Appearance section: the theme choice.
fn draw_appearance(ui: &mut egui::Ui, app: &mut MagicXApp) {
    const MODES: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];

    widgets::card(ui, app.dark(), |ui| {
        widgets::settings_row(
            ui,
            ph::PALETTE,
            strings::gui::settings::LABEL_THEME,
            strings::gui::settings::DESC_THEME,
            |ui| {
                ui.allocate_ui(egui::vec2(240.0, theme::CONTROL_HEIGHT), |ui| {
                    let selected = MODES
                        .iter()
                        .position(|m| *m == app.settings.theme)
                        .unwrap_or(0);
                    if let Some(i) = widgets::segmented(
                        ui,
                        &[
                            strings::gui::settings::THEME_SYSTEM,
                            strings::gui::settings::THEME_LIGHT,
                            strings::gui::settings::THEME_DARK,
                        ],
                        selected,
                        true,
                    ) {
                        app.settings.theme = MODES[i];
                    }
                });
            },
        );
    });
}

/// Integration section: tray and autostart toggles.
fn draw_integration(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.dark();

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

        // Tray icon creation can fail (e.g. Explorer not running); without
        // an icon the close button quits instead of hiding, so say why.
        if app.settings.minimize_to_tray
            && let Some(err) = &app.tray_error
        {
            ui.label(
                egui::RichText::new(format!("Tray icon unavailable: {err}"))
                    .size(10.0)
                    .color(theme::red()),
            );
        }

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
            match crate::integration::autostart::set_enabled(app.settings.auto_start)
                .map_err(|e| format!("{e:#}"))
            {
                Ok(()) => {
                    app.settings_status = Some((
                        if app.settings.auto_start {
                            "Autostart enabled: logon task created in Task Scheduler".to_owned()
                        } else {
                            "Autostart disabled: logon task removed from Task Scheduler".to_owned()
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
    let dark = app.dark();

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
            (strings::gui::settings::STATUS_INSTALLED, theme::green())
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
                theme::accent()
            }),
    )
    .min_size(egui::vec2(110.0, 30.0))
    .corner_radius(egui::CornerRadius::same(6))
    .fill(if app.context_menu_installed {
        theme::surface_color(dark)
    } else {
        theme::accent().gamma_multiply(0.12)
    });

    if ui
        .add_enabled(!app.context_menu_installed, btn)
        .on_hover_text(strings::gui::settings::TOOLTIP_INSTALL)
        .clicked()
    {
        match crate::integration::context_menu::current_exe_path()
            .and_then(|p| crate::integration::context_menu::install(&p))
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
                theme::red()
            } else {
                theme::muted_color(dark)
            }),
    )
    .min_size(egui::vec2(110.0, 30.0))
    .corner_radius(egui::CornerRadius::same(6))
    .fill(if app.context_menu_installed {
        theme::red().gamma_multiply(0.12)
    } else {
        theme::surface_color(dark)
    });

    if ui
        .add_enabled(app.context_menu_installed, btn)
        .on_hover_text(strings::gui::settings::TOOLTIP_REMOVE)
        .clicked()
    {
        match crate::integration::context_menu::uninstall() {
            Ok(_) => {
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

/// Backup section: export and import settings.
fn draw_backup(ui: &mut egui::Ui, app: &mut MagicXApp) {
    use std::time::Duration;

    let dark = app.dark();

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
                    .color(theme::accent()),
            )
            .min_size(egui::vec2(110.0, 30.0))
            .corner_radius(egui::CornerRadius::same(6))
            .fill(theme::accent().gamma_multiply(0.12));

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
                    .color(theme::accent()),
            )
            .min_size(egui::vec2(110.0, 30.0))
            .corner_radius(egui::CornerRadius::same(6))
            .fill(theme::accent().gamma_multiply(0.12));

            if ui
                .add(import_btn)
                .on_hover_text(strings::gui::settings::TOOLTIP_IMPORT)
                .clicked()
            {
                import_settings(app);
            }
        });

        // ── Feedback banner ──────────────────────────────────────────
        if let Some((ref msg, is_err, _)) = app.settings_status {
            ui.add_space(8.0);
            let color = if is_err { theme::red() } else { theme::green() };
            ui.label(egui::RichText::new(msg.as_str()).size(11.0).color(color));
        }
    });
}

/// Import settings from a user-chosen file and apply them.
///
/// Also syncs the autostart task and the monitor state, which only follow
/// direct UI toggles otherwise, and reports the outcome in the status banner.
fn import_settings(app: &mut MagicXApp) {
    match SettingsManager::import() {
        Ok(Some(new_settings)) => {
            let sync = crate::integration::autostart::set_enabled(new_settings.auto_start)
                .map_err(|e| format!("{e:#}"));
            app.settings = new_settings;
            app.monitor_active = app.settings.auto_clean_enabled;
            app.settings_status = Some(match sync {
                Ok(()) => (
                    strings::gui::settings::MSG_IMPORT_OK.to_owned(),
                    false,
                    std::time::Instant::now(),
                ),
                Err(e) => {
                    // Keep the checkbox truthful about the task.
                    app.settings.auto_start = crate::integration::autostart::is_enabled();
                    (
                        format!("Settings imported, but autostart sync failed: {e}"),
                        true,
                        std::time::Instant::now(),
                    )
                }
            });
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
