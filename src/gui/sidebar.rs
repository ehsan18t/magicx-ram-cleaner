//! The navigation sidebar and the main panel it switches between.

use super::icons::regular as ph;
use eframe::egui;

use super::app::{MagicXApp, Panel};
use super::{panels, theme};
use crate::strings;

/// Navigation items: `(panel, icon, label)`.
///
/// Icons come from the app's Phosphor icon subset (`super::icons::regular`),
/// which is registered at startup in [`MagicXApp::new`].
const NAV_ITEMS: [(Panel, &str, &str); 4] = [
    (Panel::Dashboard, ph::GAUGE, strings::tray::NAV_DASHBOARD),
    (Panel::Monitor, ph::ACTIVITY, strings::tray::NAV_MONITOR),
    (Panel::Processes, ph::CPU, strings::tray::NAV_PROCESSES),
    (Panel::Settings, ph::GEAR, strings::tray::NAV_SETTINGS),
];

/// Draw the sidebar with navigation and branding.
pub(super) fn draw_sidebar(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;

    egui::Panel::left("sidebar")
        .resizable(false)
        .exact_size(theme::SIDEBAR_WIDTH)
        .frame(
            egui::Frame::new()
                .fill(theme::sidebar_bg(dark))
                .inner_margin(egui::Margin::symmetric(8, 10))
                .stroke(egui::Stroke::new(0.5_f32, theme::border_color(dark))),
        )
        .show(ui, |ui| {
            draw_sidebar_brand(ui);
            ui.add_space(8.0);
            draw_sidebar_nav(ui, app);
            // Pin the About button to the bottom of the sidebar.
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                let selected = app.active_panel == Panel::About;
                draw_nav_button(
                    ui,
                    ph::INFO,
                    strings::gui::about::TITLE,
                    selected,
                    dark,
                    || {
                        app.active_panel = Panel::About;
                    },
                );
            });
        });
}

/// Draw a compact `MX` monogram badge at the top of the sidebar.
///
/// Replaces the full word-mark to save horizontal space in the icon-rail layout.
fn draw_sidebar_brand(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        let badge_size = egui::vec2(36.0, 36.0);
        let (rect, _) = ui.allocate_exact_size(badge_size, egui::Sense::hover());
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(10),
            theme::ACCENT.gamma_multiply(0.18),
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            strings::MONOGRAM,
            egui::FontId::proportional(13.0),
            theme::ACCENT,
        );
    });
}

/// Draw the navigation buttons.
fn draw_sidebar_nav(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;
    for (panel, icon, label) in NAV_ITEMS {
        let selected = app.active_panel == panel;
        draw_nav_button(ui, icon, label, selected, dark, || {
            app.active_panel = panel;
        });
        ui.add_space(2.0);
    }
}

/// Draw a single icon-only navigation button.
///
/// The button fills the sidebar width and is square (height == [`theme::SIDEBAR_BUTTON_HEIGHT`]).
/// A pill-shaped background highlights the active or hovered state.
/// Hovering reveals a tooltip with the full panel name.
fn draw_nav_button(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    selected: bool,
    dark: bool,
    on_click: impl FnOnce(),
) {
    let desired_size = egui::vec2(ui.available_width(), theme::SIDEBAR_BUTTON_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
    let response = response.on_hover_text(label);

    if response.clicked() {
        on_click();
    }

    let hovered = response.hovered();
    let painter = ui.painter();

    // Rounded pill background for active / hovered state.
    let pill = rect.shrink(4.0);
    if selected {
        painter.rect_filled(
            pill,
            egui::CornerRadius::same(8),
            theme::ACCENT.gamma_multiply(0.20),
        );
    } else if hovered {
        painter.rect_filled(
            pill,
            egui::CornerRadius::same(8),
            theme::ACCENT.gamma_multiply(0.08),
        );
    }

    // Icon colour: accent when active, primary text on hover, muted otherwise.
    let icon_color = if selected {
        theme::ACCENT
    } else if hovered {
        theme::text_color(dark)
    } else {
        theme::muted_color(dark)
    };

    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(20.0),
        icon_color,
    );
}

/// Draw the main content area based on the active panel.
pub(super) fn draw_main_panel(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::bg_color(dark)))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .content_margin(egui::Margin::same(20))
                .show(ui, |ui| match app.active_panel {
                    Panel::Dashboard => panels::dashboard::draw(ui, app),
                    Panel::Monitor => panels::monitor::draw(ui, app),
                    Panel::Processes => panels::processes::draw(ui, app),
                    Panel::Settings => panels::settings::draw(ui, app),
                    Panel::About => panels::about::draw(ui, app),
                });
        });
}
