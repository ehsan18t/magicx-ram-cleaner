//! # Dashboard Panel
//!
//! Main overview combining real-time memory status with one-click cleaning.
//! Layout: headline stats at top, uniform action buttons below, feedback at bottom.

use eframe::egui;

use crate::cleaner::CleanLevel;
use crate::stats;

use super::super::app::{CleanResultMsg, MagicXApp};
use super::super::{theme, widgets};

/// Draw the dashboard panel.
///
/// Structure:
/// 1. Memory overview card (headline %, slim bar, stat row, system info)
/// 2. Clean buttons (uniform neutral cards with color dots)
/// 3. Progress / result feedback
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;
    widgets::page_title(ui, "\u{2261}", "Dashboard", dark);

    let snapshot = app.latest_snapshot.lock().ok().and_then(|s| s.clone());

    if let Some(snap) = &snapshot {
        // ── Info card ────────────────────────────────────────────
        draw_info_card(ui, snap, dark);

        ui.add_space(theme::SECTION_SPACING);

        // ── Clean buttons ────────────────────────────────────────
        draw_clean_section(ui, app, dark);

        // ── Feedback ─────────────────────────────────────────────
        if app.cleaning_in_progress {
            ui.add_space(8.0);
            draw_progress_card(ui, dark);
        }

        if let Some(ref msg) = app.last_clean_result {
            ui.add_space(8.0);
            draw_result(ui, msg, dark);
        }
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

// ─── Info Card ───────────────────────────────────────────────────────────────

/// Unified info card: memory overview + system info below a thin divider.
fn draw_info_card(ui: &mut egui::Ui, snap: &stats::MemorySnapshot, dark: bool) {
    widgets::card(ui, dark, |ui| {
        widgets::memory_overview(ui, snap, dark);

        ui.add_space(10.0);

        // Thin divider
        let width = ui.available_width();
        let (sep, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
        ui.painter().rect_filled(
            sep,
            egui::CornerRadius::ZERO,
            theme::border_color(dark).gamma_multiply(0.4),
        );

        ui.add_space(10.0);

        // Secondary system info
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 30.0;

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
    });
}

// ─── Clean Buttons ───────────────────────────────────────────────────────────

/// Four uniform clean buttons in a 2x2 grid.
/// All share the same neutral background — only a small colour dot differentiates.
fn draw_clean_section(ui: &mut egui::Ui, app: &mut MagicXApp, dark: bool) {
    widgets::section_header(ui, "Clean Memory");

    let levels = [
        (
            CleanLevel::Gentle,
            "Gentle",
            "Low-priority standby",
            theme::LEVEL_GENTLE,
        ),
        (
            CleanLevel::Moderate,
            "Moderate",
            "Working sets + standby",
            theme::LEVEL_MODERATE,
        ),
        (
            CleanLevel::Aggressive,
            "Aggressive",
            "Cache + modified pages",
            theme::LEVEL_AGGRESSIVE,
        ),
        (
            CleanLevel::Nuclear,
            "Nuclear",
            "Full deep clean",
            theme::LEVEL_NUCLEAR,
        ),
    ];

    let mut level_to_clean = None;
    let enabled = !app.cleaning_in_progress;

    for row in levels.chunks(2) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for &(level, name, desc, color) in row {
                if draw_clean_button(ui, name, desc, color, enabled, dark) {
                    level_to_clean = Some(level);
                }
            }
        });
        ui.add_space(8.0);
    }

    if let Some(level) = level_to_clean {
        app.start_clean(level);
    }
}

/// A clean, flat button with a small colour indicator dot.
///
/// All buttons share the same neutral background. The colour dot next to
/// the name is the only visual differentiator. On hover, a subtle border
/// in the level colour appears.
///
/// Returns `true` when clicked.
fn draw_clean_button(
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
    let rounding = egui::CornerRadius::same(8);

    // ── Background: same neutral surface for all buttons ─────────
    let bg = if !enabled {
        egui::Color32::from_rgb(25, 28, 34)
    } else if hovered {
        if dark {
            egui::Color32::from_rgb(35, 40, 50)
        } else {
            egui::Color32::from_rgb(235, 238, 244)
        }
    } else {
        theme::surface_color(dark)
    };

    let stroke = if hovered && enabled {
        egui::Stroke::new(1.0, color.gamma_multiply(0.7))
    } else {
        egui::Stroke::new(0.5, theme::border_color(dark))
    };

    painter.rect(rect, rounding, bg, stroke, egui::StrokeKind::Inside);

    // ── Colour dot (8px circle) ──────────────────────────────────
    let dot_center = egui::pos2(rect.left() + 18.0, rect.top() + 19.0);
    let dot_radius = 4.0;
    let dot_color = if enabled {
        color
    } else {
        theme::muted_color(dark)
    };
    painter.circle_filled(dot_center, dot_radius, dot_color);

    // ── Text ─────────────────────────────────────────────────────
    let text_x = rect.left() + 30.0;
    let name_color = if enabled {
        theme::text_color(dark)
    } else {
        theme::muted_color(dark)
    };

    painter.text(
        egui::pos2(text_x, rect.top() + 19.0),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(13.5),
        name_color,
    );
    painter.text(
        egui::pos2(text_x, rect.top() + 37.0),
        egui::Align2::LEFT_CENTER,
        desc,
        egui::FontId::proportional(10.5),
        theme::muted_color(dark),
    );

    response.clicked() && enabled
}

// ─── Feedback Cards ──────────────────────────────────────────────────────────

/// Progress indicator while cleaning runs.
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
        Ok(result) => draw_result_success(ui, msg, result, dark),
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

/// Successful clean result with freed memory and operation breakdown.
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

/// Per-operation result breakdown.
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
