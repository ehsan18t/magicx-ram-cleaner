//! # Settings Panel
//!
//! User preferences: appearance (dark/light theme), integration toggles
//! (tray, context menu), and monitor default configuration.

use eframe::egui;

use crate::cleaner::CleanLevel;

use super::super::app::MagicXApp;
use super::super::{theme, widgets};

/// Draw the settings panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;
    widgets::page_title(ui, "\u{2699}", "Settings", dark);

    draw_appearance(ui, app);
    ui.add_space(theme::SECTION_SPACING);
    draw_integration(ui, app);
    ui.add_space(theme::SECTION_SPACING);
    draw_monitor_defaults(ui, app);
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

/// Monitor defaults section: threshold, cooldown, level, process count.
fn draw_monitor_defaults(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;

    widgets::card(ui, dark, |ui| {
        widgets::section_header(ui, "Monitor Defaults");

        // Threshold slider
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Default Threshold:")
                    .size(12.0)
                    .color(theme::text_color(dark)),
            );
            let mut threshold_f32 = f32::from(app.settings.monitor_threshold as u16);
            let slider = egui::Slider::new(&mut threshold_f32, 50.0..=99.0)
                .suffix("%")
                .step_by(1.0);
            ui.add(slider);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "slider is clamped to 50..=99"
            )]
            {
                app.settings.monitor_threshold = threshold_f32 as u32;
            }
        });

        ui.add_space(4.0);

        // Cooldown slider
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Default Cooldown:")
                    .size(12.0)
                    .color(theme::text_color(dark)),
            );
            let mut cooldown_f32 = app.settings.monitor_cooldown_secs as f32;
            let slider = egui::Slider::new(&mut cooldown_f32, 10.0..=300.0)
                .suffix("s")
                .step_by(5.0);
            ui.add(slider);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "slider is clamped to 10..=300"
            )]
            {
                app.settings.monitor_cooldown_secs = cooldown_f32 as u64;
            }
        });

        ui.add_space(4.0);

        // Default clean level
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Default Level:")
                    .size(12.0)
                    .color(theme::text_color(dark)),
            );
            egui::ComboBox::from_id_salt("settings_level")
                .selected_text(format!("{}", app.settings.default_clean_level))
                .show_ui(ui, |ui| {
                    for level in [
                        CleanLevel::Gentle,
                        CleanLevel::Moderate,
                        CleanLevel::Aggressive,
                        CleanLevel::Nuclear,
                    ] {
                        ui.selectable_value(
                            &mut app.settings.default_clean_level,
                            level,
                            format!("{level}"),
                        );
                    }
                });
        });

        ui.add_space(4.0);

        // Process count slider
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Top Processes:")
                    .size(12.0)
                    .color(theme::text_color(dark)),
            );
            let mut count_f32 = app.settings.top_process_count as f32;
            let slider = egui::Slider::new(&mut count_f32, 5.0..=50.0).step_by(5.0);
            ui.add(slider);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "slider is clamped to 5..=50"
            )]
            {
                app.settings.top_process_count = count_f32 as usize;
            }
        });
    });
}
