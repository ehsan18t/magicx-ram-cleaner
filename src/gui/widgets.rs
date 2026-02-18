//! # Shared GUI Widgets
//!
//! Reusable UI components used across multiple panels: cards, stat labels,
//! section headers, memory overview bars, toggle switches, and styled buttons.

use eframe::egui;

use crate::stats::{self, MemorySnapshot};

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

/// Draw the memory usage overview with a custom bar and key metric labels.
pub fn memory_overview(ui: &mut egui::Ui, snap: &MemorySnapshot, dark_mode: bool) {
    let load = snap.memory_load_percent as f32 / 100.0;
    let load_color = theme::load_color(load);

    // Header
    ui.label(
        egui::RichText::new("Physical Memory")
            .strong()
            .size(13.5)
            .color(theme::text_color(dark_mode)),
    );
    ui.add_space(6.0);

    // Custom-painted progress bar with depth highlight
    draw_memory_bar(ui, snap, load, load_color, dark_mode);

    ui.add_space(14.0);

    // Stat labels row
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 30.0;

        stat_label(
            ui,
            "Available",
            &stats::format_bytes(snap.available_physical),
            theme::GREEN,
            dark_mode,
        );
        stat_label(
            ui,
            "Used",
            &stats::format_bytes(snap.used_physical),
            theme::RED,
            dark_mode,
        );
        stat_label(
            ui,
            "Commit",
            &format!("{:.0}%", snap.commit_percent()),
            theme::YELLOW,
            dark_mode,
        );
        stat_label(
            ui,
            "Processes",
            &snap.process_count.to_string(),
            theme::ACCENT,
            dark_mode,
        );
    });
}

/// Custom-painted memory usage bar with a top-highlight depth effect
/// and drop-shadow text for improved readability.
fn draw_memory_bar(
    ui: &mut egui::Ui,
    snap: &MemorySnapshot,
    load: f32,
    load_color: egui::Color32,
    dark_mode: bool,
) {
    let bar_height = 32.0;
    let bar_width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(bar_width, bar_height), egui::Sense::hover());

    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter();

    // Track background
    let track_bg = if dark_mode {
        egui::Color32::from_rgb(14, 18, 25)
    } else {
        egui::Color32::from_rgb(228, 231, 236)
    };
    painter.rect_filled(rect, egui::CornerRadius::same(6), track_bg);

    // Filled portion
    let fill_width = rect.width() * load;
    if fill_width > 1.0 {
        let fill_rect = egui::Rect::from_min_size(rect.min, egui::vec2(fill_width, bar_height));
        painter.rect_filled(fill_rect, egui::CornerRadius::same(6), load_color);

        // Subtle top highlight for a 3-D depth look
        if fill_width > 12.0 {
            let hl = egui::Rect::from_min_size(rect.min, egui::vec2(fill_width, bar_height * 0.40));
            painter.rect_filled(
                hl,
                egui::CornerRadius::same(6),
                egui::Color32::from_white_alpha(22),
            );
        }
    }

    // Text with subtle drop shadow for readability on any background
    let text = format!(
        "{}%  \u{2014}  {} / {}",
        snap.memory_load_percent,
        stats::format_bytes(snap.used_physical),
        stats::format_bytes(snap.total_physical),
    );
    let text_pos = egui::pos2(rect.left() + 14.0, rect.center().y);
    let font = egui::FontId::proportional(13.0);

    painter.text(
        text_pos + egui::vec2(0.5, 0.5),
        egui::Align2::LEFT_CENTER,
        &text,
        font.clone(),
        egui::Color32::from_black_alpha(140),
    );
    painter.text(
        text_pos,
        egui::Align2::LEFT_CENTER,
        &text,
        font,
        egui::Color32::WHITE,
    );
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

        let bg_color = egui::Color32::from_rgb(
            lerp_color_u8(80, 56, anim),
            lerp_color_u8(82, 189, anim),
            lerp_color_u8(95, 248, anim),
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

/// Linearly interpolate a single colour channel for the toggle switch.
fn lerp_color_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    let result = f32::from(a).mul_add(1.0 - t, f32::from(b) * t);
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "lerp of two u8s is always in 0..=255"
    )]
    {
        result.round() as u8
    }
}
