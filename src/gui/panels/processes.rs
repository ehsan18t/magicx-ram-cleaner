//! # Processes Panel
//!
//! The programs using the most memory, grouped by executable name: several
//! instances of one program (e.g. `msedge.exe` ×34) form a single row with
//! summed sizes, like Task Manager's app grouping. Rows highlight on hover
//! and offer a Trim action that trims every instance of that program.

use std::collections::HashMap;
use std::time::Duration;

use eframe::egui;

use crate::gui::icons::regular as ph;
use crate::memory;
use crate::strings::gui::processes as text;

use super::super::app::{MagicXApp, TrimState};
use super::super::settings::TOP_PROCESSES_RANGE;
use super::super::theme::{self, Palette};
use super::super::widgets;

/// Height of a program row.
const ROW_HEIGHT: f32 = 44.0;

/// Height of the header row.
const HEADER_HEIGHT: f32 = 36.0;

/// Horizontal padding inside the list.
const PAD: f32 = 16.0;

/// Width of the instance-count column.
const COUNT_WIDTH: f32 = 64.0;

/// Width of the memory column (value and bar).
const MEMORY_WIDTH: f32 = 140.0;

/// Width of the peak column.
const PEAK_WIDTH: f32 = 90.0;

/// Width of the action column (the Trim button).
const ACTION_WIDTH: f32 = 84.0;

/// How long a trim result stays in its row.
const RESULT_SECS: f32 = 6.0;

/// Memory usage summed over every instance of one program.
#[derive(Debug, Clone)]
struct GroupedProcess {
    /// Executable name shared by all instances (e.g. `msedge.exe`).
    name: String,
    /// Lower-case name, for search and sorting.
    lower: String,
    /// Unique key for the group and its trims: the lower-case name, marked
    /// when the program is not part of Windows.
    key: String,
    /// Whether the program is part of Windows.
    windows: bool,
    /// Process IDs of the instances.
    pids: Vec<u32>,
    /// Sum of private working sets: Task Manager's "Memory" column, which
    /// does not double-count pages shared between instances.
    private_working_set: u64,
    /// Sum of full working sets, including shared pages (shown on hover).
    working_set: u64,
    /// Sum of peak working sets.
    peak_working_set: u64,
}

/// Collapse a flat process list into one entry per program.
///
/// A copy of a program running from outside Windows gets its own entry even
/// when it shares a name with a Windows process (an `svchost.exe` in a user
/// folder, say), so it can never hide inside the real one.
fn group_processes(procs: &[memory::ProcessMemoryInfo]) -> Vec<GroupedProcess> {
    let mut map: HashMap<String, GroupedProcess> = HashMap::new();
    for p in procs {
        let lower = p.name.to_lowercase();
        let key = if p.windows_process {
            lower.clone()
        } else {
            format!("{lower}|outside-windows")
        };
        map.entry(key.clone())
            .and_modify(|g| {
                g.pids.push(p.pid);
                g.private_working_set += p.private_working_set;
                g.working_set += p.working_set;
                g.peak_working_set += p.peak_working_set;
            })
            .or_insert_with(|| GroupedProcess {
                name: p.name.clone(),
                lower,
                key,
                windows: p.windows_process,
                pids: vec![p.pid],
                private_working_set: p.private_working_set,
                working_set: p.working_set,
                peak_working_set: p.peak_working_set,
            });
    }
    map.into_values().collect()
}

/// Draw the processes panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let p = theme::palette();
    widgets::page_title(ui, text::TITLE);
    draw_toolbar(ui, app, &p);
    ui.add_space(12.0);
    draw_last_trim(ui, app, &p);

    let Some(procs) = app.top_processes.lock().ok().and_then(|p| p.clone()) else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(egui::RichText::new(text::LOADING).color(p.text_secondary));
        });
        return;
    };

    let mut groups = group_processes(&procs);
    let show_windows = app.settings.show_windows_processes;
    if !show_windows {
        groups.retain(|g| !g.windows);
    }
    let program_count = groups.len();
    let query = app.process_search.trim().to_lowercase();
    if !query.is_empty() {
        groups.retain(|g| g.lower.contains(&query));
    }
    sort_processes(&mut groups, app.process_sort_col, app.process_sort_asc);
    groups.truncate(app.settings.top_process_count);

    if groups.is_empty() {
        let message = if query.is_empty() {
            text::EMPTY_FILTERED.to_owned()
        } else {
            format!(
                "No programs match \u{201c}{}\u{201d}.",
                app.process_search.trim()
            )
        };
        ui.label(egui::RichText::new(message).color(p.text_secondary));
        return;
    }

    widgets::card_with_padding(ui, 0, |ui| draw_list(ui, app, &p, &groups));

    let instances: usize = groups.iter().map(|g| g.pids.len()).sum();
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(format!(
            "Showing {} of {program_count} programs ({instances} instances), sorted by {}{}",
            groups.len(),
            text::COL_NAMES[app.process_sort_col.min(3)],
            if show_windows {
                ""
            } else {
                text::FOOTER_WINDOWS_HIDDEN
            },
        ))
        .size(theme::CAPTION)
        .color(p.text_secondary),
    );
}

/// The program count and the Windows filter on the left, search on the
/// right. On narrow windows the filter moves to a second line.
fn draw_toolbar(ui: &mut egui::Ui, app: &mut MagicXApp, p: &Palette) {
    let one_line = ui.available_width() >= 720.0;
    ui.horizontal(|ui| {
        draw_count(ui, app, p);
        if one_line {
            ui.add_space(16.0);
            draw_windows_filter(ui, app);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            search_box(ui, app, p);
        });
    });
    if !one_line {
        ui.add_space(4.0);
        draw_windows_filter(ui, app);
    }
}

/// "Show top [N] programs".
fn draw_count(ui: &mut egui::Ui, app: &mut MagicXApp, p: &Palette) {
    ui.label(egui::RichText::new(text::LABEL_SHOW_TOP).color(p.text_secondary));
    widgets::number_box(
        ui,
        "process-count",
        &mut app.settings.top_process_count,
        TOP_PROCESSES_RANGE,
    )
    .on_hover_text(text::TOOLTIP_SHOW_TOP);
    ui.label(egui::RichText::new(text::LABEL_PROGRAMS).color(p.text_secondary));
}

/// The checkbox that shows or hides Windows processes.
fn draw_windows_filter(ui: &mut egui::Ui, app: &mut MagicXApp) {
    widgets::checkbox(
        ui,
        &mut app.settings.show_windows_processes,
        text::LABEL_SHOW_WINDOWS,
    )
    .on_hover_text(text::TOOLTIP_SHOW_WINDOWS);
}

/// A search field with a magnifier inside and a clear button once it has text.
fn search_box(ui: &mut egui::Ui, app: &mut MagicXApp, p: &Palette) {
    let response = ui.add(
        egui::TextEdit::singleline(&mut app.process_search)
            .id_salt("process-search")
            .hint_text(text::SEARCH_HINT)
            .desired_width(240.0)
            .margin(egui::Margin {
                left: 30,
                right: 28,
                top: 7,
                bottom: 7,
            }),
    );
    let rect = response.rect;
    ui.painter().text(
        egui::pos2(rect.left() + 15.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        ph::MAGNIFYING_GLASS,
        egui::FontId::proportional(14.0),
        p.text_secondary,
    );
    if app.process_search.is_empty() {
        return;
    }
    let clear = egui::Rect::from_center_size(
        egui::pos2(rect.right() - 15.0, rect.center().y),
        egui::vec2(22.0, 22.0),
    );
    let clear_response = ui
        .interact(clear, ui.id().with("search-clear"), egui::Sense::click())
        .on_hover_text(text::BTN_CLEAR_SEARCH);
    if clear_response.hovered() {
        ui.painter().rect_filled(
            clear,
            egui::CornerRadius::same(theme::CONTROL_RADIUS),
            p.subtle,
        );
    }
    widgets::focus_ring(ui, &clear_response, clear, p);
    ui.painter().text(
        clear.center(),
        egui::Align2::CENTER_CENTER,
        ph::X,
        egui::FontId::proportional(12.0),
        p.text_secondary,
    );
    if clear_response.clicked() {
        app.process_search.clear();
    }
}

/// Column cells within a row rect, laid out right to left.
struct Columns {
    /// Program name.
    name: egui::Rect,
    /// Instance count.
    count: egui::Rect,
    /// Memory value and bar.
    memory: egui::Rect,
    /// Peak working set.
    peak: egui::Rect,
    /// Trim button or its result.
    action: egui::Rect,
}

impl Columns {
    /// Lay the columns out across `row`.
    fn new(row: egui::Rect) -> Self {
        let span = |left: f32, right: f32| {
            egui::Rect::from_min_max(egui::pos2(left, row.top()), egui::pos2(right, row.bottom()))
        };
        let action_right = row.right() - PAD;
        let action_left = action_right - ACTION_WIDTH;
        let peak_right = action_left - 8.0;
        let peak_left = peak_right - PEAK_WIDTH;
        let memory_right = peak_left - 16.0;
        let memory_left = memory_right - MEMORY_WIDTH;
        let count_right = memory_left - 16.0;
        let count_left = count_right - COUNT_WIDTH;
        let name_left = row.left() + PAD;
        Self {
            name: span(name_left, count_left.max(name_left + 40.0)),
            count: span(count_left, count_right),
            memory: span(memory_left, memory_right),
            peak: span(peak_left, peak_right),
            action: span(action_left, action_right),
        }
    }
}

/// The header row and the program rows.
fn draw_list(ui: &mut egui::Ui, app: &mut MagicXApp, p: &Palette, groups: &[GroupedProcess]) {
    ui.spacing_mut().item_spacing.y = 0.0;
    draw_header(ui, app, p);
    widgets::divider(ui);
    let max = groups
        .iter()
        .map(|g| g.private_working_set)
        .max()
        .unwrap_or(1)
        .max(1);
    let trims = app
        .trim_log
        .lock()
        .map(|log| log.by_program.clone())
        .unwrap_or_default();
    for group in groups {
        if let Some(pids) = draw_row(ui, p, group, max, trims.get(&group.key)) {
            app.trim_program(&group.key, &group.name, pids);
        }
    }
}

/// A short confirmation of the latest trim, above the list.
fn draw_last_trim(ui: &mut egui::Ui, app: &MagicXApp, p: &Palette) {
    let last = app.trim_log.lock().ok().and_then(|log| log.last.clone());
    let Some((name, report, at)) = last else {
        return;
    };
    if at.elapsed().as_secs_f32() >= RESULT_SECS {
        return;
    }
    let freed = memory::format_bytes(report.freed_bytes);
    let message = if report.skipped > 0 {
        format!(
            "Trimmed {name}: freed {freed}. Windows protects {} of its {} processes, so those were skipped.",
            report.skipped, report.total
        )
    } else {
        format!("Trimmed {name}: freed {freed}")
    };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(ph::CHECK).color(p.success));
        ui.label(egui::RichText::new(message).color(p.text));
    });
    ui.add_space(8.0);
}

/// Clickable column headings; clicking sorts, clicking again reverses.
fn draw_header(ui: &mut egui::Ui, app: &mut MagicXApp, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), HEADER_HEIGHT),
        egui::Sense::hover(),
    );
    let cols = Columns::new(rect);
    let headings = [
        (0, text::COL_PROCESS, cols.name, egui::Align::Min),
        (1, text::COL_COUNT, cols.count, egui::Align::Max),
        (2, text::COL_MEMORY, cols.memory, egui::Align::Min),
        (3, text::COL_PEAK, cols.peak, egui::Align::Max),
    ];
    for (col, label, cell, align) in headings {
        let sorted = app.process_sort_col == col;
        let label = if sorted {
            let caret = if app.process_sort_asc {
                ph::CARET_UP
            } else {
                ph::CARET_DOWN
            };
            format!("{label} {caret}")
        } else {
            label.to_owned()
        };
        let response = ui.interact(cell, ui.id().with(("sort", col)), egui::Sense::click());
        let color = if sorted || response.hovered() {
            p.text
        } else {
            p.text_secondary
        };
        let weight = if sorted { theme::SEMIBOLD } else { 400.0 };
        paint_text(ui, cell, &label, (theme::CAPTION, weight), color, align);
        widgets::focus_ring(ui, &response, cell, p);
        if response.clicked() {
            if sorted {
                app.process_sort_asc = !app.process_sort_asc;
            } else {
                app.process_sort_col = col;
                app.process_sort_asc = col == 0;
            }
        }
    }
}

/// One program row. Returns the PIDs to trim when its Trim button is clicked.
fn draw_row(
    ui: &mut egui::Ui,
    p: &Palette,
    group: &GroupedProcess,
    max: u64,
    trim: Option<&TrimState>,
) -> Option<Vec<u32>> {
    // Rows take keyboard focus so Trim works without a mouse: a focused row
    // shows the Trim button, and Enter or Space runs it.
    let (rect, row) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_HEIGHT),
        egui::Sense::focusable_noninteractive(),
    );
    let focused = row.has_focus();
    let active = ui.rect_contains_pointer(rect) || focused;
    let highlight = rect.shrink2(egui::vec2(4.0, 2.0));
    if active {
        ui.painter().rect_filled(
            highlight,
            egui::CornerRadius::same(theme::CONTROL_RADIUS),
            p.subtle,
        );
    }
    widgets::focus_ring(ui, &row, highlight, p);
    let key_trim =
        focused && ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space));
    let cols = Columns::new(rect);
    draw_name_cell(ui, p, cols.name, group, trim);
    paint_text(
        ui,
        cols.count,
        &group.pids.len().to_string(),
        (theme::BODY, 400.0),
        p.text_secondary,
        egui::Align::Max,
    );
    draw_memory_cell(ui, p, cols.memory, group.private_working_set, max);
    paint_text(
        ui,
        cols.peak,
        &memory::format_bytes(group.peak_working_set),
        (theme::CAPTION, 400.0),
        p.text_secondary,
        egui::Align::Max,
    );
    let clicked = draw_action(ui, p, cols.action, active, trim);
    let running = matches!(trim, Some(TrimState::Running));
    ((clicked || key_trim) && !running).then(|| group.pids.clone())
}

/// The program name, with the hover details, and a second line while a trim
/// runs or shortly after it finished.
fn draw_name_cell(
    ui: &egui::Ui,
    p: &Palette,
    cell: egui::Rect,
    group: &GroupedProcess,
    trim: Option<&TrimState>,
) {
    ui.interact(
        cell,
        ui.id().with(("name", &group.key)),
        egui::Sense::hover(),
    )
    .on_hover_text(format!(
        "{}\n{}\nPrivate working set: {}\nFull working set: {}",
        group.name,
        if group.windows {
            text::ORIGIN_WINDOWS
        } else {
            text::ORIGIN_OUTSIDE
        },
        memory::format_bytes(group.private_working_set),
        memory::format_bytes(group.working_set),
    ));
    let Some((note, color)) = trim_note(p, trim) else {
        paint_text(
            ui,
            cell,
            &group.name,
            (theme::BODY, 400.0),
            p.text,
            egui::Align::Min,
        );
        return;
    };
    let name = egui::Rect::from_min_max(cell.min, egui::pos2(cell.right(), cell.center().y + 2.0));
    let note_cell = egui::Rect::from_min_max(
        egui::pos2(cell.left(), cell.center().y + 2.0),
        cell.max - egui::vec2(0.0, 4.0),
    );
    paint_text(
        ui,
        name,
        &group.name,
        (theme::BODY, 400.0),
        p.text,
        egui::Align::Min,
    );
    paint_text(
        ui,
        note_cell,
        &note,
        (theme::CAPTION, 400.0),
        color,
        egui::Align::Min,
    );
}

/// The line shown under a program's name for its trim, if any.
fn trim_note(p: &Palette, trim: Option<&TrimState>) -> Option<(String, egui::Color32)> {
    match trim? {
        TrimState::Running => Some((text::TRIMMING.to_owned(), p.text_secondary)),
        TrimState::Done(report, at) if at.elapsed().as_secs_f32() < RESULT_SECS => {
            let freed = memory::format_bytes(report.freed_bytes);
            let summary = if report.skipped > 0 {
                format!(
                    "Freed {freed}, {} of {} skipped",
                    report.skipped, report.total
                )
            } else {
                format!("Freed {freed}")
            };
            Some((summary, p.success))
        }
        TrimState::Done(..) => None,
    }
}

/// The memory value with a bar showing its share of the largest program.
fn draw_memory_cell(ui: &egui::Ui, p: &Palette, cell: egui::Rect, bytes: u64, max: u64) {
    let value = egui::Rect::from_min_max(
        egui::pos2(cell.left(), cell.top() + 6.0),
        egui::pos2(cell.right(), cell.center().y + 4.0),
    );
    paint_text(
        ui,
        value,
        &memory::format_bytes(bytes),
        (theme::BODY, theme::SEMIBOLD),
        p.text,
        egui::Align::Min,
    );
    let bar = egui::Rect::from_min_size(
        egui::pos2(cell.left(), cell.center().y + 8.0),
        egui::vec2(cell.width(), 3.0),
    );
    ui.painter()
        .rect_filled(bar, egui::CornerRadius::same(2), p.well);
    let fraction = (bytes as f32 / max as f32).clamp(0.0, 1.0);
    let fill = egui::Rect::from_min_size(bar.min, egui::vec2(bar.width() * fraction, bar.height()));
    ui.painter()
        .rect_filled(fill, egui::CornerRadius::same(2), p.in_use);
}

/// The action column: a spinner while a trim runs, otherwise the Trim
/// button on the hovered or focused row. Returns whether it was clicked.
fn draw_action(
    ui: &mut egui::Ui,
    p: &Palette,
    cell: egui::Rect,
    active: bool,
    trim: Option<&TrimState>,
) -> bool {
    match trim {
        Some(TrimState::Running) => {
            let spinner = egui::Rect::from_center_size(
                egui::pos2(cell.right() - 34.0, cell.center().y),
                egui::vec2(16.0, 16.0),
            );
            ui.place(spinner, egui::Spinner::new().size(14.0));
            return false;
        }
        Some(TrimState::Done(_, at)) if at.elapsed().as_secs_f32() < RESULT_SECS => {
            // Keep repainting until the result under the name times out.
            ui.ctx().request_repaint_after(Duration::from_millis(500));
        }
        _ => {}
    }
    if !active {
        return false;
    }
    // Click-only (not focusable): keyboard users trim from the focused row,
    // so Tab never lands on a button that hides once the row loses focus.
    let button = egui::Rect::from_center_size(
        egui::pos2(cell.right() - 34.0, cell.center().y),
        egui::vec2(68.0, 28.0),
    );
    let response = ui
        .interact(
            button,
            ui.id().with(("trim", cell.top().to_bits())),
            egui::Sense::CLICK,
        )
        .on_hover_text(text::TOOLTIP_TRIM);
    let radius = egui::CornerRadius::same(theme::CONTROL_RADIUS);
    let fill = if response.hovered() {
        p.control_hover
    } else {
        p.control
    };
    ui.painter().rect_filled(button, radius, fill);
    ui.painter().rect_stroke(
        button,
        radius,
        egui::Stroke::new(1.0_f32, p.control_stroke),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        button.center(),
        egui::Align2::CENTER_CENTER,
        text::BTN_TRIM,
        egui::FontId::proportional(theme::CAPTION),
        p.text,
    );
    response.clicked()
}

/// Paint one line of text in `cell` at `(size, weight)`, vertically
/// centred and elided to fit.
fn paint_text(
    ui: &egui::Ui,
    cell: egui::Rect,
    content: &str,
    (size, weight): (f32, f32),
    color: egui::Color32,
    align: egui::Align,
) {
    let galley = ui.painter().layout_job(widgets::single_line_job(
        content,
        size,
        color,
        weight,
        cell.width(),
    ));
    let x = match align {
        egui::Align::Max => cell.right() - galley.size().x,
        egui::Align::Center => cell.center().x - galley.size().x / 2.0,
        egui::Align::Min => cell.left(),
    };
    let pos = egui::pos2(x, cell.center().y - galley.size().y / 2.0);
    ui.painter().with_clip_rect(cell).galley(pos, galley, color);
}

/// Sort groups by a column, with tie-breakers so equal rows keep a stable
/// order across refreshes (otherwise they would reshuffle every 5 s).
///
/// | Primary | Then by              |
/// |---------|----------------------|
/// | Name    | (unique)             |
/// | Count   | Memory, Peak, Name   |
/// | Memory  | Peak, Count, Name    |
/// | Peak    | Memory, Count, Name  |
fn sort_processes(groups: &mut [GroupedProcess], col: usize, ascending: bool) {
    groups.sort_unstable_by(|a, b| {
        let ord = match col {
            0 => a.lower.cmp(&b.lower).then_with(|| a.key.cmp(&b.key)),
            1 => a
                .pids
                .len()
                .cmp(&b.pids.len())
                .then_with(|| a.private_working_set.cmp(&b.private_working_set))
                .then_with(|| a.peak_working_set.cmp(&b.peak_working_set))
                .then_with(|| a.name.cmp(&b.name)),
            3 => a
                .peak_working_set
                .cmp(&b.peak_working_set)
                .then_with(|| a.private_working_set.cmp(&b.private_working_set))
                .then_with(|| a.pids.len().cmp(&b.pids.len()))
                .then_with(|| a.name.cmp(&b.name)),
            _ => a
                .private_working_set
                .cmp(&b.private_working_set)
                .then_with(|| a.peak_working_set.cmp(&b.peak_working_set))
                .then_with(|| a.pids.len().cmp(&b.pids.len()))
                .then_with(|| a.name.cmp(&b.name)),
        };
        if ascending { ord } else { ord.reverse() }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(pid: u32, name: &str, private: u64) -> memory::ProcessMemoryInfo {
        memory::ProcessMemoryInfo {
            pid,
            name: name.to_owned(),
            working_set: private * 2,
            peak_working_set: private * 3,
            private_working_set: private,
            windows_process: false,
        }
    }

    #[test]
    fn a_copy_outside_windows_never_merges_with_the_windows_program() {
        let real = memory::ProcessMemoryInfo {
            windows_process: true,
            ..info(1, "svchost.exe", 100)
        };
        let impostor = info(2, "svchost.exe", 40);
        let groups = group_processes(&[real, impostor]);
        assert_eq!(groups.len(), 2);
        let outside = groups.iter().find(|g| !g.windows).expect("separate group");
        assert_eq!(outside.private_working_set, 40);
        assert_ne!(
            outside.key,
            groups.iter().find(|g| g.windows).expect("real").key
        );
    }

    #[test]
    fn instances_of_a_program_are_grouped_case_insensitively() {
        let groups = group_processes(&[
            info(1, "msedge.exe", 100),
            info(2, "MSEdge.exe", 50),
            info(3, "code.exe", 70),
        ]);
        let edge = groups
            .iter()
            .find(|g| g.lower == "msedge.exe")
            .expect("grouped");
        assert_eq!(edge.pids.len(), 2);
        assert_eq!(edge.private_working_set, 150);
        assert_eq!(edge.peak_working_set, 450);
    }

    #[test]
    fn memory_sort_is_descending_by_default_with_stable_ties() {
        let mut groups = group_processes(&[
            info(1, "b.exe", 10),
            info(2, "a.exe", 10),
            info(3, "c.exe", 30),
        ]);
        sort_processes(&mut groups, 2, false);
        let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["c.exe", "b.exe", "a.exe"]);
    }
}
