//! # GUI Theme — Modern Visual Design
//!
//! Centralised colour palette, spacing constants, and custom [`egui::Visuals`]
//! configuration for a polished, modern look. All panels reference this
//! module instead of hard-coding colours.

use eframe::egui;

// ─── Colour Palette ──────────────────────────────────────────────────────────

/// Primary accent colour (sky blue — modern, easy on the eyes).
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(56, 189, 248);

/// Lighter accent for hover states.
pub const ACCENT_HOVER: egui::Color32 = egui::Color32::from_rgb(96, 205, 252);

/// Dimmed accent for subtle borders and indicators.
pub const ACCENT_DIM: egui::Color32 = egui::Color32::from_rgb(30, 120, 170);

/// Success / positive / healthy colour.
pub const GREEN: egui::Color32 = egui::Color32::from_rgb(63, 185, 80);

/// Warning / caution colour.
pub const YELLOW: egui::Color32 = egui::Color32::from_rgb(210, 153, 34);

/// Error / danger colour.
pub const RED: egui::Color32 = egui::Color32::from_rgb(248, 81, 73);

// ─── Dark Theme Colours ──────────────────────────────────────────────────────

/// Card / elevated surface background (dark).
pub const SURFACE_DARK: egui::Color32 = egui::Color32::from_rgb(25, 31, 40);

/// Main content background (dark).
pub const BG_DARK: egui::Color32 = egui::Color32::from_rgb(13, 17, 23);

/// Sidebar background (dark).
pub const SIDEBAR_BG: egui::Color32 = egui::Color32::from_rgb(17, 21, 28);

/// Subtle border colour (dark).
pub const BORDER_DARK: egui::Color32 = egui::Color32::from_rgb(48, 54, 61);

/// Primary text (dark theme).
pub const TEXT_DARK: egui::Color32 = egui::Color32::from_rgb(230, 237, 243);

/// Muted / secondary text (dark theme).
pub const MUTED_DARK: egui::Color32 = egui::Color32::from_rgb(125, 133, 144);

// ─── Light Theme Colours ─────────────────────────────────────────────────────

/// Card / elevated surface background (light).
pub const SURFACE_LIGHT: egui::Color32 = egui::Color32::from_rgb(255, 255, 255);

/// Main content background (light).
pub const BG_LIGHT: egui::Color32 = egui::Color32::from_rgb(246, 248, 250);

/// Sidebar background (light).
pub const SIDEBAR_BG_LIGHT: egui::Color32 = egui::Color32::from_rgb(240, 242, 245);

/// Subtle border colour (light).
pub const BORDER_LIGHT: egui::Color32 = egui::Color32::from_rgb(216, 222, 228);

/// Primary text (light theme).
pub const TEXT_LIGHT: egui::Color32 = egui::Color32::from_rgb(31, 35, 40);

/// Muted / secondary text (light theme).
pub const MUTED_LIGHT: egui::Color32 = egui::Color32::from_rgb(101, 109, 118);

// ─── Clean Level Colours ─────────────────────────────────────────────────────

/// Gentle cleaning colour.
pub const LEVEL_GENTLE: egui::Color32 = egui::Color32::from_rgb(63, 185, 80);

/// Moderate cleaning colour.
pub const LEVEL_MODERATE: egui::Color32 = egui::Color32::from_rgb(210, 153, 34);

/// Aggressive cleaning colour.
pub const LEVEL_AGGRESSIVE: egui::Color32 = egui::Color32::from_rgb(218, 109, 40);

/// Nuclear cleaning colour.
pub const LEVEL_NUCLEAR: egui::Color32 = egui::Color32::from_rgb(248, 81, 73);

// ─── Spacing Constants ───────────────────────────────────────────────────────

/// Standard spacing between sections.
pub const SECTION_SPACING: f32 = 16.0;

/// Inner padding for cards.
pub const CARD_PADDING: i8 = 18;

/// Corner rounding for cards.
pub const CARD_ROUNDING: u8 = 12;

/// Sidebar width in logical points — narrow icon rail, frees content area.
pub const SIDEBAR_WIDTH: f32 = 56.0;

/// Sidebar button height — square tap target for centered icons.
pub const SIDEBAR_BUTTON_HEIGHT: f32 = 44.0;

// ─── Theme Application ───────────────────────────────────────────────────────

/// Apply the custom dark theme to the egui context.
pub fn apply_dark_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();

    v.panel_fill = BG_DARK;
    v.window_fill = SURFACE_DARK;
    v.faint_bg_color = SURFACE_DARK;
    v.extreme_bg_color = egui::Color32::from_rgb(10, 13, 18);
    v.code_bg_color = egui::Color32::from_rgb(30, 35, 42);

    v.selection.bg_fill = ACCENT.gamma_multiply(0.22);
    v.selection.stroke = egui::Stroke::new(1.0, ACCENT);

    let rounding = egui::CornerRadius::same(6);

    // Non-interactive
    v.widgets.noninteractive.bg_fill = SURFACE_DARK;
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT_DARK);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(0.5, BORDER_DARK);
    v.widgets.noninteractive.corner_radius = rounding;

    // Inactive (buttons, sliders, checkboxes at rest)
    v.widgets.inactive.bg_fill = egui::Color32::from_rgb(33, 38, 46);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(180, 186, 196));
    v.widgets.inactive.bg_stroke = egui::Stroke::new(0.5, BORDER_DARK);
    v.widgets.inactive.corner_radius = rounding;

    // Hovered
    v.widgets.hovered.bg_fill = egui::Color32::from_rgb(40, 46, 56);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    v.widgets.hovered.corner_radius = rounding;

    // Active / pressed
    v.widgets.active.bg_fill = ACCENT.gamma_multiply(0.20);
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    v.widgets.active.corner_radius = rounding;

    // Open (menu expanded etc.)
    v.widgets.open.bg_fill = egui::Color32::from_rgb(36, 42, 52);
    v.widgets.open.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    v.widgets.open.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    v.widgets.open.corner_radius = rounding;

    // Window chrome
    v.window_corner_radius = egui::CornerRadius::same(8);
    v.window_stroke = egui::Stroke::new(1.0, BORDER_DARK);
    v.window_shadow = egui::Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: egui::Color32::from_black_alpha(80),
    };

    v.resize_corner_size = 8.0;
    v.popup_shadow = egui::Shadow {
        offset: [0, 2],
        blur: 8,
        spread: 0,
        color: egui::Color32::from_black_alpha(60),
    };
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);

    ctx.set_visuals(v);
}

/// Apply the custom light theme to the egui context.
pub fn apply_light_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::light();

    v.panel_fill = BG_LIGHT;
    v.window_fill = SURFACE_LIGHT;
    v.faint_bg_color = egui::Color32::from_rgb(250, 251, 253);
    v.extreme_bg_color = egui::Color32::WHITE;
    v.code_bg_color = egui::Color32::from_rgb(234, 237, 242);

    v.selection.bg_fill = ACCENT.gamma_multiply(0.15);
    v.selection.stroke = egui::Stroke::new(1.0, ACCENT_DIM);

    let rounding = egui::CornerRadius::same(6);

    // Non-interactive
    v.widgets.noninteractive.bg_fill = SURFACE_LIGHT;
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT_LIGHT);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(0.5, BORDER_LIGHT);
    v.widgets.noninteractive.corner_radius = rounding;

    // Inactive (buttons, sliders at rest)
    v.widgets.inactive.bg_fill = egui::Color32::from_rgb(234, 237, 242);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(57, 62, 70));
    v.widgets.inactive.bg_stroke = egui::Stroke::new(0.5, BORDER_LIGHT);
    v.widgets.inactive.corner_radius = rounding;

    // Hovered
    v.widgets.hovered.bg_fill = egui::Color32::from_rgb(224, 228, 234);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT_LIGHT);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    v.widgets.hovered.corner_radius = rounding;

    // Active
    v.widgets.active.bg_fill = ACCENT.gamma_multiply(0.15);
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0, TEXT_LIGHT);
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    v.widgets.active.corner_radius = rounding;

    // Open
    v.widgets.open.bg_fill = egui::Color32::from_rgb(228, 232, 238);
    v.widgets.open.fg_stroke = egui::Stroke::new(1.0, TEXT_LIGHT);
    v.widgets.open.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    v.widgets.open.corner_radius = rounding;

    // Window chrome
    v.window_corner_radius = egui::CornerRadius::same(8);
    v.window_stroke = egui::Stroke::new(1.0, BORDER_LIGHT);
    v.window_shadow = egui::Shadow {
        offset: [0, 2],
        blur: 10,
        spread: 0,
        color: egui::Color32::from_black_alpha(20),
    };

    v.resize_corner_size = 8.0;
    v.popup_shadow = egui::Shadow {
        offset: [0, 2],
        blur: 8,
        spread: 0,
        color: egui::Color32::from_black_alpha(15),
    };
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);

    ctx.set_visuals(v);
}

// ─── Theme-aware Helpers ─────────────────────────────────────────────────────

/// Map a memory load fraction (0.0 – 1.0) to a colour (green → yellow → red).
///
/// Smooth gradient: green below 60 %, blending through yellow to red above 85 %.
#[must_use]
pub fn load_color(load: f32) -> egui::Color32 {
    if load > 0.85 {
        RED
    } else if load > 0.60 {
        let t = (load - 0.60) / 0.25;
        egui::Color32::from_rgb(
            lerp_u8(210, 248, t),
            lerp_u8(153, 81, t),
            lerp_u8(34, 73, t),
        )
    } else if load > 0.40 {
        let t = (load - 0.40) / 0.20;
        egui::Color32::from_rgb(
            lerp_u8(63, 210, t),
            lerp_u8(185, 153, t),
            lerp_u8(80, 34, t),
        )
    } else {
        GREEN
    }
}

/// Surface / card background for the current theme.
#[must_use]
pub const fn surface_color(dark: bool) -> egui::Color32 {
    if dark { SURFACE_DARK } else { SURFACE_LIGHT }
}

/// Border colour for the current theme.
#[must_use]
pub const fn border_color(dark: bool) -> egui::Color32 {
    if dark { BORDER_DARK } else { BORDER_LIGHT }
}

/// Muted / secondary text colour for the current theme.
#[must_use]
pub const fn muted_color(dark: bool) -> egui::Color32 {
    if dark { MUTED_DARK } else { MUTED_LIGHT }
}

/// Primary text colour for the current theme.
#[must_use]
pub const fn text_color(dark: bool) -> egui::Color32 {
    if dark { TEXT_DARK } else { TEXT_LIGHT }
}

/// Sidebar background for the current theme.
#[must_use]
pub const fn sidebar_bg(dark: bool) -> egui::Color32 {
    if dark { SIDEBAR_BG } else { SIDEBAR_BG_LIGHT }
}

/// Linear interpolation between two `u8` values.
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    let result = f32::from(a).mul_add(1.0 - t, f32::from(b) * t);
    // The result of lerping two u8 values is always within 0..=255.
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "lerp of two u8s is always in 0..=255"
    )]
    {
        result.round() as u8
    }
}
