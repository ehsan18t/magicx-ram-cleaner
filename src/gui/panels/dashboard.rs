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

/// Level descriptor used by the circle button row and detail panel.
struct LevelInfo {
    level: CleanLevel,
    name: &'static str,
    short: &'static str,
    detail: &'static str,
    color: egui::Color32,
}

/// All four cleaning levels with their display metadata.
const LEVELS: [LevelInfo; 4] = [
    LevelInfo {
        level: CleanLevel::Gentle,
        name: "Gentle",
        short: "Low-priority standby",
        detail: "Flushes low-priority standby pages only. Safe to run at any time — no impact on running processes or file cache.",
        color: theme::LEVEL_GENTLE,
    },
    LevelInfo {
        level: CleanLevel::Moderate,
        name: "Moderate",
        short: "Working sets + standby",
        detail: "Trims the working sets of all processes and clears the standby list. May cause a brief disk spike as pages are reloaded on demand.",
        color: theme::LEVEL_MODERATE,
    },
    LevelInfo {
        level: CleanLevel::Aggressive,
        name: "Aggressive",
        short: "Cache + modified pages",
        detail: "Flushes the system file cache and forces modified pages to disk before clearing them. Effective for reclaiming large cache builds.",
        color: theme::LEVEL_AGGRESSIVE,
    },
    LevelInfo {
        level: CleanLevel::Nuclear,
        name: "Nuclear",
        short: "Full deep clean",
        detail: "Performs all cleaning operations: working sets, standby list, modified list, and system file cache. Maximum recovery; use sparingly.",
        color: theme::LEVEL_NUCLEAR,
    },
];

/// Four equal circle buttons in a row, followed by a live detail panel.
///
/// Hovering a circle updates the detail panel below with the level's full
/// description. Clicking triggers the clean operation.
fn draw_clean_section(ui: &mut egui::Ui, app: &mut MagicXApp, dark: bool) {
    widgets::section_header(ui, "Clean Memory");

    let enabled = !app.cleaning_in_progress;
    let mut level_to_clean = None;
    let mut hovered_idx: Option<usize> = None;

    // ── Circle row ───────────────────────────────────────────────
    let circle_d = 88.0_f32; // diameter
    // Divide remaining width evenly into 5 gaps (left + between + right).
    let spacing = (-circle_d).mul_add(4.0, ui.available_width()) / 5.0;

    ui.horizontal(|ui| {
        ui.add_space(spacing);
        ui.spacing_mut().item_spacing.x = spacing;

        for (i, info) in LEVELS.iter().enumerate() {
            let (rect, resp) =
                ui.allocate_exact_size(egui::vec2(circle_d, circle_d), egui::Sense::click());

            let hovered = resp.hovered() && enabled;
            let pressed = resp.is_pointer_button_down_on() && enabled;

            if hovered {
                hovered_idx = Some(i);
            }
            paint_circle_btn(
                ui.painter(),
                rect,
                info,
                &CircleCtx {
                    hovered,
                    pressed,
                    enabled,
                    dark,
                },
            );

            if resp.clicked() && enabled {
                level_to_clean = Some(info.level);
            }
        }
    });

    ui.add_space(12.0);

    // ── Detail panel ─────────────────────────────────────────────
    draw_level_detail(ui, hovered_idx, dark);

    if let Some(level) = level_to_clean {
        app.start_clean(level);
    }
}

/// Draw the detail panel that updates based on the hovered level index.
///
/// Shows a neutral prompt when nothing is hovered, or the full level
/// name, short subtitle, and descriptive text when a circle is hovered.
fn draw_level_detail(ui: &mut egui::Ui, hovered: Option<usize>, dark: bool) {
    let panel_h = 72.0_f32;
    let available = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(available, panel_h), egui::Sense::hover());
    let painter = ui.painter();

    // Panel background
    let bg = if dark {
        egui::Color32::from_rgb(18, 22, 29)
    } else {
        egui::Color32::from_rgb(244, 246, 250)
    };
    painter.rect(
        rect,
        egui::CornerRadius::same(8),
        bg,
        egui::Stroke::new(1.0, theme::border_color(dark).gamma_multiply(0.6)),
        egui::StrokeKind::Inside,
    );

    match hovered.and_then(|i| LEVELS.get(i)) {
        None => {
            // Neutral prompt
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Hover a level to see details",
                egui::FontId::proportional(11.5),
                theme::muted_color(dark).gamma_multiply(0.7),
            );
        }
        Some(info) => {
            // Left accent bar in level colour
            let bar = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 1.0, rect.top() + 8.0),
                egui::vec2(3.0, panel_h - 16.0),
            );
            painter.rect_filled(bar, egui::CornerRadius::same(2), info.color);

            let tx = rect.left() + 14.0;

            // Level name
            painter.text(
                egui::pos2(tx, rect.top() + 22.0),
                egui::Align2::LEFT_CENTER,
                info.name,
                egui::FontId::proportional(14.0),
                info.color,
            );
            // Short subtitle
            painter.text(
                egui::pos2(tx, rect.top() + 38.0),
                egui::Align2::LEFT_CENTER,
                info.short,
                egui::FontId::proportional(10.5),
                theme::muted_color(dark),
            );
            // Detail description (clipped to panel width)
            painter.text(
                egui::pos2(tx, rect.top() + 56.0),
                egui::Align2::LEFT_CENTER,
                info.detail,
                egui::FontId::proportional(10.0),
                theme::muted_color(dark).gamma_multiply(0.75),
            );
        }
    }
}

/// Shared interaction state passed to circle-button painters.
///
/// All four fields are independent boolean dimensions with no meaningful
/// two-variant enum reduction.
#[allow(clippy::struct_excessive_bools)]
struct CircleCtx {
    hovered: bool,
    pressed: bool,
    enabled: bool,
    dark: bool,
}

/// Paint a single circle clean button.
///
/// Visual anatomy:
/// - Filled circle background (tints with level colour on hover/press)
/// - Outer coloured ring (brightens on hover)
/// - Level name centred inside
///
/// Returns nothing — click detection is handled by the caller.
fn paint_circle_btn(painter: &egui::Painter, rect: egui::Rect, info: &LevelInfo, ctx: &CircleCtx) {
    let center = rect.center();
    let radius = rect.width() / 2.0;

    // Inner fill
    let base_fill = if ctx.dark {
        egui::Color32::from_rgb(22, 27, 34)
    } else {
        egui::Color32::from_rgb(248, 250, 252)
    };
    let fill = if !ctx.enabled {
        if ctx.dark {
            egui::Color32::from_rgb(20, 24, 30)
        } else {
            egui::Color32::from_rgb(238, 240, 243)
        }
    } else if ctx.pressed {
        blend_color(base_fill, info.color, 0.18)
    } else if ctx.hovered {
        blend_color(base_fill, info.color, 0.10)
    } else {
        base_fill
    };
    painter.circle_filled(center, radius - 1.0, fill);

    // Outer coloured ring
    let ring_color = if !ctx.enabled {
        theme::muted_color(ctx.dark).gamma_multiply(0.3)
    } else if ctx.hovered || ctx.pressed {
        info.color
    } else {
        info.color.gamma_multiply(0.55)
    };
    let ring_w = if ctx.hovered || ctx.pressed {
        3.0_f32
    } else {
        2.5_f32
    };
    painter.circle_stroke(
        center,
        radius - ring_w / 2.0 - 0.5,
        egui::Stroke::new(ring_w, ring_color),
    );

    // Level name
    let name_fg = if !ctx.enabled {
        theme::muted_color(ctx.dark).gamma_multiply(0.6)
    } else if ctx.hovered || ctx.pressed {
        info.color
    } else {
        theme::text_color(ctx.dark)
    };
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        info.name,
        egui::FontId::proportional(12.0),
        name_fg,
    );
}

/// Blend `base` towards `tint` by `amount` (0.0 = pure base, 1.0 = pure tint).
///
/// Used to apply subtle level-colour fills to circle backgrounds on hover/press.
fn blend_color(base: egui::Color32, tint: egui::Color32, amount: f32) -> egui::Color32 {
    let a = amount.clamp(0.0, 1.0);
    let inv = 1.0 - a;
    egui::Color32::from_rgb(
        (f32::from(base.r()).mul_add(inv, f32::from(tint.r()) * a)) as u8,
        (f32::from(base.g()).mul_add(inv, f32::from(tint.g()) * a)) as u8,
        (f32::from(base.b()).mul_add(inv, f32::from(tint.b()) * a)) as u8,
    )
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
