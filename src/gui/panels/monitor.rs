//! # Monitor Panel
//!
//! Auto-clean and the memory it watches: an on/off row, a chart of the last
//! ten minutes in the memory-list colours with the auto-clean threshold
//! drawn on it, the rules (threshold, cooldown, level), and the events
//! auto-clean produced.
//!
//! The chart stacks In use, Modified, Standby and Free from the bottom. The
//! top of the Modified band is the memory load, which is what the threshold
//! is compared with, so the line and the band meet exactly when a clean
//! would start.

use std::time::Instant;

use eframe::egui;

use crate::engine::CleanLevel;
use crate::gui::icons::regular as ph;
use crate::memory::MemoryList;
use crate::strings::gui::monitor as text;

use super::super::app::history::{self, History, Sample};
use super::super::app::{EventKind, MagicXApp};
use super::super::settings::{COOLDOWN_RANGE_SECS, THRESHOLD_RANGE};
use super::super::theme::{self, Palette};
use super::super::widgets;

/// Height of the chart's plot area.
const CHART_HEIGHT: f32 = 160.0;

/// Width of the y-axis label gutter.
const AXIS_GUTTER: f32 = 38.0;

/// Most events shown in the activity list.
const EVENTS_SHOWN: usize = 50;

/// Memory lists in stacking order, bottom first.
const STACK: [MemoryList; 4] = [
    MemoryList::InUse,
    MemoryList::Modified,
    MemoryList::Standby,
    MemoryList::Free,
];

/// The auto-clean levels in picker order.
const LEVELS: [CleanLevel; 4] = [
    CleanLevel::Gentle,
    CleanLevel::Moderate,
    CleanLevel::Aggressive,
    CleanLevel::Nuclear,
];

/// Draw the monitoring panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let p = theme::palette();
    widgets::page_title(ui, text::TITLE);

    widgets::card(ui, app.dark(), |ui| draw_switch_row(ui, app));

    ui.add_space(theme::SECTION_SPACING);
    let history = app.history.lock().map(|h| h.clone()).unwrap_or_default();
    widgets::card(ui, app.dark(), |ui| {
        draw_chart(ui, &p, &history, app.settings.monitor_threshold);
    });

    ui.add_space(theme::SECTION_SPACING);
    widgets::section_header(ui, text::SECTION_RULES);
    widgets::card(ui, app.dark(), |ui| draw_rules(ui, app));

    ui.add_space(theme::SECTION_SPACING);
    draw_activity(ui, app, &p);
}

/// Auto-clean on or off, with what it will do.
fn draw_switch_row(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let status = if app.monitor_active {
        format!(
            "On: runs {} when memory reaches {}%",
            app.settings.default_clean_level.title_case_name(),
            app.settings.monitor_threshold
        )
    } else {
        text::STATUS_OFF.to_owned()
    };
    widgets::settings_row(ui, ph::BROOM, text::LABEL_AUTO_CLEAN, &status, |ui| {
        widgets::toggle_switch(ui, &mut app.monitor_active);
    });
    // Keep the persisted setting in step with the switch.
    app.settings.auto_clean_enabled = app.monitor_active;
}

// ─── Chart ───────────────────────────────────────────────────────────────────

/// The stacked history chart with its threshold line, legend and axes.
fn draw_chart(ui: &mut egui::Ui, p: &Palette, history: &History, threshold: u32) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(text::CHART_TITLE)
                .size(theme::CAPTION)
                .color(p.text_secondary),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for list in STACK.iter().rev() {
                legend_item(ui, p, *list);
            }
        });
    });
    ui.add_space(8.0);

    let (outer, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), CHART_HEIGHT + 20.0),
        egui::Sense::hover(),
    );
    let plot = egui::Rect::from_min_max(
        egui::pos2(outer.left() + AXIS_GUTTER, outer.top() + 4.0),
        egui::pos2(outer.right(), outer.top() + 4.0 + CHART_HEIGHT),
    );
    draw_axes(ui, p, plot);

    if history.samples().len() < 2 {
        ui.painter().text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            text::CHART_EMPTY,
            egui::FontId::proportional(theme::CAPTION),
            p.text_secondary,
        );
        return;
    }

    let now = Instant::now();
    let x_of = |at: Instant| {
        let age = now.saturating_duration_since(at).as_secs_f32();
        (age / history::WINDOW.as_secs_f32())
            .mul_add(-plot.width(), plot.right())
            .max(plot.left())
    };
    let y_of = |share: f32| share.clamp(0.0, 1.0).mul_add(-plot.height(), plot.bottom());

    let mut mesh = egui::Mesh::default();
    for run in history.runs() {
        for pair in run.windows(2) {
            add_bands(&mut mesh, p, &pair[0], &pair[1], &x_of, &y_of);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));

    draw_threshold(ui, p, plot, y_of(threshold as f32 / 100.0), threshold);

    if let Some(pos) = response.hover_pos()
        && plot.contains(pos)
        && let Some(sample) = nearest_sample(history, pos.x, &x_of)
    {
        let x = x_of(sample.at);
        ui.painter().line_segment(
            [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
            egui::Stroke::new(1.0_f32, p.text_secondary),
        );
        response.on_hover_text(sample_text(sample, now));
    }
}

/// One legend entry: a swatch and the list's name.
fn legend_item(ui: &mut egui::Ui, p: &Palette, list: MemoryList) {
    let name = match list {
        MemoryList::InUse => crate::strings::gui::overview::LIST_IN_USE,
        MemoryList::Modified => crate::strings::gui::overview::LIST_MODIFIED,
        MemoryList::Standby => crate::strings::gui::overview::LIST_STANDBY,
        MemoryList::Free => crate::strings::gui::overview::LIST_FREE,
    };
    ui.label(
        egui::RichText::new(name)
            .size(theme::CAPTION)
            .color(p.text_secondary),
    );
    let (swatch, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(swatch, egui::CornerRadius::same(2), p.list(list));
}

/// Gridlines at every quarter, percentage labels, and the time labels.
fn draw_axes(ui: &egui::Ui, p: &Palette, plot: egui::Rect) {
    let painter = ui.painter();
    for quarter in 0..=4_u8 {
        let share = f32::from(quarter) / 4.0;
        let y = share.mul_add(-plot.height(), plot.bottom());
        painter.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            egui::Stroke::new(1.0_f32, p.divider),
        );
        painter.text(
            egui::pos2(plot.left() - 8.0, y),
            egui::Align2::RIGHT_CENTER,
            format!("{}%", u32::from(quarter) * 25),
            egui::FontId::proportional(theme::CAPTION - 1.0),
            p.text_tertiary,
        );
    }
    let label_y = plot.bottom() + 4.0;
    painter.text(
        egui::pos2(plot.left(), label_y),
        egui::Align2::LEFT_TOP,
        text::CHART_START,
        egui::FontId::proportional(theme::CAPTION - 1.0),
        p.text_tertiary,
    );
    painter.text(
        egui::pos2(plot.right(), label_y),
        egui::Align2::RIGHT_TOP,
        text::CHART_NOW,
        egui::FontId::proportional(theme::CAPTION - 1.0),
        p.text_tertiary,
    );
}

/// Add the stacked bands between two consecutive samples to `mesh`.
fn add_bands(
    mesh: &mut egui::Mesh,
    p: &Palette,
    a: &Sample,
    b: &Sample,
    x_of: &impl Fn(Instant) -> f32,
    y_of: &impl Fn(f32) -> f32,
) {
    let (xa, xb) = (x_of(a.at), x_of(b.at));
    if xb <= xa {
        return;
    }
    // Without page lists, show the load as In use and the rest as Free.
    let shares = |s: &Sample| s.lists.unwrap_or([s.load, 0.0, 0.0, 1.0 - s.load]);
    let (sa, sb) = (shares(a), shares(b));
    let (mut base_a, mut base_b) = (0.0, 0.0);
    for (i, list) in STACK.iter().enumerate() {
        let (top_a, top_b) = (base_a + sa[i], base_b + sb[i]);
        add_quad(
            mesh,
            [
                egui::pos2(xa, y_of(base_a)),
                egui::pos2(xa, y_of(top_a)),
                egui::pos2(xb, y_of(top_b)),
                egui::pos2(xb, y_of(base_b)),
            ],
            p.list(*list),
        );
        (base_a, base_b) = (top_a, top_b);
    }
}

/// Add a filled quadrilateral (corners in order) to `mesh`.
fn add_quad(mesh: &mut egui::Mesh, corners: [egui::Pos2; 4], color: egui::Color32) {
    let first = mesh.vertices.len() as u32;
    for corner in corners {
        mesh.colored_vertex(corner, color);
    }
    mesh.add_triangle(first, first + 1, first + 2);
    mesh.add_triangle(first, first + 2, first + 3);
}

/// The dashed auto-clean threshold line and its label.
fn draw_threshold(ui: &egui::Ui, p: &Palette, plot: egui::Rect, y: f32, threshold: u32) {
    let line = [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)];
    ui.painter().extend(egui::Shape::dashed_line(
        &line,
        egui::Stroke::new(1.5_f32, p.text),
        6.0,
        4.0,
    ));
    let label = format!("Auto-clean at {threshold}%");
    let galley =
        ui.painter()
            .layout_no_wrap(label, egui::FontId::proportional(theme::CAPTION), p.text);
    let padding = egui::vec2(6.0, 2.0);
    let size = galley.size() + padding * 2.0;
    let rect = egui::Rect::from_min_size(
        egui::pos2(
            plot.right() - size.x - 4.0,
            (y - size.y - 3.0).max(plot.top()),
        ),
        size,
    );
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(theme::CONTROL_RADIUS),
        p.card.gamma_multiply(0.9),
    );
    ui.painter().galley(rect.min + padding, galley, p.text);
}

/// The sample closest to `x`.
fn nearest_sample<'a>(
    history: &'a History,
    x: f32,
    x_of: &impl Fn(Instant) -> f32,
) -> Option<&'a Sample> {
    history
        .samples()
        .iter()
        .min_by(|a, b| (x_of(a.at) - x).abs().total_cmp(&(x_of(b.at) - x).abs()))
}

/// Hover text for one sample.
fn sample_text(sample: &Sample, now: Instant) -> String {
    let age = now.saturating_duration_since(sample.at).as_secs();
    let when = if age < 2 {
        "Now".to_owned()
    } else if age < 60 {
        format!("{age} s ago")
    } else {
        format!("{} min {} s ago", age / 60, age % 60)
    };
    let percent = |share: f32| (share * 100.0).round();
    sample.lists.map_or_else(
        || format!("{when}\nMemory load {:.0}%", percent(sample.load)),
        |[in_use, modified, standby, free]| {
            format!(
                "{when}\nMemory load {:.0}%\nIn use {:.0}%, Modified {:.0}%\nStandby {:.0}%, Free {:.0}%",
                percent(sample.load),
                percent(in_use),
                percent(modified),
                percent(standby),
                percent(free),
            )
        },
    )
}

// ─── Rules ───────────────────────────────────────────────────────────────────

/// Threshold, cooldown and the level auto-clean runs.
fn draw_rules(ui: &mut egui::Ui, app: &mut MagicXApp) {
    widgets::settings_row(
        ui,
        ph::GAUGE,
        text::LABEL_THRESHOLD,
        text::DESC_THRESHOLD,
        |ui| {
            let mut threshold = u64::from(app.settings.monitor_threshold);
            let range = u64::from(*THRESHOLD_RANGE.start())..=u64::from(*THRESHOLD_RANGE.end());
            if widgets::slider(ui, &mut threshold, range, 1, |v| format!("{v}%")).changed() {
                app.settings.monitor_threshold = u32::try_from(threshold).unwrap_or(u32::MAX);
            }
        },
    );
    widgets::divider(ui);
    widgets::settings_row(
        ui,
        ph::CLOCK,
        text::LABEL_COOLDOWN,
        text::DESC_COOLDOWN,
        |ui| {
            widgets::slider(
                ui,
                &mut app.settings.monitor_cooldown_secs,
                COOLDOWN_RANGE_SECS,
                5,
                |v| format!("{v} s"),
            );
        },
    );
    widgets::divider(ui);
    widgets::settings_row(
        ui,
        ph::SLIDERS,
        text::LABEL_CLEAN_LEVEL,
        text::DESC_CLEAN_LEVEL,
        |ui| {
            let options = LEVELS.map(|level| (level, level.title_case_name()));
            widgets::dropdown(
                ui,
                "auto-clean-level",
                &mut app.settings.default_clean_level,
                &options,
                120.0,
            );
        },
    );
}

// ─── Activity ────────────────────────────────────────────────────────────────

/// The auto-clean events, newest first.
fn draw_activity(ui: &mut egui::Ui, app: &mut MagicXApp, p: &Palette) {
    ui.horizontal(|ui| {
        widgets::section_header(ui, text::SECTION_LOG);
        if !app.monitor_log.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                if widgets::secondary_button(ui, text::BTN_CLEAR).clicked() {
                    app.monitor_log.clear();
                }
            });
        }
    });
    widgets::card(ui, app.dark(), |ui| {
        if app.monitor_log.is_empty() {
            ui.label(egui::RichText::new(text::EMPTY_LOG).color(p.text_secondary));
            return;
        }
        for event in app.monitor_log.iter().rev().take(EVENTS_SHOWN) {
            let (icon, color) = match event.kind {
                EventKind::Cleaned => (ph::CHECK, p.success),
                EventKind::Failed => (ph::WARNING_CIRCLE, p.critical),
                EventKind::Info => (ph::ACTIVITY, p.text_secondary),
            };
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&event.time)
                        .size(theme::CAPTION)
                        .color(p.text_tertiary),
                );
                ui.label(egui::RichText::new(icon).color(color));
                ui.add(egui::Label::new(egui::RichText::new(&event.text).color(p.text)).truncate())
                    .on_hover_text(&event.text);
            });
        }
    });
}
