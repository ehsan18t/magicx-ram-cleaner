//! # About Panel
//!
//! The app, its developer, and the project details, laid out like the About
//! page in Windows Settings: a header card with the app icon, then cards of
//! rows. Colour stays neutral here; only links use the accent.

use eframe::egui;

use crate::gui::icons::regular as ph;
use crate::strings::{self, gui::about as text};

use super::super::app::MagicXApp;
use super::super::theme::{self, Palette};
use super::super::widgets;

/// Side of the app icon in the header card.
const APP_ICON_SIZE: f32 = 56.0;

/// Diameter of the developer's initials avatar.
const AVATAR_SIZE: f32 = 48.0;

/// Side of a square social link button.
const LINK_BUTTON_SIZE: f32 = 36.0;

/// Draw the About panel.
pub fn draw(ui: &mut egui::Ui, app: &MagicXApp) {
    let p = theme::palette();
    widgets::page_title(ui, text::TITLE);

    widgets::card(ui, app.dark(), |ui| draw_header(ui, &p));

    ui.add_space(theme::SECTION_SPACING);
    widgets::section_header(ui, text::SECTION_DEVELOPER);
    widgets::card(ui, app.dark(), |ui| draw_developer(ui, &p));

    ui.add_space(theme::SECTION_SPACING);
    widgets::section_header(ui, text::SECTION_PROJECT);
    widgets::card(ui, app.dark(), |ui| draw_project(ui, &p));
}

/// App icon, name, version and tagline, with a link to the source.
fn draw_header(ui: &mut egui::Ui, p: &Palette) {
    ui.horizontal(|ui| {
        let icon = app_icon(ui.ctx());
        ui.add(egui::Image::new(&icon).fit_to_exact_size(egui::vec2(APP_ICON_SIZE, APP_ICON_SIZE)));
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.label(theme::semibold(strings::APP_NAME, theme::SUBTITLE).color(p.text));
            ui.label(
                egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                    .size(theme::CAPTION)
                    .color(p.text_secondary),
            );
            ui.label(egui::RichText::new(strings::APP_TAGLINE).color(p.text_secondary));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(format!(
                            "{}  {}",
                            ph::GITHUB_LOGO,
                            text::BTN_VIEW_GITHUB
                        ))
                        .color(p.text),
                    )
                    .corner_radius(egui::CornerRadius::same(theme::CONTROL_RADIUS))
                    .min_size(egui::vec2(0.0, theme::CONTROL_HEIGHT)),
                )
                .on_hover_text(strings::REPO_URL)
                .clicked()
            {
                open_link(strings::REPO_URL);
            }
        });
    });
}

/// The app icon as a texture, decoded once and kept in egui's memory.
fn app_icon(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::new("about-app-icon");
    if let Some(texture) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return texture;
    }
    let image = image::load_from_memory(include_bytes!("../../../assets/app.png")).map_or_else(
        |_| egui::ColorImage::new([1, 1], vec![egui::Color32::TRANSPARENT]),
        |img| {
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw())
        },
    );
    let texture = ctx.load_texture("app-icon", image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    texture
}

/// Avatar, name, handle and roles, with the developer's links.
fn draw_developer(ui: &mut egui::Ui, p: &Palette) {
    ui.horizontal(|ui| {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(AVATAR_SIZE, AVATAR_SIZE), egui::Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), AVATAR_SIZE / 2.0, p.subtle);
        ui.painter().circle_stroke(
            rect.center(),
            AVATAR_SIZE / 2.0 - 0.5,
            egui::Stroke::new(1.0_f32, p.control_stroke),
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            strings::developer::INITIALS,
            egui::FontId::proportional(17.0),
            p.text,
        );
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.label(theme::semibold(strings::developer::NAME, theme::BODY + 1.0).color(p.text));
            ui.label(
                egui::RichText::new(strings::developer::HANDLE)
                    .size(theme::CAPTION)
                    .color(p.text_secondary),
            );
            ui.label(
                egui::RichText::new(strings::developer::BIO_TAGS.join(", "))
                    .size(theme::CAPTION)
                    .color(p.text_secondary),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            // Right to left, so the last link is added first.
            for (icon, label, url) in [
                (
                    ph::GLOBE,
                    text::SOCIAL_WEBSITE,
                    strings::developer::WEBSITE_URL,
                ),
                (
                    ph::TELEGRAM_LOGO,
                    text::SOCIAL_TELEGRAM,
                    strings::developer::TELEGRAM_URL,
                ),
                (
                    ph::LINKEDIN_LOGO,
                    text::SOCIAL_LINKEDIN,
                    strings::developer::LINKEDIN_URL,
                ),
                (
                    ph::GITHUB_LOGO,
                    text::SOCIAL_GITHUB,
                    strings::developer::GITHUB_URL,
                ),
            ] {
                link_button(ui, p, icon, label, url);
            }
        });
    });
}

/// A square icon button that opens `url`.
fn link_button(ui: &mut egui::Ui, p: &Palette, icon: &str, label: &str, url: &str) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(LINK_BUTTON_SIZE, LINK_BUTTON_SIZE),
        egui::Sense::click(),
    );
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label);
    let fill = if response.hovered() {
        p.control_hover
    } else {
        p.control
    };
    let radius = egui::CornerRadius::same(theme::CONTROL_RADIUS);
    ui.painter().rect_filled(rect, radius, fill);
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(1.0_f32, p.control_stroke),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(18.0),
        p.text,
    );
    widgets::focus_ring(ui, &response, rect, p);
    if response.clicked() {
        open_link(url);
    }
}

/// Technology, platform, repository, license and the contribution note.
fn draw_project(ui: &mut egui::Ui, p: &Palette) {
    info_row(
        ui,
        p,
        ph::CODE,
        text::ROW_TECHNOLOGY,
        text::VALUE_TECHNOLOGY,
    );
    widgets::divider(ui);
    info_row(
        ui,
        p,
        ph::WINDOWS_LOGO,
        text::ROW_PLATFORM,
        text::VALUE_PLATFORM,
    );
    widgets::divider(ui);
    widgets::settings_row(ui, ph::GITHUB_LOGO, text::ROW_REPOSITORY, "", |ui| {
        let response = ui
            .link(egui::RichText::new(strings::REPO_SHORT).color(p.accent_text))
            .on_hover_text(strings::REPO_URL);
        if response.clicked() {
            open_link(strings::REPO_URL);
        }
    });
    widgets::divider(ui);
    info_row(
        ui,
        p,
        ph::SCALES,
        text::ROW_LICENSE,
        &format!("{}, {}", text::VALUE_LICENSE, strings::COPYRIGHT),
    );
    widgets::divider(ui);
    widgets::settings_row(
        ui,
        ph::HEART,
        text::ROW_OPEN_SOURCE,
        text::DESC_OPEN_SOURCE,
        |_| {},
    );
}

/// A row with a label on the left and a plain value on the right.
fn info_row(ui: &mut egui::Ui, p: &Palette, icon: &str, label: &str, value: &str) {
    widgets::settings_row(ui, icon, label, "", |ui| {
        ui.label(egui::RichText::new(value).color(p.text_secondary));
    });
}

/// Open `url` in the browser, without the app's administrator rights when
/// possible. A failed launch is ignored: the click simply does nothing.
fn open_link(url: &str) {
    crate::platform::shell::open_url(url).ok();
}
