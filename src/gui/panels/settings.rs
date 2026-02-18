//! # Settings Panel
//!
//! User preferences: appearance (dark/light theme) and integration toggles
//! (tray icon, context menu).

use eframe::egui;
use egui_phosphor::regular as ph;

use super::super::app::MagicXApp;
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

/// Integration section: tray and context menu toggles.
fn draw_integration(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;

    widgets::card(ui, dark, |ui| {
        widgets::section_header(ui, "Integration");

        ui.horizontal(|ui| {
            ui.checkbox(&mut app.settings.tray_enabled, "");
            ui.label(
                egui::RichText::new("System tray icon")
                    .size(12.0)
                    .color(theme::text_color(dark)),
            );
            ui.label(
                egui::RichText::new("(not yet implemented)")
                    .size(10.0)
                    .color(theme::muted_color(dark)),
            );
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.checkbox(&mut app.settings.context_menu_enabled, "");
            ui.label(
                egui::RichText::new("Desktop context menu")
                    .size(12.0)
                    .color(theme::text_color(dark)),
            );
            ui.label(
                egui::RichText::new("(not yet implemented)")
                    .size(10.0)
                    .color(theme::muted_color(dark)),
            );
        });
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
