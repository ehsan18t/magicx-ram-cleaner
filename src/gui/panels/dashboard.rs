//! # Dashboard Panel
//!
//! Main overview showing real-time memory usage, a history chart, and
//! quick system information.

use std::collections::VecDeque;

use eframe::egui;

use crate::stats;

use super::super::app::MagicXApp;
use super::super::{theme, widgets};

/// Draw the dashboard panel.
pub fn draw(ui: &mut egui::Ui, app: &MagicXApp) {
    let dark = app.settings.dark_mode;
    widgets::page_title(ui, "\u{2261}", "Dashboard", dark);

    let snapshot = app.latest_snapshot.lock().ok().and_then(|s| s.clone());

    if let Some(snap) = &snapshot {
        widgets::card(ui, dark, |ui| {
            widgets::memory_overview(ui, snap, dark);
        });

        ui.add_space(theme::SECTION_SPACING);
        draw_memory_chart(ui, app);

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

/// Draw the memory history line chart.
fn draw_memory_chart(ui: &mut egui::Ui, app: &MagicXApp) {
    widgets::section_header(ui, "Memory History");

    let history_data: Option<VecDeque<super::super::app::HistoryPoint>> =
        app.history.lock().ok().map(|h| h.clone());
    let Some(history) = history_data else {
        ui.label(egui::RichText::new("No data yet...").color(theme::MUTED_DARK));
        return;
    };

    if history.len() < 2 {
        ui.label(egui::RichText::new("Collecting data...").color(theme::MUTED_DARK));
        return;
    }

    let total_bytes = history
        .back()
        .map_or(1, |p| p.used_bytes + p.available_bytes);
    let total_gb = total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    let used_points: Vec<[f64; 2]> = history
        .iter()
        .map(|p| {
            [
                p.elapsed_secs,
                p.used_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            ]
        })
        .collect();
    let avail_points: Vec<[f64; 2]> = history
        .iter()
        .map(|p| {
            [
                p.elapsed_secs,
                p.available_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            ]
        })
        .collect();

    let used_line = egui_plot::Line::new("Used", used_points)
        .color(theme::CHART_USED)
        .width(1.5);
    let avail_line = egui_plot::Line::new("Available", avail_points)
        .color(theme::CHART_AVAILABLE)
        .width(1.5);

    egui_plot::Plot::new("memory_history")
        .height(150.0)
        .include_y(0.0)
        .include_y(total_gb * 1.05)
        .y_axis_label("GB")
        .x_axis_label("seconds")
        .legend(egui_plot::Legend::default().position(egui_plot::Corner::LeftTop))
        .show(ui, |plot_ui| {
            plot_ui.line(used_line);
            plot_ui.line(avail_line);
        });
}

/// Draw quick system information.
fn draw_quick_info(ui: &mut egui::Ui, snap: &stats::MemorySnapshot, dark: bool) {
    widgets::section_header(ui, "Quick Info");
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
            theme::MUTED_DARK,
            dark,
        );
    });
}
