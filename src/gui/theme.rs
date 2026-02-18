//! # GUI Theme — Modern Visual Design
//!
//! Centralised colour palette, spacing constants, and custom [`egui::Visuals`]
//! configuration for a polished, modern look. All panels reference this
//! module instead of hard-coding colours.

use eframe::egui;

// ─── Colour Palette ──────────────────────────────────────────────────────────

/// Primary accent colour (cyan/teal).
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0, 180, 216);

/// Lighter accent for hover states.
pub const ACCENT_HOVER: egui::Color32 = egui::Color32::from_rgb(40, 210, 240);

/// Dimmed accent for subtle indicators.
pub const ACCENT_DIM: egui::Color32 = egui::Color32::from_rgb(0, 120, 150);

/// Success / positive colour.
pub const GREEN: egui::Color32 = egui::Color32::from_rgb(72, 199, 142);

/// Warning / caution colour.
pub const YELLOW: egui::Color32 = egui::Color32::from_rgb(241, 196, 15);

/// Error / danger colour.
pub const RED: egui::Color32 = egui::Color32::from_rgb(231, 76, 60);

/// Muted text colour (dark theme).
pub const MUTED: egui::Color32 = egui::Color32::from_rgb(140, 140, 160);

/// Card / elevated surface background (dark).
pub const SURFACE_DARK: egui::Color32 = egui::Color32::from_rgb(30, 32, 40);

/// Main background (dark).
pub const BG_DARK: egui::Color32 = egui::Color32::from_rgb(18, 18, 24);

/// Sidebar background (dark).
pub const SIDEBAR_BG: egui::Color32 = egui::Color32::from_rgb(22, 24, 32);

/// Subtle border colour (dark).
pub const BORDER_DARK: egui::Color32 = egui::Color32::from_rgb(50, 52, 65);

/// Card / elevated surface background (light).
pub const SURFACE_LIGHT: egui::Color32 = egui::Color32::from_rgb(255, 255, 255);

/// Main background (light).
pub const BG_LIGHT: egui::Color32 = egui::Color32::from_rgb(240, 242, 247);

/// Sidebar background (light).
pub const SIDEBAR_BG_LIGHT: egui::Color32 = egui::Color32::from_rgb(230, 233, 240);

/// Subtle border colour (light).
pub const BORDER_LIGHT: egui::Color32 = egui::Color32::from_rgb(210, 215, 225);

/// Muted text colour (light theme).
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
pub const SECTION_SPACING: f32 = 16.0;

/// Spacing inside cards.
pub const CARD_PADDING: i8 = 14;

/// Corner rounding for cards.
pub const CARD_ROUNDING: u8 = 10;

/// Sidebar width.
pub const SIDEBAR_WIDTH: f32 = 170.0;

/// Sidebar button height.
pub const SIDEBAR_BUTTON_HEIGHT: f32 = 38.0;

// ─── Theme Application ───────────────────────────────────────────────────────

/// Apply the custom dark theme to the egui context.
pub fn apply_dark_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();

    visuals.panel_fill = BG_DARK;
    visuals.window_fill = SURFACE_DARK;
    visuals.faint_bg_color = SURFACE_DARK;
    visuals.extreme_bg_color = egui::Color32::from_rgb(12, 12, 16);
    visuals.code_bg_color = egui::Color32::from_rgb(35, 38, 48);

    // Selection
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.30);
    visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);

    // Widgets
    visuals.widgets.noninteractive.bg_fill = SURFACE_DARK;
    visuals.widgets.noninteractive.fg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 200, 210));
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(0.5, BORDER_DARK);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(40, 42, 54);
    visuals.widgets.inactive.fg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(180, 180, 195));
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(0.5, BORDER_DARK);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(50, 55, 70);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.active.bg_fill = ACCENT.gamma_multiply(0.25);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.open.bg_fill = egui::Color32::from_rgb(45, 48, 60);
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(6);

    // Window
    visuals.window_corner_radius = egui::CornerRadius::same(10);
    visuals.window_stroke = egui::Stroke::new(1.0, BORDER_DARK);
    visuals.window_shadow = egui::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: egui::Color32::from_black_alpha(80),
    };

    // Misc
    visuals.resize_corner_size = 8.0;
    visuals.popup_shadow = egui::Shadow {
        offset: [0, 2],
        blur: 10,
        spread: 0,
        color: egui::Color32::from_black_alpha(60),
    };
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);

    ctx.set_visuals(visuals);
}

/// Apply the custom light theme to the egui context.
pub fn apply_light_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();

    visuals.panel_fill = BG_LIGHT;
    visuals.window_fill = SURFACE_LIGHT;
    visuals.faint_bg_color = egui::Color32::from_rgb(248, 249, 252);
    visuals.extreme_bg_color = egui::Color32::WHITE;
    visuals.code_bg_color = egui::Color32::from_rgb(235, 238, 245);

    // Selection
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.20);
    visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);

    // Widgets
    visuals.widgets.noninteractive.bg_fill = SURFACE_LIGHT;
    visuals.widgets.noninteractive.fg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 50, 60));
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(0.5, BORDER_LIGHT);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(230, 233, 240);
    visuals.widgets.inactive.fg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 72));
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(0.5, BORDER_LIGHT);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(220, 225, 235);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 30, 40));
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.active.bg_fill = ACCENT.gamma_multiply(0.20);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(20, 20, 30));
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.open.bg_fill = egui::Color32::from_rgb(225, 228, 238);
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 30, 40));
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(6);

    // Window
    visuals.window_corner_radius = egui::CornerRadius::same(10);
    visuals.window_stroke = egui::Stroke::new(1.0, BORDER_LIGHT);
    visuals.window_shadow = egui::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: egui::Color32::from_black_alpha(30),
    };

    // Misc
    visuals.resize_corner_size = 8.0;
    visuals.popup_shadow = egui::Shadow {
        offset: [0, 2],
        blur: 10,
        spread: 0,
        color: egui::Color32::from_black_alpha(25),
    };
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);

    ctx.set_visuals(visuals);
}

/// Map a memory load fraction (0.0–1.0) to a colour (green → yellow → red).
///
/// Smooth gradient: green below 60%, yellow around 60-85%, red above 85%.
#[must_use]
pub fn load_color(load: f32) -> egui::Color32 {
    if load > 0.85 {
        RED
    } else if load > 0.60 {
        // Blend from yellow to orange
        let t = (load - 0.60) / 0.25;
        egui::Color32::from_rgb(
            lerp_u8(241, 231, t),
            lerp_u8(196, 126, t),
            lerp_u8(15, 34, t),
        )
    } else if load > 0.40 {
        // Blend from green to yellow
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

/// Get the appropriate surface / card background for the current theme.
#[must_use]
pub const fn surface_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        SURFACE_DARK
    } else {
        SURFACE_LIGHT
    }
}

/// Get the appropriate border colour for the current theme.
#[must_use]
pub const fn border_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode { BORDER_DARK } else { BORDER_LIGHT }
}

/// Get the appropriate muted text colour for the current theme.
#[must_use]
pub const fn muted_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode { MUTED } else { MUTED_LIGHT }
}

/// Get the appropriate sidebar background for the current theme.
#[must_use]
pub const fn sidebar_bg(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        SIDEBAR_BG
    } else {
        SIDEBAR_BG_LIGHT
    }
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
