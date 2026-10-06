//! The navigation pane and the page it switches between.
//!
//! Follows Windows 11 Task Manager: a labeled pane that collapses to an icon
//! rail. The menu button toggles it, and the choice is remembered. On windows
//! narrower than [`theme::PANE_AUTO_COLLAPSE_WIDTH`] the pane is always a
//! rail, and the menu button opens the labeled pane over the page instead.

use eframe::egui;

use super::app::{MagicXApp, Panel};
use super::icons::regular as ph;
use super::theme::{self, Palette};
use super::{panels, widgets};
use crate::strings;

/// Pages listed at the top of the pane: `(panel, icon, label)`.
const TOP_ITEMS: [(Panel, &str, &str); 4] = [
    (Panel::Overview, ph::GAUGE, strings::tray::NAV_OVERVIEW),
    (Panel::Monitor, ph::ACTIVITY, strings::tray::NAV_MONITOR),
    (Panel::Processes, ph::CPU, strings::tray::NAV_PROCESSES),
    (Panel::Settings, ph::GEAR, strings::tray::NAV_SETTINGS),
];

/// Height of one navigation item.
const ITEM_HEIGHT: f32 = 36.0;

/// Draw the navigation pane on the left of `ui`.
pub(super) fn draw_pane(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let p = theme::palette();
    let narrow = ui.ctx().content_rect().width() < theme::PANE_AUTO_COLLAPSE_WIDTH;
    if !narrow {
        app.nav_overlay_open = false;
    }
    let expanded = app.settings.nav_expanded && !narrow;

    let target = if expanded {
        theme::PANE_EXPANDED_WIDTH
    } else {
        theme::PANE_RAIL_WIDTH
    };
    let width = ui
        .ctx()
        .animate_value_with_time(egui::Id::new("nav-width"), target, 0.15);

    egui::Panel::left("nav")
        .resizable(false)
        .exact_size(width)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(p.bg).inner_margin(pane_margin()))
        .show(ui, |ui| {
            let show_labels = width > theme::PANE_RAIL_WIDTH + 60.0;
            if menu_button(ui, &p).clicked() {
                if narrow {
                    app.nav_overlay_open = !app.nav_overlay_open;
                } else {
                    app.settings.nav_expanded = !app.settings.nav_expanded;
                }
            }
            ui.add_space(4.0);
            draw_items(ui, app, &p, show_labels);
        });

    if narrow && app.nav_overlay_open {
        draw_overlay(ui.ctx(), app, &p);
    }
}

/// Margin inside the pane.
const fn pane_margin() -> egui::Margin {
    egui::Margin {
        left: 6,
        right: 6,
        top: 6,
        bottom: 8,
    }
}

/// The page items, with About pinned to the bottom.
fn draw_items(ui: &mut egui::Ui, app: &mut MagicXApp, p: &Palette, show_labels: bool) {
    for (panel, icon, label) in TOP_ITEMS {
        if nav_item(ui, p, icon, label, app.active_panel == panel, show_labels).clicked() {
            app.active_panel = panel;
            app.nav_overlay_open = false;
        }
        ui.add_space(2.0);
    }
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        let selected = app.active_panel == Panel::About;
        if nav_item(
            ui,
            p,
            ph::INFO,
            strings::gui::about::TITLE,
            selected,
            show_labels,
        )
        .clicked()
        {
            app.active_panel = Panel::About;
            app.nav_overlay_open = false;
        }
    });
}

/// The labeled pane drawn over the page on narrow windows.
fn draw_overlay(ctx: &egui::Context, app: &mut MagicXApp, p: &Palette) {
    let screen = ctx.content_rect();
    let area = egui::Area::new(egui::Id::new("nav-overlay"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(p.bg)
                .stroke(egui::Stroke::new(1.0_f32, p.card_stroke))
                .shadow(ui.visuals().window_shadow)
                .inner_margin(pane_margin())
                .show(ui, |ui| {
                    ui.set_width(theme::PANE_EXPANDED_WIDTH - 12.0);
                    ui.set_height(screen.height() - 14.0);
                    if menu_button(ui, p).clicked() {
                        app.nav_overlay_open = false;
                    }
                    ui.add_space(4.0);
                    draw_items(ui, app, p, true);
                });
        });

    // A click anywhere outside the overlay closes it.
    let clicked_outside = ctx.input(|i| {
        i.pointer.any_click()
            && i.pointer
                .interact_pos()
                .is_some_and(|pos| !area.response.rect.contains(pos))
    });
    if clicked_outside {
        app.nav_overlay_open = false;
    }
}

/// The menu button that expands and collapses the pane.
fn menu_button(ui: &mut egui::Ui, p: &Palette) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(theme::PANE_RAIL_WIDTH - 12.0, ITEM_HEIGHT),
        egui::Sense::click(),
    );
    let response = response.on_hover_text(strings::gui::NAV_TOGGLE);
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(theme::CONTROL_RADIUS),
            p.subtle,
        );
    }
    widgets::focus_ring(ui, &response, rect, p);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        ph::LIST,
        egui::FontId::proportional(18.0),
        p.text,
    );
    response
}

/// One navigation item: icon, and the label when the pane is expanded.
fn nav_item(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: &str,
    label: &str,
    selected: bool,
    show_label: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ITEM_HEIGHT),
        egui::Sense::click(),
    );
    let response = if show_label {
        response
    } else {
        response.on_hover_text(label)
    };

    let painter = ui.painter();
    if selected || response.hovered() {
        painter.rect_filled(
            rect,
            egui::CornerRadius::same(theme::CONTROL_RADIUS),
            p.subtle,
        );
    }
    if selected {
        // The selection mark: a short accent pill on the leading edge.
        let mark = egui::Rect::from_center_size(
            egui::pos2(rect.left() + 1.5, rect.center().y),
            egui::vec2(3.0, 16.0),
        );
        painter.rect_filled(mark, egui::CornerRadius::same(2), p.accent);
    }
    widgets::focus_ring(ui, &response, rect, p);

    let icon_center = egui::pos2(
        rect.left() + (theme::PANE_RAIL_WIDTH - 12.0) / 2.0,
        rect.center().y,
    );
    painter.text(
        icon_center,
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(18.0),
        p.text,
    );
    if show_label {
        let text_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 44.0, rect.top()),
            rect.right_bottom(),
        );
        let galley = painter.layout_job(widgets::single_line_job(
            label,
            theme::BODY,
            p.text,
            if selected { theme::SEMIBOLD } else { 400.0 },
            text_rect.width(),
        ));
        let pos = egui::pos2(text_rect.left(), rect.center().y - galley.size().y / 2.0);
        painter
            .with_clip_rect(text_rect)
            .galley(pos, galley, p.text);
    }
    response
}

/// Draw the active page in the remaining space.
pub(super) fn draw_page(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let p = theme::palette();
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(p.bg))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .content_margin(egui::Margin {
                    left: 28,
                    right: 28,
                    top: 18,
                    bottom: 28,
                })
                .show(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(theme::CONTENT_MAX_WIDTH));
                    match app.active_panel {
                        Panel::Overview => panels::overview::draw(ui, app),
                        Panel::Monitor => panels::monitor::draw(ui, app),
                        Panel::Processes => panels::processes::draw(ui, app),
                        Panel::Settings => panels::settings::draw(ui, app),
                        Panel::About => panels::about::draw(ui, app),
                    }
                });
        });
}
