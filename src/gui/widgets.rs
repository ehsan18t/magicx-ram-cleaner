//! # Shared GUI Widgets
//!
//! The building blocks every page uses, styled after Windows 11 controls:
//! cards, page titles, Settings-style rows, toggle switches, segmented
//! controls and buttons. All colours come from the active
//! [`Palette`](super::theme::Palette).

use eframe::egui;

use super::theme::{self, Palette};

// ─── Containers And Headings ─────────────────────────────────────────────────

/// A card: the elevated surface that groups related content.
pub fn card(ui: &mut egui::Ui, _dark: bool, add_contents: impl FnOnce(&mut egui::Ui)) {
    card_with_padding(ui, theme::CARD_PADDING, add_contents);
}

/// A card with custom inner padding (zero for edge-to-edge lists).
pub fn card_with_padding(ui: &mut egui::Ui, padding: i8, add_contents: impl FnOnce(&mut egui::Ui)) {
    let p = theme::palette();
    egui::Frame::new()
        .fill(p.card)
        .stroke(egui::Stroke::new(1.0_f32, p.card_stroke))
        .corner_radius(egui::CornerRadius::same(theme::CARD_RADIUS))
        .inner_margin(egui::Margin::same(padding))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add_contents(ui);
        });
}

/// The heading of a page.
pub fn page_title(ui: &mut egui::Ui, title: &str) {
    let p = theme::palette();
    ui.label(theme::display(title, theme::TITLE).color(p.text));
    ui.add_space(14.0);
}

/// A group heading above one or more cards, as in Windows Settings.
pub fn section_header(ui: &mut egui::Ui, title: &str) {
    let p = theme::palette();
    ui.label(theme::semibold(title, theme::BODY).color(p.text));
    ui.add_space(6.0);
}

/// A thin horizontal divider across the available width.
pub fn divider(ui: &mut egui::Ui) {
    let p = theme::palette();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::ZERO, p.divider);
}

/// Draw the keyboard focus ring around `rect` when `response` has focus.
pub fn focus_ring(ui: &egui::Ui, response: &egui::Response, rect: egui::Rect, p: &Palette) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(1.0),
            egui::CornerRadius::same(theme::CONTROL_RADIUS + 1),
            egui::Stroke::new(2.0_f32, p.text),
            egui::StrokeKind::Outside,
        );
    }
}

/// A single-line text layout at `size` and `weight`, elided to `max_width`.
#[must_use]
pub fn single_line_job(
    text: &str,
    size: f32,
    color: egui::Color32,
    weight: f32,
    max_width: f32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color,
            coords: egui::epaint::text::VariationCoords::new([(b"wght", weight)]),
            ..Default::default()
        },
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width);
    job
}

// ─── Settings Rows ───────────────────────────────────────────────────────────

/// A Windows Settings-style row: icon, title and description on the left,
/// a control on the right. Rows inside one card are separated by dividers
/// drawn by the caller with [`divider`].
pub fn settings_row(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    description: &str,
    add_control: impl FnOnce(&mut egui::Ui),
) {
    let p = theme::palette();
    ui.horizontal(|ui| {
        ui.set_min_height(48.0);
        ui.add_space(2.0);
        ui.label(egui::RichText::new(icon).size(20.0).color(p.text));
        ui.add_space(10.0);
        let title = egui::RichText::new(title).size(theme::BODY).color(p.text);
        if description.is_empty() {
            // A lone title centres on the row, level with the control.
            ui.label(title);
        } else {
            ui.vertical(|ui| {
                ui.add_space(2.0);
                ui.label(title);
                ui.label(
                    egui::RichText::new(description)
                        .size(theme::CAPTION)
                        .color(p.text_secondary),
                );
            });
        }
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            add_control,
        );
    });
}

// ─── Controls ────────────────────────────────────────────────────────────────

/// A Windows 11 toggle switch. Returns the response; `changed()` is set when
/// the user flips it.
pub fn toggle_switch(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let p = theme::palette();
    let (rect, mut response) = ui.allocate_exact_size(egui::vec2(40.0, 20.0), egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *on, ""));

    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_with_time(response.id, *on, 0.12);
        let hovered = response.hovered();
        let radius = egui::CornerRadius::same(10);
        let painter = ui.painter();
        if *on {
            let fill = if hovered { p.accent_hover } else { p.accent };
            painter.rect_filled(rect, radius, fill);
        } else {
            let fill = if hovered { p.control_hover } else { p.control };
            painter.rect_filled(rect, radius, fill);
            painter.rect_stroke(
                rect,
                radius,
                egui::Stroke::new(1.0_f32, p.text_secondary),
                egui::StrokeKind::Inside,
            );
        }
        let knob_r = if hovered { 7.0 } else { 6.0 };
        let x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), t);
        let knob = if *on { p.on_accent } else { p.text_secondary };
        painter.circle_filled(egui::pos2(x, rect.center().y), knob_r, knob);
        focus_ring(ui, &response, rect, &p);
    }
    response
}

/// A segmented control: one choice out of a few, shown side by side.
/// Returns the index the user clicked this frame, if any.
pub fn segmented(
    ui: &mut egui::Ui,
    options: &[&str],
    selected: usize,
    enabled: bool,
) -> Option<usize> {
    let p = theme::palette();
    let height = theme::CONTROL_HEIGHT;
    let (track, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(
        track,
        egui::CornerRadius::same(theme::CONTROL_RADIUS + 2),
        p.well,
    );

    let count = options.len().max(1) as f32;
    let width = (track.width() - 6.0) / count;
    let mut clicked = None;
    for (i, label) in options.iter().enumerate() {
        let rect = egui::Rect::from_min_size(
            egui::pos2(
                width.mul_add(i as f32, track.left() + 3.0),
                track.top() + 3.0,
            ),
            egui::vec2(width, height - 6.0),
        );
        let sense = if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let response = ui.interact(rect, ui.id().with(("segment", i)), sense);
        let is_selected = i == selected;
        if is_selected {
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(theme::CONTROL_RADIUS),
                p.card,
            );
            ui.painter().rect_stroke(
                rect,
                egui::CornerRadius::same(theme::CONTROL_RADIUS),
                egui::Stroke::new(1.0_f32, p.control_stroke),
                egui::StrokeKind::Inside,
            );
        } else if response.hovered() && enabled {
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(theme::CONTROL_RADIUS),
                p.control_hover.gamma_multiply(0.6),
            );
        }
        focus_ring(ui, &response, rect, &p);
        let color = if !enabled {
            p.text_disabled
        } else if is_selected {
            p.text
        } else {
            p.text_secondary
        };
        let weight = if is_selected { theme::SEMIBOLD } else { 400.0 };
        let galley = ui.painter().layout_job(single_line_job(
            label,
            theme::BODY,
            color,
            weight,
            rect.width() - 8.0,
        ));
        ui.painter()
            .galley(rect.center() - galley.size() / 2.0, galley, color);
        if response.clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// The accent-filled primary button. At most one per page.
pub fn primary_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    let p = theme::palette();
    let (fill, text) = if enabled {
        (p.accent, p.on_accent)
    } else {
        (p.control, p.text_tertiary)
    };
    let button = egui::Button::new(theme::semibold(label, theme::BODY).color(text))
        .fill(fill)
        .stroke(egui::Stroke::NONE)
        .corner_radius(egui::CornerRadius::same(theme::CONTROL_RADIUS))
        .min_size(egui::vec2(112.0, theme::CONTROL_HEIGHT));
    let response = ui.add_enabled(enabled, button);
    if enabled && response.hovered() {
        ui.painter().rect_filled(
            response.rect,
            egui::CornerRadius::same(theme::CONTROL_RADIUS),
            if p.dark {
                egui::Color32::from_black_alpha(28)
            } else {
                egui::Color32::from_white_alpha(36)
            },
        );
    }
    response
}

/// A standard (secondary) button.
pub fn secondary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let p = theme::palette();
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(theme::BODY).color(p.text))
            .corner_radius(egui::CornerRadius::same(theme::CONTROL_RADIUS))
            .min_size(egui::vec2(80.0, theme::CONTROL_HEIGHT)),
    )
}

/// A Windows 11 slider for whole numbers: a rail with the accent filling up
/// to the thumb, and the value shown on its left by `format`. Drag or click
/// the rail, or use the arrow keys when it has focus.
pub fn slider(
    ui: &mut egui::Ui,
    value: &mut u64,
    range: std::ops::RangeInclusive<u64>,
    step: u64,
    format: impl Fn(u64) -> String,
) -> egui::Response {
    let p = theme::palette();
    let (min, max) = (*range.start(), *range.end());
    let step = step.max(1);
    ui.horizontal(|ui| {
        let (rect, mut response) = ui.allocate_exact_size(
            egui::vec2(180.0, theme::CONTROL_HEIGHT),
            egui::Sense::click_and_drag(),
        );
        let rail = rect.shrink2(egui::vec2(10.0, 0.0));
        let span = (max - min).max(1) as f32;

        let press = response
            .interact_pointer_pos()
            .filter(|_| response.dragged() || response.clicked());
        let mut new = press.map_or(*value, |pos| {
            let t = ((pos.x - rail.left()) / rail.width()).clamp(0.0, 1.0);
            let steps = (t * span / step as f32).round() as u64;
            min + steps * step
        });
        if response.has_focus() {
            ui.input(|i| {
                if i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowUp) {
                    new = new.saturating_add(step);
                }
                if i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowDown) {
                    new = new.saturating_sub(step);
                }
            });
        }
        let new = new.clamp(min, max);
        if new != *value {
            *value = new;
            response.mark_changed();
        }

        let t = (*value - min) as f32 / span;
        let x = egui::lerp(rail.left()..=rail.right(), t);
        let track = egui::Rect::from_center_size(rail.center(), egui::vec2(rail.width(), 4.0));
        let painter = ui.painter();
        painter.rect_filled(track, egui::CornerRadius::same(2), p.control_stroke);
        let filled = egui::Rect::from_min_max(track.min, egui::pos2(x, track.max.y));
        painter.rect_filled(filled, egui::CornerRadius::same(2), p.accent);
        let center = egui::pos2(x, rail.center().y);
        painter.circle_filled(center, 10.0, p.control);
        painter.circle_stroke(center, 10.0, egui::Stroke::new(1.0_f32, p.control_stroke));
        let dot = if response.hovered() || response.dragged() {
            6.0
        } else {
            5.0
        };
        painter.circle_filled(center, dot, p.accent);
        focus_ring(ui, &response, rect, &p);

        // The value sits on the left, as in Windows Settings.
        ui.add_sized(
            egui::vec2(52.0, theme::CONTROL_HEIGHT),
            egui::Label::new(egui::RichText::new(format(*value)).color(p.text)),
        );
        response
    })
    .inner
}
