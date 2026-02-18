//! # Shared GUI Widgets
//!
//! Reusable UI components used across multiple panels: cards, stat labels,
//! section headers, memory overview bars, and styled buttons.

use eframe::egui;

use crate::stats::{self, MemorySnapshot};

use super::theme;

// ─── Card Widget ─────────────────────────────────────────────────────────────

/// Draw a card-style container with rounded corners and a subtle background.
///
/// The `add_contents` closure receives the inner `Ui` with padding applied.
pub fn card(ui: &mut egui::Ui, dark_mode: bool, add_contents: impl FnOnce(&mut egui::Ui)) {
    let bg = theme::surface_color(dark_mode);
    let border = theme::border_color(dark_mode);
    let rounding = egui::CornerRadius::same(theme::CARD_ROUNDING);
    let padding = egui::Margin::same(theme::CARD_PADDING);

    egui::Frame::new()
        .fill(bg)
        .stroke(egui::Stroke::new(0.5, border))
        .corner_radius(rounding)
        .inner_margin(padding)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add_contents(ui);
        });
}

// ─── Section Header ──────────────────────────────────────────────────────────

/// Draw a section header with accent-coloured left bar and title text.
pub fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.horizontal(|ui| {
        // Accent bar
        let (rect, _) = ui.allocate_exact_size(egui::vec2(3.0, 18.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(1), theme::ACCENT);
        ui.add_space(4.0);
        ui.label(egui::RichText::new(title).strong().size(15.0));
    });
    ui.add_space(6.0);
}

/// Draw a page title (used at the top of each panel).
pub fn page_title(ui: &mut egui::Ui, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .strong()
            .size(22.0)
            .color(theme::ACCENT),
    );
    ui.add_space(4.0);
}

// ─── Stat Label ──────────────────────────────────────────────────────────────

/// Draw a compact stat label with a muted title and coloured value.
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
        ui.label(egui::RichText::new(value).strong().size(15.0).color(color));
    });
}

// ─── Memory Overview ─────────────────────────────────────────────────────────

/// Draw the memory usage progress bar and key metric labels.
pub fn memory_overview(ui: &mut egui::Ui, snap: &MemorySnapshot, dark_mode: bool) {
    let load = snap.memory_load_percent as f32 / 100.0;
    let load_color = theme::load_color(load);

    // Progress bar
    ui.vertical(|ui| {
        ui.label(egui::RichText::new("Physical Memory").strong().size(13.0));
        ui.add_space(4.0);

        let bar = egui::ProgressBar::new(load)
            .text(
                egui::RichText::new(format!(
                    "{}%  —  {} / {}",
                    snap.memory_load_percent,
                    stats::format_bytes(snap.used_physical),
                    stats::format_bytes(snap.total_physical),
                ))
                .strong()
                .size(12.0),
            )
            .fill(load_color)
            .corner_radius(egui::CornerRadius::same(4));
        ui.add(bar);
    });

    ui.add_space(10.0);

    // Stat labels row
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 28.0;

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

// ─── Styled Button ───────────────────────────────────────────────────────────

/// Create a coloured button with rounded styling.
///
/// Returns `true` if clicked.
pub fn colored_button(
    ui: &mut egui::Ui,
    label: &str,
    color: egui::Color32,
    width: f32,
    enabled: bool,
) -> bool {
    let button = egui::Button::new(egui::RichText::new(label).strong().color(color).size(14.0))
        .min_size(egui::vec2(width, 36.0))
        .corner_radius(egui::CornerRadius::same(6));

    ui.add_enabled(enabled, button).clicked()
}
