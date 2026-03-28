//! # Shared GUI Widgets
//!
//! Reusable UI components used across multiple panels: cards, stat labels,
//! section headers, memory overview bars, toggle switches, and styled buttons.

use eframe::egui;

use crate::stats::{self, MemorySnapshot};
use crate::strings;

use super::theme;

// ─── Card Container ──────────────────────────────────────────────────────────

/// Draw a card-style container with rounded corners, subtle background, and
/// a soft shadow for visual depth.
///
/// The `add_contents` closure receives the inner `Ui` with padding applied.
pub fn card(ui: &mut egui::Ui, dark_mode: bool, add_contents: impl FnOnce(&mut egui::Ui)) {
    let bg = theme::surface_color(dark_mode);
    let border = theme::border_color(dark_mode);

    egui::Frame::new()
        .fill(bg)
        .stroke(egui::Stroke::new(0.5, border))
        .corner_radius(egui::CornerRadius::same(theme::CARD_ROUNDING))
        .inner_margin(egui::Margin::same(theme::CARD_PADDING))
        .shadow(egui::Shadow {
            offset: [0, 2],
            blur: 10,
            spread: 0,
            color: egui::Color32::from_black_alpha(if dark_mode { 50 } else { 15 }),
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add_contents(ui);
        });
}

// ─── Section Header ──────────────────────────────────────────────────────────

/// Draw a section header with an accent-coloured left bar and title.
pub fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(4.0, 20.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(2), theme::ACCENT);
        ui.add_space(6.0);
        ui.label(egui::RichText::new(title).strong().size(15.0));
    });
    ui.add_space(8.0);
}

/// Draw a page title used at the top of each panel, with a subtle
/// separator line for visual structure.
pub fn page_title(ui: &mut egui::Ui, icon: &str, title: &str, dark_mode: bool) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon).size(22.0).color(theme::ACCENT));
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(title)
                .strong()
                .size(22.0)
                .color(theme::text_color(dark_mode)),
        );
    });
    ui.add_space(8.0);
    // Subtle horizontal separator
    let width = ui.available_width();
    let (sep_rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
    ui.painter().rect_filled(
        sep_rect,
        egui::CornerRadius::same(0),
        theme::border_color(dark_mode).gamma_multiply(0.5),
    );
    ui.add_space(12.0);
}

// ─── Stat Label ──────────────────────────────────────────────────────────────

/// Compact stat label: muted title above a coloured value.
pub fn stat_label(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    color: egui::Color32,
    dark_mode: bool,
) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(11.0)
                .color(theme::muted_color(dark_mode)),
        );
        ui.add_space(2.0);
        ui.label(egui::RichText::new(value).strong().size(15.0).color(color));
    });
}

// ─── Memory Overview ─────────────────────────────────────────────────────────

/// Draw the memory overview: large percentage headline, slim bar, stat row.
///
/// Clean, modern layout with clear typographic hierarchy and minimal colour.
pub fn memory_overview(ui: &mut egui::Ui, snap: &MemorySnapshot, dark_mode: bool) {
    let load = snap.memory_load_percent as f32 / 100.0;
    let load_color = theme::load_color(load);

    // ── Headline: big percentage + context ───────────────────────
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{}%", snap.memory_load_percent))
                .strong()
                .size(36.0)
                .color(load_color),
        );
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(strings::gui::widgets::LABEL_MEMORY_USED)
                    .size(12.0)
                    .color(theme::muted_color(dark_mode)),
            );
            ui.label(
                egui::RichText::new(format!(
                    "{} of {}",
                    stats::format_bytes(snap.used_physical),
                    stats::format_bytes(snap.total_physical),
                ))
                .size(12.0)
                .color(theme::text_color(dark_mode)),
            );
        });
    });

    ui.add_space(10.0);

    // ── Slim progress bar ────────────────────────────────────────
    draw_memory_bar(ui, load, load_color, dark_mode);

    ui.add_space(14.0);

    // ── Stat labels row ──────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 30.0;

        stat_label(
            ui,
            strings::gui::widgets::LABEL_AVAILABLE,
            &stats::format_bytes(snap.available_physical),
            theme::GREEN,
            dark_mode,
        );
        stat_label(
            ui,
            strings::gui::widgets::LABEL_USED,
            &stats::format_bytes(snap.used_physical),
            theme::RED,
            dark_mode,
        );
        stat_label(
            ui,
            strings::gui::widgets::LABEL_COMMIT,
            &format!("{:.0}%", snap.commit_percent()),
            theme::YELLOW,
            dark_mode,
        );
        stat_label(
            ui,
            strings::gui::widgets::LABEL_PROCESSES,
            &snap.process_count.to_string(),
            theme::ACCENT,
            dark_mode,
        );
    });
}

/// Slim 6 px progress bar - flat, no text overlay, no fake depth.
fn draw_memory_bar(ui: &mut egui::Ui, load: f32, load_color: egui::Color32, dark_mode: bool) {
    let bar_height = 6.0;
    let bar_width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(bar_width, bar_height), egui::Sense::hover());

    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter();
    let rounding = egui::CornerRadius::same(3);

    // Track
    let track_bg = if dark_mode {
        egui::Color32::from_rgb(30, 35, 44)
    } else {
        egui::Color32::from_rgb(224, 228, 234)
    };
    painter.rect_filled(rect, rounding, track_bg);

    // Fill
    let fill_width = rect.width() * load;
    if fill_width > 1.0 {
        let fill_rect = egui::Rect::from_min_size(rect.min, egui::vec2(fill_width, bar_height));
        painter.rect_filled(fill_rect, rounding, load_color);
    }
}

// ─── Toggle Switch ───────────────────────────────────────────────────────────

/// Draw an animated iOS-style toggle switch. Returns `true` when toggled.
///
/// The switch smoothly animates between on/off states using
/// [`egui::Context::animate_bool`].
pub fn toggle_switch(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let desired_size = egui::vec2(36.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let anim = ui.ctx().animate_bool(response.id, *on);

        // Off-state background adapts to the current theme so the track
        // does not look jarring on light backgrounds.
        let dark_mode = ui.visuals().dark_mode;
        let (off_r, off_g, off_b) = if dark_mode {
            (80_u8, 82_u8, 95_u8)
        } else {
            (175_u8, 178_u8, 190_u8)
        };

        let bg_color = egui::Color32::from_rgb(
            super::theme::lerp_u8(off_r, 56, anim),
            super::theme::lerp_u8(off_g, 189, anim),
            super::theme::lerp_u8(off_b, 248, anim),
        );

        // Track background
        let track_radius = rect.height() / 2.0;
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(track_radius as u8), bg_color);

        // Knob
        let knob_radius = rect.height() / 2.0 - 2.5;
        let knob_x = egui::lerp(
            rect.left() + knob_radius + 2.5..=rect.right() - knob_radius - 2.5,
            anim,
        );
        let knob_center = egui::pos2(knob_x, rect.center().y);
        ui.painter()
            .circle_filled(knob_center, knob_radius, egui::Color32::WHITE);
    }

    response
}
