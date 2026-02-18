//! # Dashboard Panel
//!
//! Main overview combining real-time memory status with one-click cleaning.
//! Replaces the former separate Dashboard + Clean panels.

use eframe::egui;

use crate::cleaner::CleanLevel;
use crate::stats;

use super::super::app::{CleanResultMsg, MagicXApp};
use super::super::{theme, widgets};

/// Draw the dashboard panel (memory overview + cleaning).
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;
    widgets::page_title(ui, "\u{2261}", "Dashboard", dark);

    let snapshot = app.latest_snapshot.lock().ok().and_then(|s| s.clone());

    if let Some(snap) = &snapshot {
        widgets::card(ui, dark, |ui| {
            widgets::memory_overview(ui, snap, dark);
        });

        ui.add_space(theme::SECTION_SPACING);
        draw_clean_section(ui, app, dark);

        if app.cleaning_in_progress {
            ui.add_space(6.0);
            draw_progress_card(ui, dark);
        }

        if let Some(ref msg) = app.last_clean_result {
            ui.add_space(6.0);
            draw_result(ui, msg, dark);
        }

        ui.add_space(theme::SECTION_SPACING);
        draw_quick_info(ui, snap, dark);
    } else {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            ui.spinner();
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Loading memory information...")
                    .size(12.0)
                    .color(theme::muted_color(dark)),
            );
        });
    }
}

/// Draw the cleaning buttons in a 2x2 card grid.
fn draw_clean_section(ui: &mut egui::Ui, app: &mut MagicXApp, dark: bool) {
    widgets::section_header(ui, "Clean Memory");

    let levels = [
        (
            CleanLevel::Gentle,
            "Gentle",
            "Low-priority standby only",
            theme::LEVEL_GENTLE,
        ),
        (
            CleanLevel::Moderate,
            "Moderate",
            "Working sets + low standby",
            theme::LEVEL_MODERATE,
        ),
        (
            CleanLevel::Aggressive,
            "Aggressive",
            "Cache + modified + standby",
            theme::LEVEL_AGGRESSIVE,
        ),
        (
            CleanLevel::Nuclear,
            "Nuclear",
            "Full deep clean + combining",
            theme::LEVEL_NUCLEAR,
        ),
    ];

    let mut level_to_clean = None;
    let enabled = !app.cleaning_in_progress;

    // 2x2 grid of clean-level cards
    for row in levels.chunks(2) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for &(level, name, desc, color) in row {
                if draw_level_card(ui, name, desc, color, enabled, dark) {
                    level_to_clean = Some(level);
                }
            }
        });
        ui.add_space(4.0);
    }

    if let Some(level) = level_to_clean {
        app.start_clean(level);
    }
}

/// Draw a single cleaning level as a clickable card.
///
/// Returns `true` when the card is clicked.
fn draw_level_card(
    ui: &mut egui::Ui,
    name: &str,
    desc: &str,
    color: egui::Color32,
    enabled: bool,
    dark: bool,
) -> bool {
    let width = (ui.available_width() - 8.0) / 2.0;
    let desired = egui::vec2(width, 52.0);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click());

    let hovered = response.hovered() && enabled;
    let painter = ui.painter();

    // Card background
    let bg = if hovered {
        color.gamma_multiply(if dark { 0.15 } else { 0.10 })
    } else {
        theme::surface_color(dark)
    };
    let border = if hovered {
        egui::Stroke::new(1.0, color.gamma_multiply(0.6))
    } else {
        egui::Stroke::new(0.5, theme::border_color(dark))
    };
    painter.rect(
        rect,
        egui::CornerRadius::same(8),
        bg,
        border,
        egui::StrokeKind::Inside,
    );

    // Colour accent bar on the left
    let bar = egui::Rect::from_min_size(rect.left_top(), egui::vec2(3.0, rect.height()));
    painter.rect_filled(bar, egui::CornerRadius::same(1), color);

    // Text content
    let text_x = rect.left() + 12.0;
    let name_color = if enabled {
        theme::text_color(dark)
    } else {
        theme::muted_color(dark)
    };
    painter.text(
        egui::pos2(text_x, rect.top() + 16.0),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(13.0),
        name_color,
    );
    painter.text(
        egui::pos2(text_x, rect.top() + 34.0),
        egui::Align2::LEFT_CENTER,
        desc,
        egui::FontId::proportional(10.5),
        theme::muted_color(dark),
    );

    response.clicked() && enabled
}

/// Animated progress card shown while cleaning runs.
fn draw_progress_card(ui: &mut egui::Ui, dark: bool) {
    widgets::card(ui, dark, |ui| {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Cleaning in progress...")
                    .strong()
                    .size(12.0)
                    .color(theme::ACCENT),
            );
        });
    });
}

/// Draw the result of a cleaning operation.
fn draw_result(ui: &mut egui::Ui, msg: &CleanResultMsg, dark: bool) {
    widgets::card(ui, dark, |ui| match &msg.result {
        Ok(result) => {
            draw_result_success(ui, msg, result, dark);
        }
        Err(e) => {
            ui.label(
                egui::RichText::new(format!("\u{2717} Clean failed: {e}"))
                    .color(theme::RED)
                    .strong()
                    .size(13.0),
            );
        }
    });
}

/// Draw a successful clean result with stats and operation list.
fn draw_result_success(
    ui: &mut egui::Ui,
    msg: &CleanResultMsg,
    result: &crate::cleaner::SmartCleanResult,
    dark: bool,
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("\u{2713}")
                .color(theme::GREEN)
                .strong()
                .size(14.0),
        );
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(format!("{} clean completed", msg.level))
                .strong()
                .size(13.0)
                .color(theme::text_color(dark)),
        );
    });

    ui.add_space(4.0);
    let freed_str = if result.total_freed > 0 {
        stats::format_bytes(result.total_freed as u64)
    } else {
        "0 B".to_string()
    };

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 20.0;
        widgets::stat_label(ui, "Freed", &freed_str, theme::GREEN, dark);
        widgets::stat_label(
            ui,
            "Usage",
            &format!(
                "{}% \u{2192} {}%",
                result.overall_before.memory_load_percent, result.overall_after.memory_load_percent
            ),
            theme::ACCENT,
            dark,
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
            dark,
        );
    });

    ui.add_space(6.0);
    draw_operation_list(ui, &result.results, dark);
}

/// Draw the per-operation result breakdown.
fn draw_operation_list(ui: &mut egui::Ui, results: &[crate::cleaner::CleanResult], dark: bool) {
    for r in results {
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
                    .size(11.0),
            );
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(&r.operation)
                    .size(11.0)
                    .color(theme::text_color(dark)),
            );

            if r.freed_bytes > 0 {
                ui.label(
                    egui::RichText::new(format!("+{}", stats::format_bytes(r.freed_bytes as u64)))
                        .color(theme::GREEN)
                        .size(10.5),
                );
            }

            ui.label(
                egui::RichText::new(format!("{:.2}s", r.elapsed_secs))
                    .color(theme::muted_color(dark))
                    .size(10.0),
            );
        });
    }
}

/// Draw quick system information.
fn draw_quick_info(ui: &mut egui::Ui, snap: &stats::MemorySnapshot, dark: bool) {
    widgets::section_header(ui, "System Info");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 20.0;

        widgets::stat_label(
            ui,
            "Total RAM",
            &stats::format_bytes(snap.total_physical),
            theme::ACCENT,
            dark,
        );
        widgets::stat_label(
            ui,
            "Page File",
            &format!(
                "{} / {}",
                stats::format_bytes(snap.total_page_file - snap.available_page_file),
                stats::format_bytes(snap.total_page_file)
            ),
            theme::YELLOW,
            dark,
        );
        widgets::stat_label(
            ui,
            "Threads",
            &snap.thread_count.to_string(),
            theme::muted_color(dark),
            dark,
        );
    });
}
