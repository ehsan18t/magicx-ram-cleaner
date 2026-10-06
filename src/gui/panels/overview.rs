//! # Overview Panel
//!
//! The memory map (installed RAM split into In use, Modified, Standby and
//! Free), a plain-language summary, and cleaning through one level picker
//! and a single Clean now button. Picking a level highlights the lists it
//! reclaims and shows what it is expected to free; after a clean the map
//! moves from its old state to the new one. See `docs/UI_DESIGN.md`.

use std::time::Instant;

use eframe::egui;

use crate::engine::{CleanLevel, CleanResult, SmartCleanResult};
use crate::gui::icons::regular as ph;
use crate::memory::{self, MemoryComposition, MemoryList, MemorySnapshot};
use crate::strings::{self, gui::overview as text};

use super::super::app::{CleanProgress, CleanResultMsg, MagicXApp, MapTransition};
use super::super::theme::{self, Palette};
use super::super::widgets;

/// The levels in picker order, with their plain descriptions.
const LEVELS: [(CleanLevel, &str); 4] = [
    (CleanLevel::Gentle, strings::levels::GENTLE_DESC),
    (CleanLevel::Moderate, strings::levels::MODERATE_DESC),
    (CleanLevel::Aggressive, strings::levels::AGGRESSIVE_DESC),
    (CleanLevel::Nuclear, strings::levels::NUCLEAR_DESC),
];

/// Memory load at which the summary turns into a pressure warning.
const PRESSURE_LOAD_PERCENT: u32 = 90;

/// Height of the memory map bar.
const MAP_HEIGHT: f32 = 32.0;

/// Opacity of memory lists the selected level does not reclaim.
const DIMMED: f32 = 0.4;

/// How long the map keeps highlighting a level after it is picked.
const PICK_HIGHLIGHT_SECS: f32 = 4.0;

/// Draw the Overview panel.
pub fn draw(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let p = theme::palette();
    widgets::page_title(ui, text::TITLE);

    let Some(snap) = app.latest_snapshot.lock().ok().and_then(|s| s.clone()) else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(egui::RichText::new(text::LOADING).color(p.text_secondary));
        });
        return;
    };

    let level = app.settings.manual_clean_level;
    let highlight = highlight_amount(ui, app);

    widgets::card(ui, app.dark(), |ui| {
        draw_summary(ui, &p, &snap);
        ui.add_space(14.0);
        let parts = map_parts(&snap, app.map_transition.as_ref(), ui.ctx());
        draw_map(ui, &p, &parts, level, highlight);
        ui.add_space(12.0);
        draw_legend(ui, &p, &parts);
    });

    ui.add_space(theme::SECTION_SPACING);
    let card = ui.scope(|ui| draw_clean_card(ui, app, &p, &snap)).response;
    remember_card_hover(ui, card.hovered() || card.contains_pointer());

    ui.add_space(theme::SECTION_SPACING);
    draw_quick_stats(ui, &p, &snap);
}

// ─── Summary ─────────────────────────────────────────────────────────────────

/// One sentence that says what the memory map shows.
fn draw_summary(ui: &mut egui::Ui, p: &Palette, snap: &MemorySnapshot) {
    if snap.memory_load_percent >= PRESSURE_LOAD_PERCENT {
        ui.label(
            theme::semibold(
                format!(
                    "Memory is under pressure: {}% is in use. A clean can free some of it.",
                    snap.memory_load_percent
                ),
                theme::BODY,
            )
            .color(p.critical),
        );
        return;
    }
    let sentence = snap.composition().map_or_else(
        || {
            format!(
                "Apps are using {} of {}.",
                memory::format_bytes(snap.used_physical),
                memory::format_bytes(snap.total_physical),
            )
        },
        |c| {
            format!(
                "Apps are using {}. Another {} is standby cache that Windows can hand back the \
                 moment an app needs it.",
                memory::format_bytes(c.in_use),
                memory::format_bytes(c.standby),
            )
        },
    );
    ui.label(egui::RichText::new(sentence).color(p.text_secondary));
}

// ─── Memory Map ──────────────────────────────────────────────────────────────

/// One segment of the memory map.
struct Part {
    /// The list it shows; `Free` stands for Available when lists are unknown.
    list: MemoryList,
    /// Legend name.
    name: &'static str,
    /// Bytes shown in the legend.
    bytes: u64,
    /// Share of the bar, 0.0 to 1.0.
    fraction: f32,
}

/// The segments to draw: the four lists, mid-transition after a clean, or
/// In use and Available when the lists are unknown.
fn map_parts(
    snap: &MemorySnapshot,
    transition: Option<&MapTransition>,
    ctx: &egui::Context,
) -> Vec<Part> {
    let Some(live) = snap.composition() else {
        let total = snap.total_physical.max(1) as f32;
        return vec![
            Part {
                list: MemoryList::InUse,
                name: text::LIST_IN_USE,
                bytes: snap.used_physical,
                fraction: snap.used_physical as f32 / total,
            },
            Part {
                list: MemoryList::Free,
                name: text::LIST_AVAILABLE,
                bytes: snap.available_physical,
                fraction: snap.available_physical as f32 / total,
            },
        ];
    };

    let shown = transition
        .and_then(|t| {
            let progress =
                t.started.elapsed().as_secs_f32() / MapTransition::DURATION.as_secs_f32();
            (progress < 1.0).then(|| {
                ctx.request_repaint();
                blend(&t.from, &t.to, ease_out_cubic(progress))
            })
        })
        .unwrap_or_else(|| fractions(&live));

    [
        (MemoryList::InUse, text::LIST_IN_USE),
        (MemoryList::Modified, text::LIST_MODIFIED),
        (MemoryList::Standby, text::LIST_STANDBY),
        (MemoryList::Free, text::LIST_FREE),
    ]
    .into_iter()
    .zip(shown)
    .map(|((list, name), fraction)| Part {
        list,
        name,
        bytes: live.bytes(list),
        fraction,
    })
    .collect()
}

/// Shares of installed RAM per list, in map order.
fn fractions(c: &MemoryComposition) -> [f32; 4] {
    let total = c.total().max(1) as f32;
    [
        c.in_use as f32 / total,
        c.modified as f32 / total,
        c.standby as f32 / total,
        c.free as f32 / total,
    ]
}

/// Shares part of the way from `from` to `to`.
fn blend(from: &MemoryComposition, to: &MemoryComposition, t: f32) -> [f32; 4] {
    let (a, b) = (fractions(from), fractions(to));
    std::array::from_fn(|i| egui::lerp(a[i]..=b[i], t))
}

/// Ease-out cubic: fast start, gentle landing.
fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// How strongly the map highlights the selected level's lists (0 to 1):
/// while the clean card is hovered, for a few seconds after a pick, and
/// while a clean runs.
fn highlight_amount(ui: &egui::Ui, app: &MagicXApp) -> f32 {
    let hovered = ui
        .data(|d| d.get_temp::<bool>(card_hover_id()))
        .unwrap_or(false);
    let recently_picked = ui
        .data(|d| d.get_temp::<Instant>(pick_time_id()))
        .is_some_and(|t| t.elapsed().as_secs_f32() < PICK_HIGHLIGHT_SECS);
    if recently_picked {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
    }
    let on = hovered || recently_picked || app.cleaning_in_progress;
    ui.ctx()
        .animate_bool_with_time(egui::Id::new("overview-highlight"), on, 0.2)
}

/// Id of the stored "clean card is hovered" flag.
fn card_hover_id() -> egui::Id {
    egui::Id::new("overview-clean-card-hovered")
}

/// Id of the stored time a level was last picked.
fn pick_time_id() -> egui::Id {
    egui::Id::new("overview-level-picked-at")
}

/// Store whether the clean card is hovered, for the next frame's highlight.
fn remember_card_hover(ui: &egui::Ui, hovered: bool) {
    ui.data_mut(|d| d.insert_temp(card_hover_id(), hovered));
}

/// The segmented bar.
fn draw_map(ui: &mut egui::Ui, p: &Palette, parts: &[Part], level: CleanLevel, highlight: f32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), MAP_HEIGHT),
        egui::Sense::hover(),
    );
    let visible: Vec<&Part> = parts.iter().filter(|part| part.fraction > 0.001).collect();
    let gap = 2.0;
    let usable = (visible.len().saturating_sub(1) as f32).mul_add(-gap, rect.width());
    let mut x = rect.left();
    let last = visible.len().saturating_sub(1);
    for (i, part) in visible.iter().enumerate() {
        let width = if i == last {
            rect.right() - x
        } else {
            (part.fraction * usable).max(2.0)
        };
        let segment = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(width, rect.height()));
        let round = |first: bool, last: bool| egui::CornerRadius {
            nw: if first { 4 } else { 0 },
            sw: if first { 4 } else { 0 },
            ne: if last { 4 } else { 0 },
            se: if last { 4 } else { 0 },
        };
        let targeted = part.list == MemoryList::Free || level.reclaims(part.list);
        let opacity = if targeted {
            1.0
        } else {
            egui::lerp(1.0..=DIMMED, highlight)
        };
        ui.painter().rect_filled(
            segment,
            round(i == 0, i == last),
            p.list(part.list).gamma_multiply(opacity),
        );
        let response = ui.interact(segment, ui.id().with(("map", i)), egui::Sense::hover());
        response.on_hover_text(format!(
            "{}: {} ({:.0}%)",
            part.name,
            memory::format_bytes(part.bytes),
            part.fraction * 100.0
        ));
        x += width + gap;
    }
}

/// The legend: swatch, name and size of each segment.
fn draw_legend(ui: &mut egui::Ui, p: &Palette, parts: &[Part]) {
    ui.columns(4, |columns| {
        for (column, part) in columns.iter_mut().zip(parts) {
            column.spacing_mut().item_spacing.y = 2.0;
            column.horizontal(|ui| {
                let (swatch, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(swatch, egui::CornerRadius::same(2), p.list(part.list));
                if part.list == MemoryList::Free {
                    // Free is a neutral fill; outline it so the swatch reads.
                    ui.painter().rect_stroke(
                        swatch,
                        egui::CornerRadius::same(2),
                        egui::Stroke::new(1.0_f32, p.text_tertiary),
                        egui::StrokeKind::Inside,
                    );
                }
                ui.label(
                    egui::RichText::new(part.name)
                        .size(theme::CAPTION)
                        .color(p.text_secondary),
                );
            });
            column.label(
                theme::semibold(memory::format_bytes(part.bytes), theme::BODY + 1.0).color(p.text),
            );
        }
    });
}

// ─── Clean Card ──────────────────────────────────────────────────────────────

/// The level picker, estimate, Clean now button, and progress or result.
fn draw_clean_card(ui: &mut egui::Ui, app: &mut MagicXApp, p: &Palette, snap: &MemorySnapshot) {
    let cleaning = app.cleaning_in_progress;
    widgets::card(ui, app.dark(), |ui| {
        let selected = LEVELS
            .iter()
            .position(|(level, _)| *level == app.settings.manual_clean_level)
            .unwrap_or(0);
        let names = LEVELS.map(|(level, _)| level.title_case_name());
        if let Some(i) = widgets::segmented(ui, &names, selected, !cleaning) {
            app.settings.manual_clean_level = LEVELS[i].0;
            ui.data_mut(|d| d.insert_temp(pick_time_id(), Instant::now()));
        }
        ui.add_space(12.0);

        let (level, description) = LEVELS
            .iter()
            .find(|(level, _)| *level == app.settings.manual_clean_level)
            .copied()
            .unwrap_or(LEVELS[0]);
        let mut clean_clicked = false;
        ui.horizontal(|ui| {
            let button_width = 124.0;
            let text_width = (ui.available_width() - button_width - 16.0).max(120.0);
            ui.allocate_ui_with_layout(
                egui::vec2(text_width, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.label(theme::semibold(estimate_text(level, snap), 15.0).color(p.text));
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(description)
                                .size(theme::CAPTION)
                                .color(p.text_secondary),
                        )
                        .wrap(),
                    );
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if cleaning {
                    text::BTN_CLEANING
                } else {
                    text::BTN_CLEAN
                };
                clean_clicked = widgets::primary_button(ui, label, !cleaning).clicked();
            });
        });
        if clean_clicked {
            app.start_clean(level);
        }

        let progress = app.clean_progress.lock().ok().and_then(|p| p.clone());
        if let Some(progress) = progress {
            ui.add_space(12.0);
            draw_progress(ui, p, &progress);
        } else if let Some(msg) = &app.last_clean_result {
            ui.add_space(12.0);
            widgets::divider(ui);
            ui.add_space(10.0);
            draw_result(ui, p, msg);
        }
    });
}

/// "Frees about X", from the level's estimate for the current memory.
fn estimate_text(level: CleanLevel, snap: &MemorySnapshot) -> String {
    snap.composition().map_or_else(
        || text::NO_ESTIMATE.to_owned(),
        |c| {
            let estimate = level.estimate(&c);
            let amount = memory::format_bytes(estimate.bytes);
            if estimate.plus_app_memory {
                format!("Frees about {amount}, plus part of app memory")
            } else {
                format!("Frees about {amount}")
            }
        },
    )
}

/// The running clean: current step and a step bar.
fn draw_progress(ui: &mut egui::Ui, p: &Palette, progress: &CleanProgress) {
    let step = progress.step.clamp(1, progress.total.max(1));
    let label = progress
        .label
        .trim_end_matches("...")
        .trim_end_matches('\u{2026}');
    let label = if label.is_empty() { "Starting" } else { label };
    let who = if progress.auto { "Auto-clean" } else { "Cleaning" };
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(
            egui::RichText::new(format!(
                "{who} ({}): {label}, step {step} of {}, {} s",
                progress.level.title_case_name(),
                progress.total.max(step),
                progress.started.elapsed().as_secs(),
            ))
            .color(p.text_secondary),
        );
    });
    ui.add_space(6.0);
    let (bar, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 4.0), egui::Sense::hover());
    ui.painter().rect_filled(bar, egui::CornerRadius::same(2), p.well);
    let fraction = step as f32 / progress.total.max(step) as f32;
    let fill = egui::Rect::from_min_size(bar.min, egui::vec2(bar.width() * fraction, bar.height()));
    ui.painter().rect_filled(fill, egui::CornerRadius::same(2), p.accent);
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(100));
}

/// The outcome of the last clean.
fn draw_result(ui: &mut egui::Ui, p: &Palette, msg: &CleanResultMsg) {
    match &msg.result {
        Ok(result) => draw_success(ui, p, msg, result),
        Err(e) => {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(ph::WARNING_CIRCLE).size(16.0).color(p.critical));
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!(
                            "The {} clean couldn\u{2019}t run: {e}",
                            msg.level.title_case_name()
                        ))
                        .color(p.critical),
                    )
                    .wrap(),
                );
            });
        }
    }
}

/// A finished clean: amount freed, time taken, failures, and the steps.
fn draw_success(ui: &mut egui::Ui, p: &Palette, msg: &CleanResultMsg, result: &SmartCleanResult) {
    let freed = memory::format_bytes(result.reclaimed_bytes().max(0) as u64);
    let who = if msg.auto { "Auto-clean freed" } else { "Freed" };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(ph::CHECK).size(16.0).color(p.success));
        ui.label(
            theme::semibold(
                format!("{who} {freed} in {:.1} s", result.total_elapsed_secs),
                theme::BODY,
            )
            .color(p.text),
        );
    });
    let failed = result.failed_count();
    if failed > 0 {
        ui.label(
            egui::RichText::new(format!(
                "{failed} of {} steps failed. Open the details to see which.",
                result.results.len()
            ))
            .size(theme::CAPTION)
            .color(p.critical),
        );
    } else {
        ui.label(
            egui::RichText::new(text::REFILL_NOTE)
                .size(theme::CAPTION)
                .color(p.text_secondary),
        );
    }
    egui::CollapsingHeader::new(
        egui::RichText::new(text::DETAILS)
            .size(theme::CAPTION)
            .color(p.text_secondary),
    )
    .id_salt("overview-clean-details")
    .show(ui, |ui| {
        for step in &result.results {
            draw_step(ui, p, step);
        }
    });
}

/// One step of a finished clean.
fn draw_step(ui: &mut egui::Ui, p: &Palette, step: &CleanResult) {
    let (icon, color) = if step.success {
        (ph::CHECK, p.success)
    } else {
        (ph::X, p.critical)
    };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon).size(theme::CAPTION).color(color));
        ui.label(
            egui::RichText::new(&step.operation)
                .size(theme::CAPTION)
                .color(p.text),
        );
        let reclaimed = step.reclaimed_bytes();
        if reclaimed != 0 {
            ui.label(
                egui::RichText::new(memory::format_signed_bytes(reclaimed))
                    .size(theme::CAPTION)
                    .color(p.text_secondary),
            );
        }
        ui.label(
            egui::RichText::new(format!("{:.2} s", step.elapsed_secs))
                .size(theme::CAPTION)
                .color(p.text_tertiary),
        );
    });
}

// ─── Quick Stats ─────────────────────────────────────────────────────────────

/// Commit charge, process count and thread count in one quiet row.
fn draw_quick_stats(ui: &mut egui::Ui, p: &Palette, snap: &MemorySnapshot) {
    let commit = snap.commit_total_pages.saturating_mul(snap.page_size);
    let limit = snap.commit_limit_pages.saturating_mul(snap.page_size);
    let stats = [
        (
            text::STAT_COMMIT,
            format!(
                "{:.0}% ({} of {})",
                snap.commit_percent(),
                memory::format_bytes(commit),
                memory::format_bytes(limit)
            ),
        ),
        (text::STAT_PROCESSES, group_digits(u64::from(snap.process_count))),
        (text::STAT_THREADS, group_digits(u64::from(snap.thread_count))),
    ];
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 36.0;
        for (label, value) in stats {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .size(theme::CAPTION)
                        .color(p.text_secondary),
                );
                ui.label(egui::RichText::new(value).color(p.text));
            });
        }
    });
}

/// `7258` as `7,258`.
fn group_digits(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_are_grouped_in_thousands() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(7258), "7,258");
        assert_eq!(group_digits(1_234_567), "1,234,567");
    }

    /// A 16 GiB snapshot with 6 GiB available and no page-list data.
    fn snapshot_without_lists() -> MemorySnapshot {
        const GIB: u64 = 1024 * 1024 * 1024;
        MemorySnapshot {
            memory_load_percent: 62,
            total_physical: 16 * GIB,
            available_physical: 6 * GIB,
            used_physical: 10 * GIB,
            total_page_file: 0,
            available_page_file: 0,
            total_virtual: 0,
            available_virtual: 0,
            commit_total_pages: 0,
            commit_limit_pages: 0,
            commit_peak_pages: 0,
            physical_available_pages: 0,
            physical_total_pages: 0,
            kernel_paged_pages: 0,
            kernel_nonpaged_pages: 0,
            page_size: 4096,
            handle_count: 0,
            process_count: 0,
            thread_count: 0,
            lists: None,
        }
    }

    #[test]
    fn map_falls_back_to_in_use_and_available_without_lists() {
        let snap = snapshot_without_lists();
        let parts = map_parts(&snap, None, &egui::Context::default());
        let names: Vec<&str> = parts.iter().map(|p| p.name).collect();
        assert_eq!(names, [text::LIST_IN_USE, text::LIST_AVAILABLE]);
        assert!((parts[0].fraction - 0.625).abs() < 0.001);
        assert_eq!(estimate_text(CleanLevel::Gentle, &snap), text::NO_ESTIMATE);
    }

    #[test]
    fn transition_eases_from_start_to_end() {
        assert!((ease_out_cubic(0.0)).abs() < f32::EPSILON);
        assert!((ease_out_cubic(1.0) - 1.0).abs() < f32::EPSILON);
        assert!(ease_out_cubic(0.5) > 0.5, "ease-out moves fast first");
    }
}
