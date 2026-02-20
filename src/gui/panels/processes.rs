//! # Processes Panel
//!
//! Sortable table of top memory-consuming programs, grouped by executable name.
//! Multiple instances of the same program (e.g. `msedge.exe ×5`) are merged
//! into a single row with summed working-set sizes — matching the style of
//! Windows Task Manager's app grouping.
//!
//! Click column headers to sort. Uses `egui_extras::TableBuilder`.
//!
//! The panel owns its own "Show top N" count control, keeping
//! process-display preferences co-located with the display itself.

use std::collections::HashMap;

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

/// Aggregated memory usage for all processes sharing the same executable name.
#[derive(Debug, Clone)]
struct GroupedProcess {
    /// Executable name shared by all grouped instances (e.g. `msedge.exe`).
    name: String,
    /// Number of running instances contributing to this group.
    instance_count: usize,
    /// Sum of private working-set sizes across all instances (bytes).
    ///
    /// This is the primary displayed metric — it matches Task Manager’s
    /// "Memory" column and avoids double-counting shared pages (DLLs)
    /// when multiple instances of the same program are grouped.
    private_working_set: u64,
    /// Sum of full working-set sizes across all instances (bytes).
    ///
    /// Includes shared pages (DLLs, mapped files). Shown as a tooltip
    /// detail — not the primary column — because summing it across
    /// grouped instances inflates the total relative to Task Manager.
    working_set: u64,
    /// Sum of peak working-set sizes across all instances (bytes).
    peak_working_set: u64,
}

/// Collapse a flat process list into per-program groups.
///
/// Uses a [`HashMap`] for O(n) grouping. Entries with the same
/// [`name`](stats::ProcessMemoryInfo::name) (case-insensitive) are merged:
/// instance counts and all memory metrics are summed.
fn group_processes(procs: &[stats::ProcessMemoryInfo]) -> Vec<GroupedProcess> {
    let mut map: HashMap<String, GroupedProcess> = HashMap::new();

    for p in procs {
        let key = p.name.to_lowercase();
        map.entry(key)
            .and_modify(|g| {
                g.instance_count += 1;
                g.private_working_set += p.private_working_set;
                g.working_set += p.working_set;
                g.peak_working_set += p.peak_working_set;
            })
            .or_insert_with(|| GroupedProcess {
                name: p.name.clone(),
                instance_count: 1,
                private_working_set: p.private_working_set,
                working_set: p.working_set,
                peak_working_set: p.peak_working_set,
            });
    }

    map.into_values().collect()
}

/// Draw the processes panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;
    widgets::page_title(ui, ph::CPU, "Top Processes", dark);

    draw_toolbar(ui, app);
    ui.add_space(8.0);

    let procs = app.top_processes.lock().ok().map(|p| p.clone());

    let Some(procs) = procs else {
        ui.spinner();
        return;
    };

    let mut grouped = group_processes(&procs);

    // Filter by search text (case-insensitive substring match).
    let query = app.process_search.trim().to_lowercase();
    if !query.is_empty() {
        grouped.retain(|g| g.name.to_lowercase().contains(&query));
    }

    // Sort first so the slice we show is the correct top-N by the active column.
    sort_processes(&mut grouped, app.process_sort_col, app.process_sort_asc);
    grouped.truncate(app.settings.top_process_count);
    let max_ws = grouped
        .iter()
        .map(|g| g.private_working_set)
        .max()
        .unwrap_or(1)
        .max(1);
    draw_table_card(ui, app, &grouped, max_ws, dark);
}

/// Draw the card containing the info row and sortable process table.
fn draw_table_card(
    ui: &mut egui::Ui,
    app: &mut MagicXApp,
    groups: &[GroupedProcess],
    max_ws: u64,
    dark: bool,
) {
    widgets::card(ui, dark, |ui| {
        let total_instances: usize = groups.iter().map(|g| g.instance_count).sum();
        ui.label(
            egui::RichText::new(format!(
                "{} programs  ({} instances)  \u{b7}  sorted by {}",
                groups.len(),
                total_instances,
                col_name(app.process_sort_col)
            ))
            .size(11.0)
            .color(theme::muted_color(dark)),
        );
        ui.add_space(6.0);

        TableBuilder::new(ui)
            .striped(true)
            .vscroll(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::remainder().at_least(140.0)) // Name
            .column(Column::exact(58.0))                 // Instance count
            .column(Column::exact(115.0))                // Working Set + bar
            .column(Column::exact(90.0))                 // Peak
            .header(HEADER_HEIGHT, |mut header| {
                let cols: &[(&str, usize)] =
                    &[("Process", 0), ("Count", 1), ("Memory", 2), ("Peak", 3)];
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
                body.rows(ROW_HEIGHT, groups.len(), |mut row| {
                    let idx = row.index();
                    let g = &groups[idx];
                    row.col(|ui| {
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(&g.name)
                                .size(12.0)
                                .color(theme::text_color(dark)),
                        );
                    });
                    row.col(|ui| {
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.add_space(4.0);
                                let label = if g.instance_count > 1 {
                                    format!("{}×", g.instance_count)
                                } else {
                                    "1".to_owned()
                                };
                                ui.label(
                                    egui::RichText::new(label)
                                        .size(11.0)
                                        .color(theme::muted_color(dark)),
                                );
                            },
                        );
                    });
                    row.col(|ui| {
                        let resp = draw_ws_cell(ui, g.private_working_set, max_ws);
                        resp.on_hover_text(format!(
                            "Private: {}\nFull WS: {}",
                            stats::format_bytes(g.private_working_set),
                            stats::format_bytes(g.working_set),
                        ));
                    });
                    row.col(|ui| {
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(stats::format_bytes(
                                        g.peak_working_set,
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
///
/// Returns the [`egui::Response`] so callers can attach tooltips.
fn draw_ws_cell(ui: &mut egui::Ui, working_set: u64, max_ws: u64) -> egui::Response {
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
    })
    .response
}

/// Combined toolbar: "Show top" slider on the left, search box on the right.
fn draw_toolbar(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;
    ui.horizontal(|ui| {
        // ── Left: count slider ─────────────────────────────────────────────
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
            .suffix(" programs")
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

        // ── Right: search box ──────────────────────────────────────────────
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Always render the clear button so the widget tree is stable
            // across frames — conditional widgets cause egui to drop the
            // TextEdit's focus after the first keystroke.
            let has_text = !app.process_search.is_empty();
            if ui
                .add_enabled(has_text, egui::Button::new(ph::X).small())
                .on_hover_text("Clear search")
                .clicked()
            {
                app.process_search.clear();
            }
            let edit = egui::TextEdit::singleline(&mut app.process_search)
                .hint_text("search…")
                .desired_width(140.0);
            ui.add(edit);
            ui.label(
                egui::RichText::new(ph::MAGNIFYING_GLASS)
                    .size(13.0)
                    .color(theme::muted_color(dark)),
            );
        });
    });
}

/// Human-readable name for a sort column index.
const fn col_name(col: usize) -> &'static str {
    match col {
        0 => "name",
        1 => "count",
        3 => "peak",
        _ => "memory",
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

/// Sort grouped processes by the specified column.
///
/// Uses a **deterministic tiebreaker** (case-insensitive name) so that rows
/// with identical primary values keep a stable order across refreshes.
/// Without this, `sort_unstable_by` reshuffles equal elements every 5 s,
/// producing a visually glitchy flickering effect in the table.
fn sort_processes(groups: &mut [GroupedProcess], col: usize, ascending: bool) {
    groups.sort_unstable_by(|a, b| {
        let primary = match col {
            0 => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            1 => a.instance_count.cmp(&b.instance_count),
            3 => a.peak_working_set.cmp(&b.peak_working_set),
            // Default: private working set (col 2) — matches Task Manager
            _ => a.private_working_set.cmp(&b.private_working_set),
        };
        // Tiebreaker: sort by name so equal-valued rows never shuffle.
        let ord = primary.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        if ascending { ord } else { ord.reverse() }
    });
}
