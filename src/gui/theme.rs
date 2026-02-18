//! # GUI Theme — Modern Visual Design
//!
//! Centralised colour palette, spacing constants, and custom [`egui::Visuals`]
//! configuration for a polished, modern look. All panels reference this
//! module instead of hard-coding colours.

use eframe::egui;

// ─── Colour Palette ──────────────────────────────────────────────────────────

/// Primary accent colour (vibrant cyan).
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0, 180, 216);

/// Lighter accent for hover states.
pub const ACCENT_HOVER: egui::Color32 = egui::Color32::from_rgb(50, 210, 240);

/// Dimmed accent for subtle borders and indicators.
pub const ACCENT_DIM: egui::Color32 = egui::Color32::from_rgb(0, 110, 140);

/// Success / positive / healthy colour.
pub const GREEN: egui::Color32 = egui::Color32::from_rgb(72, 199, 142);

/// Warning / caution colour.
pub const YELLOW: egui::Color32 = egui::Color32::from_rgb(241, 196, 15);

/// Error / danger colour.
pub const RED: egui::Color32 = egui::Color32::from_rgb(231, 76, 60);

// ─── Dark Theme Colours ──────────────────────────────────────────────────────

/// Card / elevated surface background (dark).
pub const SURFACE_DARK: egui::Color32 = egui::Color32::from_rgb(28, 30, 38);

/// Main content background (dark).
pub const BG_DARK: egui::Color32 = egui::Color32::from_rgb(18, 18, 24);

/// Sidebar background (dark).
pub const SIDEBAR_BG: egui::Color32 = egui::Color32::from_rgb(22, 24, 32);

/// Subtle border colour (dark).
pub const BORDER_DARK: egui::Color32 = egui::Color32::from_rgb(46, 48, 60);

/// Primary text (dark theme).
pub const TEXT_DARK: egui::Color32 = egui::Color32::from_rgb(220, 222, 230);

/// Muted / secondary text (dark theme).
pub const MUTED_DARK: egui::Color32 = egui::Color32::from_rgb(130, 132, 150);

// ─── Light Theme Colours ─────────────────────────────────────────────────────

/// Card / elevated surface background (light).
pub const SURFACE_LIGHT: egui::Color32 = egui::Color32::from_rgb(255, 255, 255);

/// Main content background (light).
pub const BG_LIGHT: egui::Color32 = egui::Color32::from_rgb(242, 244, 248);

/// Sidebar background (light).
pub const SIDEBAR_BG_LIGHT: egui::Color32 = egui::Color32::from_rgb(232, 235, 242);

/// Subtle border colour (light).
pub const BORDER_LIGHT: egui::Color32 = egui::Color32::from_rgb(210, 214, 224);

/// Primary text (light theme).
pub const TEXT_LIGHT: egui::Color32 = egui::Color32::from_rgb(30, 32, 42);

/// Muted / secondary text (light theme).
pub const MUTED_LIGHT: egui::Color32 = egui::Color32::from_rgb(100, 105, 120);

// ─── Clean Level Colours ─────────────────────────────────────────────────────

/// Gentle cleaning colour.
pub const LEVEL_GENTLE: egui::Color32 = egui::Color32::from_rgb(72, 199, 142);

/// Moderate cleaning colour.
pub const LEVEL_MODERATE: egui::Color32 = egui::Color32::from_rgb(241, 196, 15);

/// Aggressive cleaning colour.
pub const LEVEL_AGGRESSIVE: egui::Color32 = egui::Color32::from_rgb(230, 126, 34);

/// Nuclear cleaning colour.
pub const LEVEL_NUCLEAR: egui::Color32 = egui::Color32::from_rgb(231, 76, 60);

// ─── Chart Colours ───────────────────────────────────────────────────────────

/// Used memory line colour in charts.
pub const CHART_USED: egui::Color32 = egui::Color32::from_rgb(231, 76, 60);

/// Available memory line colour in charts.
pub const CHART_AVAILABLE: egui::Color32 = egui::Color32::from_rgb(72, 199, 142);

// ─── Spacing Constants ───────────────────────────────────────────────────────

/// Standard spacing between sections.
pub const SECTION_SPACING: f32 = 10.0;

/// Inner padding for cards.
pub const CARD_PADDING: i8 = 10;

/// Corner rounding for cards.
pub const CARD_ROUNDING: u8 = 8;

/// Sidebar width in logical points.
pub const SIDEBAR_WIDTH: f32 = 148.0;

/// Sidebar button height.
pub const SIDEBAR_BUTTON_HEIGHT: f32 = 32.0;

// ─── Theme Application ───────────────────────────────────────────────────────

/// Apply the custom dark theme to the egui context.
pub fn apply_dark_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();

    v.panel_fill = BG_DARK;
    v.window_fill = SURFACE_DARK;
    v.faint_bg_color = SURFACE_DARK;
    v.extreme_bg_color = egui::Color32::from_rgb(14, 14, 18);
    v.code_bg_color = egui::Color32::from_rgb(32, 34, 44);

    v.selection.bg_fill = ACCENT.gamma_multiply(0.25);
    v.selection.stroke = egui::Stroke::new(1.0, ACCENT);

    let rounding = egui::CornerRadius::same(6);

    // Non-interactive
    v.widgets.noninteractive.bg_fill = SURFACE_DARK;
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT_DARK);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(0.5, BORDER_DARK);
    v.widgets.noninteractive.corner_radius = rounding;

    // Inactive
    v.widgets.inactive.bg_fill = egui::Color32::from_rgb(38, 40, 52);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(175, 178, 192));
    v.widgets.inactive.bg_stroke = egui::Stroke::new(0.5, BORDER_DARK);
    v.widgets.inactive.corner_radius = rounding;

    // Hovered
    v.widgets.hovered.bg_fill = egui::Color32::from_rgb(48, 52, 66);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    v.widgets.hovered.corner_radius = rounding;

    // Active / pressed
    v.widgets.active.bg_fill = ACCENT.gamma_multiply(0.22);
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    v.widgets.active.corner_radius = rounding;

    // Open (menu expanded etc.)
    v.widgets.open.bg_fill = egui::Color32::from_rgb(42, 44, 56);
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
    v.faint_bg_color = egui::Color32::from_rgb(248, 249, 252);
    v.extreme_bg_color = egui::Color32::WHITE;
    v.code_bg_color = egui::Color32::from_rgb(236, 238, 244);

    v.selection.bg_fill = ACCENT.gamma_multiply(0.18);
    v.selection.stroke = egui::Stroke::new(1.0, ACCENT);

    let rounding = egui::CornerRadius::same(6);

    // Non-interactive
    v.widgets.noninteractive.bg_fill = SURFACE_LIGHT;
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT_LIGHT);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(0.5, BORDER_LIGHT);
    v.widgets.noninteractive.corner_radius = rounding;

    // Inactive
    v.widgets.inactive.bg_fill = egui::Color32::from_rgb(228, 231, 238);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(55, 58, 68));
    v.widgets.inactive.bg_stroke = egui::Stroke::new(0.5, BORDER_LIGHT);
    v.widgets.inactive.corner_radius = rounding;

    // Hovered
    v.widgets.hovered.bg_fill = egui::Color32::from_rgb(218, 222, 232);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 30, 38));
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    v.widgets.hovered.corner_radius = rounding;

    // Active
    v.widgets.active.bg_fill = ACCENT.gamma_multiply(0.18);
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(18, 20, 28));
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    v.widgets.active.corner_radius = rounding;

    // Open
    v.widgets.open.bg_fill = egui::Color32::from_rgb(222, 226, 235);
    v.widgets.open.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 30, 38));
    v.widgets.open.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    v.widgets.open.corner_radius = rounding;

    // Window chrome
    v.window_corner_radius = egui::CornerRadius::same(8);
    v.window_stroke = egui::Stroke::new(1.0, BORDER_LIGHT);
    v.window_shadow = egui::Shadow {
        offset: [0, 3],
        blur: 12,
        spread: 0,
        color: egui::Color32::from_black_alpha(25),
    };

    v.resize_corner_size = 8.0;
    v.popup_shadow = egui::Shadow {
        offset: [0, 2],
        blur: 8,
        spread: 0,
        color: egui::Color32::from_black_alpha(20),
    };
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);

    ctx.set_visuals(v);
}

// ─── Theme-aware Helpers ─────────────────────────────────────────────────────

/// Map a memory load fraction (0.0 – 1.0) to a colour (green \u{2192} yellow \u{2192} red).
///
/// Smooth gradient: green below 60 %, blending through yellow to red above 85 %.
#[must_use]
pub fn load_color(load: f32) -> egui::Color32 {
    if load > 0.85 {
        RED
    } else if load > 0.60 {
        let t = (load - 0.60) / 0.25;
        egui::Color32::from_rgb(
            lerp_u8(241, 231, t),
            lerp_u8(196, 126, t),
            lerp_u8(15, 34, t),
        )
    } else if load > 0.40 {
        let t = (load - 0.40) / 0.20;
        egui::Color32::from_rgb(
            lerp_u8(72, 241, t),
            lerp_u8(199, 196, t),
            lerp_u8(142, 15, t),
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
