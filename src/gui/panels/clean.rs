//! # Clean Panel
//!
//! One-click memory cleaning with four severity levels. Shows results
//! with per-operation breakdowns after each clean.

use eframe::egui;

use crate::cleaner::CleanLevel;
use crate::stats;

use super::super::app::{CleanResultMsg, MagicXApp};
use super::super::{theme, widgets};

/// Draw the cleaning panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    widgets::page_title(ui, "\u{26A1} Clean Memory");

    widgets::card(ui, app.settings.dark_mode, |ui| {
        ui.label(
            egui::RichText::new("Choose a cleaning level:")
                .size(13.0)
                .color(theme::muted_color(app.settings.dark_mode)),
        );
        ui.add_space(10.0);

        let levels = [
            (
                CleanLevel::Gentle,
                "Gentle",
                "Low-priority standby only (safe for gaming)",
                theme::LEVEL_GENTLE,
            ),
            (
                CleanLevel::Moderate,
                "Moderate",
                "Working sets + low-priority standby",
                theme::LEVEL_MODERATE,
            ),
            (
                CleanLevel::Aggressive,
                "Aggressive",
                "Cache + working sets + modified + standby",
                theme::LEVEL_AGGRESSIVE,
            ),
            (
                CleanLevel::Nuclear,
                "Nuclear",
                "Everything + memory combining + second pass",
                theme::LEVEL_NUCLEAR,
            ),
        ];

        let mut level_to_clean = None;

        for (level, name, desc, color) in levels {
            let enabled = !app.cleaning_in_progress;
            ui.horizontal(|ui| {
                if widgets::colored_button(ui, name, color, 120.0, enabled) {
                    level_to_clean = Some(level);
                }
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(desc)
                        .size(12.0)
                        .color(theme::muted_color(app.settings.dark_mode)),
                );
            });
            ui.add_space(4.0);
        }

        if let Some(level) = level_to_clean {
            app.start_clean(level);
        }
    });

    if app.cleaning_in_progress {
        ui.add_space(12.0);
        widgets::card(ui, app.settings.dark_mode, |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Cleaning in progress...")
                        .strong()
                        .size(14.0),
                );
            });
        });
    }

    if let Some(ref msg) = app.last_clean_result {
        ui.add_space(12.0);
        draw_result(ui, msg, app.settings.dark_mode);
    }
}

/// Draw the result of a cleaning operation inside a card.
fn draw_result(ui: &mut egui::Ui, msg: &CleanResultMsg, dark_mode: bool) {
    widgets::card(ui, dark_mode, |ui| {
        match &msg.result {
            Ok(result) => {
                ui.label(
                    egui::RichText::new(format!("{} clean completed", msg.level))
                        .strong()
                        .size(16.0)
                        .color(theme::GREEN),
                );
                ui.add_space(8.0);

                let freed_str = if result.total_freed > 0 {
                    stats::format_bytes(result.total_freed as u64)
                } else {
                    "0 B".to_string()
                };

                // Summary metrics
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 24.0;

                    widgets::stat_label(ui, "Total Freed", &freed_str, theme::GREEN, dark_mode);
                    widgets::stat_label(
                        ui,
                        "RAM Usage",
                        &format!(
                            "{}% \u{2192} {}%",
                            result.overall_before.memory_load_percent,
                            result.overall_after.memory_load_percent
                        ),
                        theme::ACCENT,
                        dark_mode,
                    );
                    widgets::stat_label(
                        ui,
                        "Available",
                        &format!(
                            "{} \u{2192} {}",
                            stats::format_bytes(result.overall_before.available_physical),
                            stats::format_bytes(result.overall_after.available_physical)
                        ),
                        theme::YELLOW,
                        dark_mode,
                    );
                });

                ui.add_space(10.0);
                widgets::section_header(ui, "Operations");

                for r in &result.results {
                    let (icon, icon_color) = if r.success {
                        ("\u{2713}", theme::GREEN)
                    } else {
                        ("\u{2717}", theme::RED)
                    };

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(icon)
                                .color(icon_color)
                                .strong()
                                .size(13.0),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(&r.operation).size(12.0));

                        if r.freed_bytes > 0 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "+{}",
                                    stats::format_bytes(r.freed_bytes as u64)
                                ))
                                .color(theme::GREEN)
                                .size(12.0),
                            );
                        }

                        ui.label(
                            egui::RichText::new(format!("{:.2}s", r.elapsed_secs))
                                .color(theme::muted_color(dark_mode))
                                .size(11.0),
                        );
                    });
                }
            }
            Err(e) => {
                ui.label(
                    egui::RichText::new(format!("\u{2717} Clean failed: {e}"))
                        .color(theme::RED)
                        .strong()
                        .size(14.0),
                );
            }
        }
    });
}
