//! # Processes Panel
//!
//! Sortable table of the top processes ranked by memory usage.
//! Click column headers to sort. Uses `egui_extras::TableBuilder`.
//!
//! The panel owns its own "Show top N" count control, keeping
//! process-display preferences co-located with the display itself.

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use egui_phosphor::regular as ph;

use crate::stats;

use super::super::app::MagicXApp;
use super::super::{theme, widgets};

/// Row height for comfortable reading.
const ROW_HEIGHT: f32 = 26.0;

/// Header row height.
const HEADER_HEIGHT: f32 = 26.0;

/// Draw the processes panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;
    widgets::page_title(ui, ph::CPU, "Top Processes", dark);

    draw_count_control(ui, app);
    ui.add_space(8.0);

    let procs = app.top_processes.lock().ok().map(|p| p.clone());

    let Some(mut procs) = procs else {
        ui.spinner();
        return;
    };

    sort_processes(&mut procs, app.process_sort_col, app.process_sort_asc);
    let max_ws = procs.first().map_or(1, |p| p.working_set.max(1));
    draw_table_card(ui, app, &procs, max_ws, dark);
}

/// Draw the card containing the info row and sortable process table.
fn draw_table_card(
    ui: &mut egui::Ui,
    app: &mut MagicXApp,
    procs: &[stats::ProcessMemoryInfo],
    max_ws: u64,
    dark: bool,
) {
    widgets::card(ui, dark, |ui| {
        ui.label(
            egui::RichText::new(format!(
                "{} processes  \u{b7}  sorted by {}",
                procs.len(),
                col_name(app.process_sort_col)
            ))
            .size(11.0)
            .color(theme::muted_color(dark)),
        );
        ui.add_space(6.0);

        let available_height = ui.available_height().max(200.0);

        TableBuilder::new(ui)
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::remainder().at_least(140.0)) // Name
            .column(Column::exact(58.0))                 // PID
            .column(Column::exact(115.0))                // Working Set + bar
            .column(Column::exact(90.0))                 // Peak
            .max_scroll_height(available_height)
            .header(HEADER_HEIGHT, |mut header| {
                let cols: &[(&str, usize)] =
                    &[("Process", 0), ("PID", 1), ("Working Set", 2), ("Peak", 3)];
                for &(label, col_idx) in cols {
                    header.col(|ui| {
                        if col_idx == 0 {
                            draw_sort_header(ui, label, col_idx, app, dark);
                        } else {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| draw_sort_header(ui, label, col_idx, app, dark),
                            );
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, procs.len(), |mut row| {
                    let idx = row.index();
                    let p = &procs[idx];
                    row.col(|ui| {
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(&p.name)
                                .size(12.0)
                                .color(theme::text_color(dark)),
                        );
                    });
                    row.col(|ui| {
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(p.pid.to_string())
                                        .size(11.0)
                                        .color(theme::muted_color(dark)),
                                );
                            },
                        );
                    });
                    row.col(|ui| draw_ws_cell(ui, p.working_set, max_ws));
                    row.col(|ui| {
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(stats::format_bytes(
                                        p.peak_working_set,
                                    ))
                                    .size(11.0)
                                    .color(theme::YELLOW),
                                );
                            },
                        );
                    });
                });
            });
    });
}

/// Right-aligned working-set cell with a relative-usage micro bar below the value.
fn draw_ws_cell(ui: &mut egui::Ui, working_set: u64, max_ws: u64) {
    let fraction = (working_set as f32 / max_ws as f32).clamp(0.0, 1.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.add_space(3.0);
            ui.label(
                egui::RichText::new(stats::format_bytes(working_set))
                    .size(11.5)
                    .strong()
                    .color(theme::ACCENT),
            );
            let (bar, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 2.5), egui::Sense::hover());
            ui.painter().rect_filled(
                bar,
                egui::CornerRadius::same(1),
                theme::ACCENT.gamma_multiply(0.18),
            );
            let fill = egui::Rect::from_min_size(
                bar.left_top(),
                egui::vec2(bar.width() * fraction, bar.height()),
            );
            ui.painter().rect_filled(
                fill,
                egui::CornerRadius::same(1),
                theme::ACCENT.gamma_multiply(0.85),
            );
        });
    });
}

/// Compact "Show top N" control rendered between the page title and the table.
fn draw_count_control(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Show top")
                .size(11.5)
                .color(theme::muted_color(dark)),
        );
        ui.add_space(4.0);
        let mut count_f32 = app.settings.top_process_count as f32;
        let slider = egui::Slider::new(&mut count_f32, 5.0..=50.0)
            .step_by(5.0)
            .show_value(true)
            .integer()
            .suffix(" processes")
            .text("");
        if ui.add(slider).changed() {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "slider is clamped to 5..=50"
            )]
            {
                app.settings.top_process_count = count_f32 as usize;
            }
        }
    });
}

/// Human-readable name for a sort column index.
const fn col_name(col: usize) -> &'static str {
    match col {
        0 => "name",
        1 => "PID",
        3 => "peak",
        _ => "working set",
    }
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
    let header_text = if is_sorted {
        let caret = if app.process_sort_asc {
            ph::CARET_UP
        } else {
            ph::CARET_DOWN
        };
        format!("{label} {caret}")
    } else {
        label.to_owned()
    };

    let text = egui::RichText::new(header_text)
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
