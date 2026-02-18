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

/// Four clean-level buttons in a 2×2 grid.
///
/// Each button has a coloured left-edge stripe, bold name, description,
/// and a right-side intensity indicator. On hover the card tints subtly
/// with its level colour.
fn draw_clean_section(ui: &mut egui::Ui, app: &mut MagicXApp, dark: bool) {
    widgets::section_header(ui, "Clean Memory");

    let levels = [
        (
            CleanLevel::Gentle,
            "Gentle",
            "Low-priority standby",
            theme::LEVEL_GENTLE,
            1_u8,
        ),
        (
            CleanLevel::Moderate,
            "Moderate",
            "Working sets + standby",
            theme::LEVEL_MODERATE,
            2_u8,
        ),
        (
            CleanLevel::Aggressive,
            "Aggressive",
            "Cache + modified pages",
            theme::LEVEL_AGGRESSIVE,
            3_u8,
        ),
        (
            CleanLevel::Nuclear,
            "Nuclear",
            "Full deep clean",
            theme::LEVEL_NUCLEAR,
            4_u8,
        ),
    ];

    let mut level_to_clean = None;
    let enabled = !app.cleaning_in_progress;

    for row in levels.chunks(2) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for &(level, name, desc, color, intensity) in row {
                if draw_clean_button(ui, name, desc, color, intensity, enabled, dark) {
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

/// A level card button with a coloured left stripe and intensity indicator.
///
/// Layout:
/// - 3 px coloured left stripe (brightens on hover)
/// - Bold 13.5 px level name (tinted in level colour on hover)
/// - 10.5 px muted description
/// - Right side: filled pill badge with intensity dots (● × N)
///
/// On hover the background tints very subtly with the level colour (~8 % opacity).
///
/// Returns `true` when clicked.
fn draw_clean_button(
    ui: &mut egui::Ui,
    name: &str,
    desc: &str,
    color: egui::Color32,
    intensity: u8,
    enabled: bool,
    dark: bool,
) -> bool {
    let width = (ui.available_width() - 8.0) / 2.0;
    let height = 62.0_f32;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());

    let ctx = BtnCtx {
        hovered: response.hovered() && enabled,
        pressed: response.is_pointer_button_down_on() && enabled,
        enabled,
        dark,
    };
    let painter = ui.painter();

    paint_btn_card(painter, rect, color, height, &ctx);
    paint_btn_text(painter, rect, name, desc, color, &ctx);
    paint_btn_badge(painter, rect, color, intensity, &ctx);

    response.clicked() && enabled
}

/// Shared interaction state for a button render pass.
///
/// All four fields are distinct boolean dimensions — there is no meaningful
/// two-variant enum that would reduce the count without obscuring intent.
#[allow(clippy::struct_excessive_bools)]
struct BtnCtx {
    hovered: bool,
    pressed: bool,
    enabled: bool,
    dark: bool,
}

/// Paint the card background, border, and left colour stripe.
fn paint_btn_card(
    painter: &egui::Painter,
    rect: egui::Rect,
    color: egui::Color32,
    height: f32,
    ctx: &BtnCtx,
) {
    let base_bg = if ctx.dark {
        egui::Color32::from_rgb(22, 27, 34)
    } else {
        egui::Color32::from_rgb(248, 250, 252)
    };

    let fill = if !ctx.enabled {
        if ctx.dark {
            egui::Color32::from_rgb(20, 24, 30)
        } else {
            egui::Color32::from_rgb(242, 244, 246)
        }
    } else if ctx.pressed {
        blend_color(base_bg, color, 0.14)
    } else if ctx.hovered {
        blend_color(base_bg, color, 0.08)
    } else {
        base_bg
    };

    let border = if ctx.hovered && ctx.enabled {
        color.gamma_multiply(0.45)
    } else {
        theme::border_color(ctx.dark).gamma_multiply(if ctx.dark { 1.0 } else { 0.8 })
    };

    painter.rect(
        rect,
        egui::CornerRadius::same(8),
        fill,
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );

    // Left colour stripe — widens slightly on hover
    let stripe_w = if ctx.hovered && ctx.enabled {
        4.0_f32
    } else {
        3.0_f32
    };
    let stripe_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 1.0, rect.top() + 6.0),
        egui::vec2(stripe_w, height - 12.0),
    );
    let stripe_fill = if ctx.enabled {
        if ctx.hovered {
            color
        } else {
            color.gamma_multiply(0.75)
        }
    } else {
        theme::muted_color(ctx.dark).gamma_multiply(0.5)
    };
    painter.rect_filled(
        stripe_rect,
        egui::CornerRadius {
            nw: 3,
            sw: 3,
            ne: 2,
            se: 2,
        },
        stripe_fill,
    );
}

/// Paint the level name and description text inside a button card.
fn paint_btn_text(
    painter: &egui::Painter,
    rect: egui::Rect,
    name: &str,
    desc: &str,
    color: egui::Color32,
    ctx: &BtnCtx,
) {
    let tx = rect.left() + 14.0;
    // Name: tinted in level colour on hover, plain text otherwise
    let name_fg = if ctx.hovered && ctx.enabled {
        color.gamma_multiply(0.9)
    } else if ctx.enabled {
        theme::text_color(ctx.dark)
    } else {
        theme::muted_color(ctx.dark)
    };
    painter.text(
        egui::pos2(tx, rect.top() + 22.0),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(13.5),
        name_fg,
    );
    let desc_fg = if ctx.enabled {
        theme::muted_color(ctx.dark)
    } else {
        theme::muted_color(ctx.dark).gamma_multiply(0.6)
    };
    painter.text(
        egui::pos2(tx, rect.top() + 40.0),
        egui::Align2::LEFT_CENTER,
        desc,
        egui::FontId::proportional(10.5),
        desc_fg,
    );
}

/// Paint the intensity pill badge on the right side of the button card.
///
/// Each dot (●) represents one cleaning stage; Nuclear gets four.
fn paint_btn_badge(
    painter: &egui::Painter,
    rect: egui::Rect,
    color: egui::Color32,
    intensity: u8,
    ctx: &BtnCtx,
) {
    let dots: String = "\u{25CF}".repeat(usize::from(intensity));
    // Each filled-circle glyph ≈ 7 px wide at 8 pt, plus 10 px pill padding.
    let pill_w = f32::from(intensity).mul_add(7.0, 10.0);
    let pill_h = 16.0_f32;
    let pill_rect = egui::Rect::from_min_size(
        egui::pos2(rect.right() - pill_w - 10.0, rect.center().y - pill_h / 2.0),
        egui::vec2(pill_w, pill_h),
    );
    let pill_fill = if ctx.enabled {
        color.gamma_multiply(if ctx.hovered { 0.25 } else { 0.15 })
    } else {
        theme::muted_color(ctx.dark).gamma_multiply(0.12)
    };
    let pill_ink = if ctx.enabled {
        color.gamma_multiply(if ctx.hovered { 1.0 } else { 0.7 })
    } else {
        theme::muted_color(ctx.dark).gamma_multiply(0.5)
    };
    painter.rect_filled(pill_rect, egui::CornerRadius::same(8), pill_fill);
    painter.text(
        pill_rect.center(),
        egui::Align2::CENTER_CENTER,
        dots.as_str(),
        egui::FontId::proportional(8.0),
        pill_ink,
    );
}

/// Blend `base` towards `tint` by `amount` (0.0 = pure base, 1.0 = pure tint).
///
/// Used to apply subtle level-colour tints to button backgrounds on hover/press.
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
