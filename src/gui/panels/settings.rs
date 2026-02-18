//! # Settings Panel
//!
//! User-configurable preferences: appearance (theme), integration options
//! (tray, context menu), and default monitor values.

use eframe::egui;

use crate::cleaner::CleanLevel;

use super::super::app::{GuiSettings, MagicXApp};
use super::super::{theme, widgets};

/// Draw the settings panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    widgets::page_title(ui, "\u{2699}\u{FE0F} Settings");

    draw_appearance(ui, &mut app.settings);
    ui.add_space(theme::SECTION_SPACING);
    draw_integration(ui, &mut app.settings);
    ui.add_space(theme::SECTION_SPACING);
    draw_monitor_defaults(ui, &mut app.settings);
}

/// Draw the appearance section.
fn draw_appearance(ui: &mut egui::Ui, settings: &mut GuiSettings) {
    widgets::card(ui, settings.dark_mode, |ui| {
        widgets::section_header(ui, "Appearance");

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Theme:").size(13.0));
            ui.add_space(8.0);

            let dark_btn =
                egui::Button::new(egui::RichText::new("\u{1F319} Dark").size(13.0).color(
                    if settings.dark_mode {
                        theme::ACCENT
                    } else {
                        theme::MUTED
                    },
                ))
                .min_size(egui::vec2(80.0, 30.0))
                .corner_radius(egui::CornerRadius::same(6))
                .selected(settings.dark_mode);

            let light_btn = egui::Button::new(
                egui::RichText::new("\u{2600}\u{FE0F} Light")
                    .size(13.0)
                    .color(if settings.dark_mode {
                        theme::MUTED
                    } else {
                        theme::ACCENT
                    }),
            )
            .min_size(egui::vec2(80.0, 30.0))
            .corner_radius(egui::CornerRadius::same(6))
            .selected(!settings.dark_mode);

            if ui.add(dark_btn).clicked() {
                settings.dark_mode = true;
            }
            if ui.add(light_btn).clicked() {
                settings.dark_mode = false;
            }
        });
    });
}

/// Draw the integration section (tray, context menu).
fn draw_integration(ui: &mut egui::Ui, settings: &mut GuiSettings) {
    widgets::card(ui, settings.dark_mode, |ui| {
        widgets::section_header(ui, "Integration");

        ui.checkbox(&mut settings.tray_enabled, "Enable system tray icon");
        ui.label(
            egui::RichText::new("  Minimize to system tray. Right-click for quick actions.")
                .size(11.0)
                .color(theme::muted_color(settings.dark_mode)),
        );
        if settings.tray_enabled {
            ui.label(
                egui::RichText::new(
                    "  \u{26A0} Tray icon support will be available in a future update.",
                )
                .color(theme::YELLOW)
                .size(11.0),
            );
        }

        ui.add_space(6.0);

        ui.checkbox(
            &mut settings.context_menu_enabled,
            "Enable Windows context menu",
        );
        ui.label(
            egui::RichText::new("  Add \"MagicX RAM Cleaner\" to Desktop right-click menu.")
                .size(11.0)
                .color(theme::muted_color(settings.dark_mode)),
        );
        if settings.context_menu_enabled {
            ui.label(
                egui::RichText::new(
                    "  \u{26A0} Context menu integration will be available in a future update.",
                )
                .color(theme::YELLOW)
                .size(11.0),
            );
        }
    });
}

/// Draw the monitor defaults section.
fn draw_monitor_defaults(ui: &mut egui::Ui, settings: &mut GuiSettings) {
    widgets::card(ui, settings.dark_mode, |ui| {
        widgets::section_header(ui, "Monitor Defaults");

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Default threshold:").size(13.0));
            ui.add(egui::Slider::new(&mut settings.monitor_threshold, 50..=99).suffix("%"));
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Default cooldown:").size(13.0));
            ui.add(egui::Slider::new(&mut settings.monitor_cooldown_secs, 10..=300).suffix("s"));
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Default clean level:").size(13.0));
            egui::ComboBox::from_id_salt("settings_level")
                .selected_text(settings.default_clean_level.to_string())
                .show_ui(ui, |ui| {
                    for level in [
                        CleanLevel::Gentle,
                        CleanLevel::Moderate,
                        CleanLevel::Aggressive,
                        CleanLevel::Nuclear,
                    ] {
                        ui.selectable_value(
                            &mut settings.default_clean_level,
                            level,
                            level.to_string(),
                        );
                    }
                });
        });

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Top processes to show:").size(13.0));
            ui.add(egui::Slider::new(&mut settings.top_process_count, 5..=50));
        });
    });
}
