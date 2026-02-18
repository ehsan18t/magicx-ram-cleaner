//! # Dashboard Panel
//!
//! Main overview showing real-time memory usage, a history chart, and
//! quick-clean action buttons.

use std::collections::VecDeque;

use eframe::egui;

use crate::stats;

use super::super::app::MagicXApp;
use super::super::{theme, widgets};

/// Draw the dashboard panel.
pub fn draw(ui: &mut egui::Ui, app: &MagicXApp) {
    widgets::page_title(ui, "\u{1F4CA} Dashboard");

    let snapshot = app.latest_snapshot.lock().ok().and_then(|s| s.clone());

    if let Some(snap) = &snapshot {
        widgets::card(ui, app.settings.dark_mode, |ui| {
            widgets::memory_overview(ui, snap, app.settings.dark_mode);
        });

        ui.add_space(theme::SECTION_SPACING);
        draw_memory_chart(ui, app);

        ui.add_space(theme::SECTION_SPACING);
        draw_quick_actions(ui, snap);
    } else {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.spinner();
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Loading memory information...")
                    .size(14.0)
                    .color(theme::MUTED),
            );
        });
    }
}

/// Draw the memory history line chart.
fn draw_memory_chart(ui: &mut egui::Ui, app: &MagicXApp) {
    widgets::section_header(ui, "Memory History");

    // Clone data out of the lock to satisfy `significant_drop_tightening`.
    let history_data: Option<VecDeque<super::super::app::HistoryPoint>> =
        app.history.lock().ok().map(|h| h.clone());
    let Some(history) = history_data else {
        ui.label(egui::RichText::new("No data yet...").color(theme::MUTED));
        return;
    };

    if history.len() < 2 {
        ui.label(egui::RichText::new("Collecting data...").color(theme::MUTED));
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
        .width(2.0);
    let avail_line = egui_plot::Line::new("Available", avail_points)
        .color(theme::CHART_AVAILABLE)
        .width(2.0);

    egui_plot::Plot::new("memory_history")
        .height(180.0)
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

/// Draw quick-action info on the dashboard.
fn draw_quick_actions(ui: &mut egui::Ui, snap: &stats::MemorySnapshot) {
    widgets::section_header(ui, "Quick Info");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 20.0;

        widgets::stat_label(
            ui,
            "Total RAM",
            &stats::format_bytes(snap.total_physical),
            theme::ACCENT,
            true, // dark_mode fallback — stat_labels work in both themes
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
            true,
        );
        widgets::stat_label(
            ui,
            "Threads",
            &snap.thread_count.to_string(),
            theme::MUTED,
            true,
        );
    });
}
