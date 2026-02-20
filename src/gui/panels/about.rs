//! # About Panel
//!
//! Professionally-designed full-page panel combining an app hero banner,
//! developer profile card with social links, and project / license details.
//! Layout follows clear visual hierarchy with generous whitespace and
//! purposeful use of the accent colour.

use eframe::egui;
use egui_phosphor::regular as ph;

use super::super::app::MagicXApp;
use super::super::{theme, widgets};

// --- Links -------------------------------------------------------------------

/// Source code repository URL.
const REPO_URL: &str = "https://github.com/ehsan18t/magicx-ram-cleaner";

/// Developer's GitHub profile URL.
const DEV_GITHUB: &str = "https://github.com/ehsan18t";

/// Developer's `LinkedIn` profile URL.
const DEV_LINKEDIN: &str = "https://linkedin.com/in/ehsan18t";

/// Developer's Telegram profile URL.
const DEV_TELEGRAM: &str = "https://t.me/ehsan18t";

/// Developer's personal website URL.
const DEV_WEBSITE: &str = "https://ehsankhan.me";

// --- Layout constants --------------------------------------------------------

/// Side length of the square app monogram badge in the hero section.
const HERO_BADGE_SIZE: f32 = 60.0;

/// Approximate height of the hero right-side content block
/// (title row + tagline + button/chip row + inter-line spacing).
/// Used to vertically centre the monogram badge within the hero card.
const HERO_CONTENT_HEIGHT: f32 = 86.0;

/// Diameter of the developer avatar circle — compact sidebar profile.
const AVATAR_SIZE: f32 = 56.0;

/// Diameter of a circular social-link icon button.
const ICON_BTN_SIZE: f32 = 42.0;

/// Approximate height of the developer bio block (name + handle + tag pills).
/// Used to vertically centre the bio within the social-grid row height.
const BIO_CONTENT_HEIGHT: f32 = 64.0;

/// GitHub brand background color -- very dark charcoal.
const COLOR_GITHUB: egui::Color32 = egui::Color32::from_rgb(36, 41, 47);

/// `LinkedIn` brand background color.
const COLOR_LINKEDIN: egui::Color32 = egui::Color32::from_rgb(0, 119, 181);

/// Telegram brand background color.
const COLOR_TELEGRAM: egui::Color32 = egui::Color32::from_rgb(38, 165, 228);

/// Website / personal link background color -- deep cyan.
const COLOR_WEBSITE: egui::Color32 = egui::Color32::from_rgb(14, 116, 144);

// --- Entry point -------------------------------------------------------------

/// Draw the about panel.
pub fn draw(ui: &mut egui::Ui, app: &MagicXApp) {
    let dark = app.settings.dark_mode;
    widgets::page_title(ui, ph::INFO, "About", dark);

    draw_hero(ui, dark);
    ui.add_space(theme::SECTION_SPACING);
    draw_developer(ui, dark);
    ui.add_space(theme::SECTION_SPACING);
    draw_project(ui, dark);
}

// --- Sections ----------------------------------------------------------------

/// Hero card: full-width card with a thin accent strip along the top edge,
/// monogram badge, app name, version chip, tagline, platform metadata chips,
/// and a `View on GitHub` CTA inline with the chips.
fn draw_hero(ui: &mut egui::Ui, dark: bool) {
    let bg = theme::surface_color(dark);
    let border = theme::border_color(dark);
    let r = theme::CARD_ROUNDING;

    let resp = egui::Frame::new()
        .fill(bg)
        .stroke(egui::Stroke::new(0.5, border))
        .corner_radius(egui::CornerRadius::same(r))
        .inner_margin(egui::Margin::same(theme::CARD_PADDING))
        .shadow(egui::Shadow {
            offset: [0, 2],
            blur: 10,
            spread: 0,
            color: egui::Color32::from_black_alpha(if dark { 50 } else { 15 }),
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            draw_hero_content(ui, dark);
        });

    // Accent strip flush with the card's top edge
    let card = resp.response.rect;
    let strip = egui::Rect::from_min_size(card.min, egui::vec2(card.width(), 3.0));
    ui.painter().rect_filled(
        strip,
        egui::CornerRadius {
            nw: r,
            ne: r,
            sw: 0,
            se: 0,
        },
        theme::ACCENT,
    );
}

/// Inner content row of the hero card: monogram badge, app name, version chip,
/// tagline, platform metadata chips, and the `View on GitHub` CTA button.
fn draw_hero_content(ui: &mut egui::Ui, dark: bool) {
    ui.horizontal(|ui| {
        // Monogram badge — vertically centred in the row
        ui.vertical(|ui| {
            let pad = (HERO_CONTENT_HEIGHT - HERO_BADGE_SIZE) / 2.0;
            ui.add_space(pad.max(0.0));

            let br = egui::CornerRadius::same(16_u8);
            let (badge_rect, _) = ui.allocate_exact_size(
                egui::vec2(HERO_BADGE_SIZE, HERO_BADGE_SIZE),
                egui::Sense::hover(),
            );
            ui.painter()
                .rect_filled(badge_rect, br, theme::ACCENT.gamma_multiply(0.18));
            ui.painter().rect_stroke(
                badge_rect,
                br,
                egui::Stroke::new(1.5, theme::ACCENT.gamma_multiply(0.55)),
                egui::StrokeKind::Outside,
            );
            ui.painter().text(
                badge_rect.center(),
                egui::Align2::CENTER_CENTER,
                "MGX",
                egui::FontId::proportional(16.0),
                theme::ACCENT,
            );
        });
        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("MagicX RAM Cleaner")
                        .strong()
                        .size(20.0)
                        .color(theme::text_color(dark)),
                );
                ui.add_space(6.0);
                version_chip(ui, dark);
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("The most powerful Windows RAM cleaner")
                    .size(12.0)
                    .color(theme::muted_color(dark)),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                view_on_github_btn(ui, dark);
                ui.add_space(4.0);
                ui.spacing_mut().item_spacing.x = 8.0;
                meta_chip(ui, ph::WINDOWS_LOGO, "Windows x86-64", dark);
                meta_chip(ui, ph::SCALES, "MIT License", dark);
            });
        });
    });
}

/// Developer profile card: avatar and bio text vertically centred on the left,
/// a thin accent rule, and a 2×2 grid of social icon buttons on the right.
/// Both sides share a uniform height derived from the social grid dimensions.
fn draw_developer(ui: &mut egui::Ui, dark: bool) {
    widgets::card(ui, dark, |ui| {
        widgets::section_header(ui, "Developer");

        let grid_side = ICON_BTN_SIZE.mul_add(2.0, 8.0); // 42×2 + 8 = 92 px
        let avatar_total = AVATAR_SIZE + 10.0; // inner circle + outer ring

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.set_min_height(grid_side);

            // Avatar — vertically centred in the row
            ui.vertical(|ui| {
                let pad = (grid_side - avatar_total) / 2.0;
                ui.add_space(pad.max(0.0));
                draw_dev_avatar(ui);
            });
            ui.add_space(14.0);

            // Right section: RTL places social grid at the far right,
            // then the divider, then the bio fills the rest.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;

                // Social grid — far right
                ui.allocate_ui_with_layout(
                    egui::vec2(grid_side, grid_side),
                    egui::Layout::top_down(egui::Align::Center),
                    draw_dev_socials_grid,
                );

                // Padding + vertical divider + padding
                ui.add_space(12.0);
                let (vdiv, _) =
                    ui.allocate_exact_size(egui::vec2(1.0, grid_side), egui::Sense::hover());
                ui.painter().rect_filled(
                    vdiv,
                    egui::CornerRadius::same(0),
                    theme::border_color(dark).gamma_multiply(0.50),
                );
                ui.add_space(12.0);

                // Bio — remaining width, vertically centred with manual pad
                let bio_w = ui.available_width().max(0.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(bio_w, grid_side),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let pad = (grid_side - BIO_CONTENT_HEIGHT) / 2.0;
                        ui.add_space(pad.max(0.0));
                        draw_dev_bio(ui, dark);
                    },
                );
            });
        });
    });
}

/// Draw the circular dual-ring monogram avatar for the developer profile.
fn draw_dev_avatar(ui: &mut egui::Ui) {
    let outer_r = AVATAR_SIZE / 2.0 + 5.0;
    let total_d = outer_r * 2.0;
    let (outer_rect, _) =
        ui.allocate_exact_size(egui::vec2(total_d, total_d), egui::Sense::hover());
    let center = outer_rect.center();
    ui.painter().circle_stroke(
        center,
        outer_r - 0.5,
        egui::Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.22)),
    );
    ui.painter().circle_filled(
        center,
        AVATAR_SIZE / 2.0,
        theme::ACCENT.gamma_multiply(0.18),
    );
    ui.painter().circle_stroke(
        center,
        AVATAR_SIZE / 2.0,
        egui::Stroke::new(2.0, theme::ACCENT.gamma_multiply(0.60)),
    );
    ui.painter().text(
        center,
        egui::Align2::CENTER_CENTER,
        "EK",
        egui::FontId::proportional(18.0),
        theme::ACCENT,
    );
}

/// Draw the developer name, `@`handle, and bio tag pills.
fn draw_dev_bio(ui: &mut egui::Ui, dark: bool) {
    ui.vertical(|ui| {
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("Ehsan Khan")
                .strong()
                .size(16.0)
                .color(theme::text_color(dark)),
        );
        ui.add_space(1.0);
        ui.label(
            egui::RichText::new("@ehsan18t")
                .size(12.0)
                .color(theme::ACCENT),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            tag_pill(ui, "Software Engineer", dark);
            tag_pill(ui, "Open Source Enthusiast", dark);
        });
    });
}

/// Draw the 2×2 grid of circular brand-coloured social icon buttons.
fn draw_dev_socials_grid(ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            social_icon_btn(ui, ph::GITHUB_LOGO, "GitHub", DEV_GITHUB, COLOR_GITHUB);
            social_icon_btn(
                ui,
                ph::LINKEDIN_LOGO,
                "LinkedIn",
                DEV_LINKEDIN,
                COLOR_LINKEDIN,
            );
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            social_icon_btn(
                ui,
                ph::TELEGRAM_LOGO,
                "Telegram",
                DEV_TELEGRAM,
                COLOR_TELEGRAM,
            );
            social_icon_btn(ui, ph::GLOBE, "Website", DEV_WEBSITE, COLOR_WEBSITE);
        });
    });
}

/// Project card: technology, platform, repository, license, and a styled
/// contribution call-to-action banner.
fn draw_project(ui: &mut egui::Ui, dark: bool) {
    widgets::card(ui, dark, |ui| {
        widgets::section_header(ui, "Project & License");

        info_row(ui, ph::CODE, "Technology", dark, |ui| {
            ui.label(
                egui::RichText::new("Rust")
                    .size(12.0)
                    .strong()
                    .color(theme::text_color(dark)),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("2024 Edition")
                    .size(12.0)
                    .color(theme::muted_color(dark)),
            );
        });

        row_divider(ui, dark);

        info_row(ui, ph::WINDOWS_LOGO, "Platform", dark, |ui| {
            ui.label(
                egui::RichText::new("Windows x86-64")
                    .size(12.0)
                    .strong()
                    .color(theme::text_color(dark)),
            );
        });

        row_divider(ui, dark);

        info_row(ui, ph::GITHUB_LOGO, "Repository", dark, |ui| {
            ui.hyperlink_to(
                egui::RichText::new("ehsan18t/magicx-ram-cleaner")
                    .size(12.0)
                    .color(theme::ACCENT),
                REPO_URL,
            );
        });

        row_divider(ui, dark);

        info_row(ui, ph::SCALES, "License", dark, |ui| {
            ui.label(
                egui::RichText::new("MIT")
                    .size(12.0)
                    .strong()
                    .color(theme::text_color(dark)),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("\u{00b7}  \u{00a9} 2026 MagicXMod")
                    .size(12.0)
                    .color(theme::muted_color(dark)),
            );
        });

        ui.add_space(12.0);

        draw_contrib_banner(ui, dark);
    });
}

/// Contribution call-to-action: accent-tinted banner with a heart icon,
/// heading, and a brief open-source blurb.
///
/// The heart icon is drawn via `allocate_exact_size` + `painter.text()` so
/// it is truly vertically centred within the text block regardless of how
/// many lines the description wraps to.
fn draw_contrib_banner(ui: &mut egui::Ui, dark: bool) {
    let bg = theme::ACCENT.gamma_multiply(if dark { 0.08 } else { 0.06 });
    let border = theme::ACCENT.gamma_multiply(0.20);

    egui::Frame::new()
        .fill(bg)
        .stroke(egui::Stroke::new(0.5, border))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            // Measure the text block height first so the icon rect can match it.
            // We render into a temporary vertical Ui that is laid out but not yet
            // painted — instead we compute the text region width, then draw both
            // icon and text in a single horizontal pass.
            //
            // Approach: draw the text block on the right with a known left
            // margin for the icon, then overlay the icon centred on the text
            // block's actual height.
            let icon_col_w = 30.0; // 20 px glyph + breathing room
            let gap = 10.0;
            let text_x_offset = icon_col_w + gap;

            // Draw the text block, offset to the right to leave room for the icon.
            let text_resp = ui.horizontal(|ui| {
                ui.add_space(text_x_offset);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Open Source")
                            .size(13.0)
                            .strong()
                            .color(theme::text_color(dark)),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(
                            "Free and open-source software. Contributions, \
                             bug reports, and feature requests are welcome!",
                        )
                        .size(11.0)
                        .color(theme::muted_color(dark)),
                    );
                })
            });

            // Paint the heart icon centred on the text block's actual height.
            let text_rect = text_resp.response.rect;
            let icon_center = egui::pos2(text_rect.left() + icon_col_w / 2.0, text_rect.center().y);
            ui.painter().text(
                icon_center,
                egui::Align2::CENTER_CENTER,
                ph::HEART,
                egui::FontId::proportional(20.0),
                theme::ACCENT,
            );
        });
}

// --- Helper Widgets ----------------------------------------------------------

/// Render the version string as a small rounded chip beside the app name.
fn version_chip(ui: &mut egui::Ui, dark: bool) {
    let version = concat!("v", env!("CARGO_PKG_VERSION"));
    let galley = ui.painter().layout_no_wrap(
        version.to_owned(),
        egui::FontId::proportional(11.0),
        theme::ACCENT,
    );
    let padding = egui::vec2(8.0, 3.0);
    let chip_size = galley.size() + padding * 2.0;
    let (rect, _) = ui.allocate_exact_size(chip_size, egui::Sense::hover());
    let bg = if dark {
        theme::ACCENT.gamma_multiply(0.15)
    } else {
        theme::ACCENT.gamma_multiply(0.12)
    };
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(6), bg);
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(6),
        egui::Stroke::new(0.5, theme::ACCENT.gamma_multiply(0.40)),
        egui::StrokeKind::Outside,
    );
    ui.painter()
        .galley(rect.min + padding, galley, theme::ACCENT);
}

/// Render a compact platform or metadata chip with an icon prefix.
fn meta_chip(ui: &mut egui::Ui, icon: &str, label: &str, dark: bool) {
    let text = format!("{icon}  {label}");
    let galley = ui.painter().layout_no_wrap(
        text,
        egui::FontId::proportional(11.0),
        egui::Color32::PLACEHOLDER,
    );
    let padding = egui::vec2(8.0, 4.0);
    let chip_size = galley.size() + padding * 2.0;
    let (rect, _) = ui.allocate_exact_size(chip_size, egui::Sense::hover());
    let bg = theme::border_color(dark).gamma_multiply(if dark { 0.70 } else { 0.60 });
    let text_color = theme::muted_color(dark);
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(6), bg);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{icon}  {label}"),
        egui::FontId::proportional(11.0),
        text_color,
    );
}

/// Render a fully-rounded tag pill for developer bio labels.
fn tag_pill(ui: &mut egui::Ui, label: &str, dark: bool) {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(11.0),
        egui::Color32::PLACEHOLDER,
    );
    let padding = egui::vec2(10.0, 4.0);
    let size = galley.size() + padding * 2.0;
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let bg = theme::ACCENT.gamma_multiply(if dark { 0.12 } else { 0.10 });
    let border = theme::ACCENT.gamma_multiply(0.28);
    let text_color = theme::ACCENT.gamma_multiply(0.90);
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(100), bg);
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(100),
        egui::Stroke::new(0.5, border),
        egui::StrokeKind::Outside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.0),
        text_color,
    );
}

/// Render a labelled info row: icon circle, label, then a value rendered by
/// the provided closure.
fn info_row(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    dark: bool,
    add_value: impl FnOnce(&mut egui::Ui),
) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        // Small icon circle
        let (icon_rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
        ui.painter().circle_filled(
            icon_rect.center(),
            11.0,
            theme::ACCENT.gamma_multiply(if dark { 0.14 } else { 0.11 }),
        );
        ui.painter().text(
            icon_rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            egui::FontId::proportional(11.0),
            theme::ACCENT.gamma_multiply(0.80),
        );

        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(format!("{label}:"))
                .size(12.0)
                .color(theme::muted_color(dark)),
        );
        ui.add_space(4.0);
        add_value(ui);
    });
    ui.add_space(4.0);
}

/// Draw a thin horizontal rule used between info rows or above CTAs.
fn row_divider(ui: &mut egui::Ui, dark: bool) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(0),
        theme::border_color(dark).gamma_multiply(0.50),
    );
}

/// Render an accent-styled ghost button that opens the repository in a new tab.
fn view_on_github_btn(ui: &mut egui::Ui, dark: bool) {
    let icon = ph::GITHUB_LOGO;
    let galley = ui.painter().layout_no_wrap(
        format!("{icon}  View on GitHub"),
        egui::FontId::proportional(12.0),
        egui::Color32::PLACEHOLDER,
    );
    let pad = egui::vec2(12.0, 4.0);
    let btn_size = galley.size() + pad * 2.0;
    let (rect, response) = ui.allocate_exact_size(btn_size, egui::Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Open repository in browser");

    let hovered = response.hovered();
    let br = egui::CornerRadius::same(8);
    let bg = theme::ACCENT.gamma_multiply(if hovered {
        if dark { 0.22 } else { 0.18 }
    } else if dark {
        0.12
    } else {
        0.09
    });
    let border = theme::ACCENT.gamma_multiply(if hovered { 0.80 } else { 0.35 });
    let text_color = if hovered {
        theme::ACCENT_HOVER
    } else {
        theme::ACCENT
    };

    ui.painter().rect_filled(rect, br, bg);
    ui.painter().rect_stroke(
        rect,
        br,
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Outside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{icon}  View on GitHub"),
        egui::FontId::proportional(12.0),
        text_color,
    );

    if response.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(REPO_URL));
    }
}

/// Render a circular brand-coloured icon button that opens `url` on click.
///
/// The platform `label` is shown as a tooltip on hover. The button is drawn as
/// a filled circle with the given `color`; brightness increases slightly on
/// hover and a faint white ring is added as a hover indicator.
fn social_icon_btn(ui: &mut egui::Ui, icon: &str, label: &str, url: &str, color: egui::Color32) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ICON_BTN_SIZE, ICON_BTN_SIZE),
        egui::Sense::click(),
    );
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label);

    let hovered = response.hovered();
    let bg = if hovered {
        color.gamma_multiply(1.25)
    } else {
        color
    };
    let radius = ICON_BTN_SIZE / 2.0;

    ui.painter().circle_filled(rect.center(), radius, bg);
    if hovered {
        ui.painter().circle_stroke(
            rect.center(),
            radius,
            egui::Stroke::new(2.0, egui::Color32::WHITE.gamma_multiply(0.40)),
        );
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(20.0),
        egui::Color32::WHITE,
    );

    if response.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}
