//! # Processes Panel
//!
//! Sortable table of the top processes ranked by memory usage.
//! Click column headers to sort. Uses `egui_extras::TableBuilder`.

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::stats;

use super::super::app::MagicXApp;
use super::super::{theme, widgets};

/// Draw the processes panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;
    widgets::page_title(ui, "\u{2630}", "Top Processes", dark);

    let procs = app.top_processes.lock().ok().map(|p| p.clone());

    let Some(mut procs) = procs else {
        ui.spinner();
        return;
    };

    sort_processes(&mut procs, app.process_sort_col, app.process_sort_asc);

    widgets::card(ui, dark, |ui| {
        ui.label(
            egui::RichText::new(format!("Showing top {} by working set", procs.len()))
                .size(11.0)
                .color(theme::muted_color(dark)),
        );
        ui.add_space(6.0);

        let available_height = ui.available_height().max(200.0);

        TableBuilder::new(ui)
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::remainder().at_least(120.0)) // Name
            .column(Column::exact(60.0))                 // PID
            .column(Column::exact(90.0))                 // Working Set
            .column(Column::exact(90.0))                 // Peak
            .max_scroll_height(available_height)
            .header(22.0, |mut header| {
                let cols: &[(&str, usize)] =
                    &[("Process", 0), ("PID", 1), ("Working Set", 2), ("Peak", 3)];

                for &(label, col_idx) in cols {
                    header.col(|ui| {
                        draw_sort_header(ui, label, col_idx, app, dark);
                    });
                }
            })
            .body(|body| {
                body.rows(20.0, procs.len(), |mut row| {
                    let idx = row.index();
                    let p = &procs[idx];
                    row.col(|ui| {
                        ui.label(egui::RichText::new(&p.name).size(11.0));
                    });
                    row.col(|ui| {
                        ui.label(
                            egui::RichText::new(p.pid.to_string())
                                .size(11.0)
                                .color(theme::muted_color(dark)),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            egui::RichText::new(stats::format_bytes(p.working_set))
                                .size(11.0)
                                .color(theme::ACCENT),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            egui::RichText::new(stats::format_bytes(p.peak_working_set))
                                .size(11.0)
                                .color(theme::YELLOW),
                        );
                    });
                });
            });
    });
}

/// Draw a clickable column header with sort arrow.
fn draw_sort_header(
    ui: &mut egui::Ui,
    label: &str,
    col_idx: usize,
    app: &mut MagicXApp,
    dark: bool,
) {
    let is_sorted = app.process_sort_col == col_idx;
    let arrow = if is_sorted {
        if app.process_sort_asc {
            " \u{25B2}"
        } else {
            " \u{25BC}"
        }
    } else {
        ""
    };

    let text = egui::RichText::new(format!("{label}{arrow}"))
        .strong()
        .size(11.0)
        .color(if is_sorted {
            theme::ACCENT
        } else {
            theme::text_color(dark)
        });

    if ui
        .add(egui::Label::new(text).sense(egui::Sense::click()))
        .clicked()
    {
        if app.process_sort_col == col_idx {
            app.process_sort_asc = !app.process_sort_asc;
        } else {
            app.process_sort_col = col_idx;
            app.process_sort_asc = false;
        }
    }
}

/// Sort processes by the specified column.
fn sort_processes(procs: &mut [stats::ProcessMemoryInfo], col: usize, ascending: bool) {
    procs.sort_unstable_by(|a, b| {
        let ord = match col {
            0 => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            1 => a.pid.cmp(&b.pid),
            3 => a.peak_working_set.cmp(&b.peak_working_set),
            // Default: working set (col 2)
            _ => a.working_set.cmp(&b.working_set),
        };
        if ascending { ord } else { ord.reverse() }
    });
}
