//! # Monitor Panel
//!
//! Continuous memory monitoring with configurable auto-clean threshold,
//! cooldown, and clean level. Shows a live log of monitor events.

use eframe::egui;

use crate::cleaner::CleanLevel;

use super::super::app::MagicXApp;
use super::super::{theme, widgets};

/// Draw the monitor panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    widgets::page_title(ui, "\u{1F50D} Memory Monitor");

    // Current status
    let snapshot = app.latest_snapshot.lock().ok().and_then(|s| s.clone());
    if let Some(snap) = &snapshot {
        widgets::card(ui, app.settings.dark_mode, |ui| {
            widgets::memory_overview(ui, snap, app.settings.dark_mode);
        });
    }

    ui.add_space(theme::SECTION_SPACING);

    // Auto-clean settings
    widgets::card(ui, app.settings.dark_mode, |ui| {
        widgets::section_header(ui, "Auto-Clean Settings");

        draw_threshold_slider(ui, app);
        ui.add_space(4.0);
        draw_cooldown_slider(ui, app);
        ui.add_space(4.0);
        draw_level_selector(ui, app);

        ui.add_space(12.0);
        draw_toggle_button(ui, app);
    });

    // Monitor log
    if !app.monitor_log.is_empty() {
        ui.add_space(theme::SECTION_SPACING);
        draw_log(ui, app);
    }
}

/// Draw the threshold slider.
fn draw_threshold_slider(ui: &mut egui::Ui, app: &mut MagicXApp) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Threshold:").size(13.0));
        ui.add(
            egui::Slider::new(&mut app.settings.monitor_threshold, 50..=99)
                .suffix("%")
                .custom_formatter(|n, _| format!("{n:.0}%")),
        );
    });
}

/// Draw the cooldown slider.
fn draw_cooldown_slider(ui: &mut egui::Ui, app: &mut MagicXApp) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Cooldown:").size(13.0));
        ui.add(egui::Slider::new(&mut app.settings.monitor_cooldown_secs, 10..=300).suffix("s"));
    });
}

/// Draw the clean level selector.
fn draw_level_selector(ui: &mut egui::Ui, app: &mut MagicXApp) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Clean level:").size(13.0));
        egui::ComboBox::from_id_salt("monitor_level")
            .selected_text(app.settings.default_clean_level.to_string())
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
                        level.to_string(),
                    );
                }
            });
    });
}

/// Draw the start/stop monitoring toggle button.
fn draw_toggle_button(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let (label, color) = if app.monitor_active {
        ("\u{23F9}  Stop Monitoring", theme::RED)
    } else {
        ("\u{25B6}  Start Monitoring", theme::GREEN)
    };

    if widgets::colored_button(ui, label, color, 180.0, true) {
        app.monitor_active = !app.monitor_active;
        if app.monitor_active {
            app.monitor_log.push("Monitor started.".to_string());
        } else {
            app.monitor_log.push("Monitor stopped.".to_string());
        }
    }
}

/// Draw the monitor event log.
fn draw_log(ui: &mut egui::Ui, app: &MagicXApp) {
    widgets::card(ui, app.settings.dark_mode, |ui| {
        widgets::section_header(ui, "Monitor Log");

        egui::ScrollArea::vertical()
            .max_height(200.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for msg in &app.monitor_log {
                    ui.label(
                        egui::RichText::new(msg)
                            .size(12.0)
                            .color(theme::muted_color(app.settings.dark_mode)),
                    );
                }
            });
    });
}
