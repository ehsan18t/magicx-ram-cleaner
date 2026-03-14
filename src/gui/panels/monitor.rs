//! # Monitor Panel
//!
//! Continuous memory monitoring with an animated on/off toggle,
//! configurable threshold, cooldown, cleaning level, and a live log.

use eframe::egui;
use egui_phosphor::regular as ph;

use crate::cleaner::CleanLevel;

use super::super::app::MagicXApp;
use super::super::{theme, widgets};

/// Draw the monitoring panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;
    widgets::page_title(ui, ph::ACTIVITY, "Memory Monitor", dark);

    draw_toggle_card(ui, app, dark);
    ui.add_space(theme::SECTION_SPACING);
    draw_config_card(ui, app, dark);
    ui.add_space(theme::SECTION_SPACING);
    draw_log_card(ui, app, dark);
}

/// Toggle card with the on/off switch and status label.
fn draw_toggle_card(ui: &mut egui::Ui, app: &mut MagicXApp, dark: bool) {
    widgets::card(ui, dark, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Auto-Clean")
                    .strong()
                    .size(14.0)
                    .color(theme::text_color(dark)),
            );
            ui.add_space(8.0);
            widgets::toggle_switch(ui, &mut app.monitor_active);
            // Keep the persisted setting in sync with the ephemeral toggle.
            app.settings.auto_clean_enabled = app.monitor_active;
            ui.add_space(8.0);
            let (status_text, status_color) = if app.monitor_active {
                ("Running", theme::GREEN)
            } else {
                ("Stopped", theme::muted_color(dark))
            };
            ui.label(
                egui::RichText::new(status_text)
                    .color(status_color)
                    .size(12.0),
            );
        });

        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Automatically cleans memory when usage exceeds the threshold.")
                .size(11.0)
                .color(theme::muted_color(dark)),
        );
    });
}

/// Configuration card: threshold, cooldown, and clean level controls.
fn draw_config_card(ui: &mut egui::Ui, app: &mut MagicXApp, dark: bool) {
    widgets::card(ui, dark, |ui| {
        widgets::section_header(ui, "Configuration");

        // Threshold slider
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Threshold:")
                    .size(12.0)
                    .color(theme::text_color(dark)),
            );
            let mut threshold_f32 = app.settings.monitor_threshold as f32;
            let slider = egui::Slider::new(&mut threshold_f32, 50.0..=99.0)
                .suffix("%")
                .step_by(1.0);
            ui.add(slider);
            // Safe truncation: slider is clamped to 50..=99 which fits in u32.
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
                egui::RichText::new("Cooldown:")
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

        // Clean level combo
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Clean Level:")
                    .size(12.0)
                    .color(theme::text_color(dark)),
            );
            egui::ComboBox::from_id_salt("monitor_level")
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
    });
}

/// Activity log card with scrollable message list.
fn draw_log_card(ui: &mut egui::Ui, app: &mut MagicXApp, dark: bool) {
    widgets::card(ui, dark, |ui| {
        ui.horizontal(|ui| {
            widgets::section_header(ui, "Activity Log");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(egui::RichText::new("Clear").size(10.0))
                    .clicked()
                {
                    app.monitor_log.clear();
                }
            });
        });

        if app.monitor_log.is_empty() {
            ui.label(
                egui::RichText::new("No activity yet.")
                    .size(11.0)
                    .color(theme::muted_color(dark)),
            );
        } else {
            egui::ScrollArea::vertical()
                .max_height(120.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for msg in &app.monitor_log {
                        ui.label(
                            egui::RichText::new(msg)
                                .size(10.5)
                                .color(theme::muted_color(dark)),
                        );
                    }
                });
        }
    });
}
