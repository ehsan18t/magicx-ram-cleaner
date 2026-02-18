//! # Processes Panel
//!
//! Sortable table of top processes by memory usage, with working set and
//! peak working set columns.

use eframe::egui;

use crate::stats::{self, ProcessMemoryInfo};

use super::super::app::MagicXApp;
use super::super::{theme, widgets};

/// Draw the processes panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    widgets::page_title(ui, "\u{1F4CB} Top Processes");

    // Clone data out of the lock to satisfy `significant_drop_tightening`.
    let processes_data: Option<Vec<ProcessMemoryInfo>> =
        app.top_processes.lock().ok().map(|p| p.clone());
    let Some(procs) = processes_data else {
        ui.label(egui::RichText::new("Loading...").color(theme::MUTED));
        return;
    };

    if procs.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.spinner();
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Loading process list...").color(theme::MUTED));
        });
        return;
    }

    let sorted_procs = sort_processes(procs, app.process_sort_col, app.process_sort_asc);

    draw_table(ui, app, &sorted_procs);
}

/// Sort the process list by the selected column.
fn sort_processes(
    mut procs: Vec<ProcessMemoryInfo>,
    col: usize,
    ascending: bool,
) -> Vec<ProcessMemoryInfo> {
    match col {
        0 => procs.sort_by(|a, b| a.name.cmp(&b.name)),
        1 => procs.sort_by(|a, b| a.pid.cmp(&b.pid)),
        2 => procs.sort_by(|a, b| a.working_set.cmp(&b.working_set)),
        3 => procs.sort_by(|a, b| a.peak_working_set.cmp(&b.peak_working_set)),
        _ => {}
    }
    if !ascending {
        procs.reverse();
    }
    procs
}

/// Draw the sortable process table.
fn draw_table(ui: &mut egui::Ui, app: &mut MagicXApp, sorted_procs: &[ProcessMemoryInfo]) {
    let table = egui_extras::TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(egui_extras::Column::initial(220.0).at_least(100.0))
        .column(egui_extras::Column::initial(70.0).at_least(50.0))
        .column(egui_extras::Column::initial(130.0).at_least(80.0))
        .column(egui_extras::Column::initial(130.0).at_least(80.0))
        .min_scrolled_height(0.0);

    table
        .header(28.0, |mut header| {
            let cols = ["Process", "PID", "Working Set", "Peak"];
            for (i, col_name) in cols.iter().enumerate() {
                header.col(|ui| {
                    draw_column_header(ui, col_name, i, app);
                });
            }
        })
        .body(|body| {
            body.rows(24.0, sorted_procs.len(), |mut row| {
                let idx = row.index();
                let p = &sorted_procs[idx];
                row.col(|ui| {
                    ui.label(egui::RichText::new(&p.name).size(12.0));
                });
                row.col(|ui| {
                    ui.label(
                        egui::RichText::new(p.pid.to_string())
                            .size(12.0)
                            .color(theme::MUTED),
                    );
                });
                row.col(|ui| {
                    ui.label(
                        egui::RichText::new(stats::format_bytes(p.working_set))
                            .strong()
                            .size(12.0),
                    );
                });
                row.col(|ui| {
                    ui.label(
                        egui::RichText::new(stats::format_bytes(p.peak_working_set))
                            .size(12.0)
                            .color(theme::MUTED),
                    );
                });
            });
        });
}

/// Draw a sortable column header button.
fn draw_column_header(ui: &mut egui::Ui, name: &str, col_index: usize, app: &mut MagicXApp) {
    let arrow = if app.process_sort_col == col_index {
        if app.process_sort_asc {
            " \u{25B2}"
        } else {
            " \u{25BC}"
        }
    } else {
        ""
    };

    if ui
        .add(
            egui::Button::new(
                egui::RichText::new(format!("{name}{arrow}"))
                    .strong()
                    .size(12.0),
            )
            .frame(false),
        )
        .clicked()
    {
        if app.process_sort_col == col_index {
            app.process_sort_asc = !app.process_sort_asc;
        } else {
            app.process_sort_col = col_index;
            // Ascending for name/pid, descending for memory columns
            app.process_sort_asc = col_index < 2;
        }
    }
}
