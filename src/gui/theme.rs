//! # GUI Theme
//!
//! The design tokens of the Windows 11 look (see `docs/UI_DESIGN.md`): a
//! [`Palette`] of neutral surfaces and text, the user's Windows accent colour,
//! and one fixed colour per memory list. Panels read the active palette with
//! [`palette`] instead of hard-coding colours.

use std::cell::Cell;

use eframe::egui;

use crate::memory::MemoryList;
use crate::platform::appearance::{AccentPalette, Rgb};

// ─── Type Scale ──────────────────────────────────────────────────────────────

/// Caption text: secondary labels and metadata.
pub const CAPTION: f32 = 12.0;

/// Body text: the default size for content and controls.
pub const BODY: f32 = 14.0;

/// Subtitle text: card and section headings.
pub const SUBTITLE: f32 = 20.0;

/// Title text: page headings.
pub const TITLE: f32 = 28.0;

/// Weight of strong text (Segoe UI Variable Semibold).
pub const SEMIBOLD: f32 = 600.0;

// ─── Shape And Spacing ───────────────────────────────────────────────────────

/// Corner radius of controls (buttons, inputs, list items).
pub const CONTROL_RADIUS: u8 = 4;

/// Corner radius of cards.
pub const CARD_RADIUS: u8 = 8;

/// Inner padding of cards.
pub const CARD_PADDING: i8 = 16;

/// Height of standard controls (buttons, inputs, navigation items).
pub const CONTROL_HEIGHT: f32 = 32.0;

/// Vertical space between cards and sections.
pub const SECTION_SPACING: f32 = 16.0;

/// Width of the navigation pane when expanded.
pub const PANE_EXPANDED_WIDTH: f32 = 180.0;

/// Width of the navigation pane when collapsed to an icon rail.
pub const PANE_RAIL_WIDTH: f32 = 52.0;

/// Window width below which the navigation pane collapses on its own.
pub const PANE_AUTO_COLLAPSE_WIDTH: f32 = 760.0;

/// Widest the page content grows, so lines stay readable on large windows.
pub const CONTENT_MAX_WIDTH: f32 = 1000.0;

// ─── Palette ─────────────────────────────────────────────────────────────────

/// Windows' default accent palette (blue), used when the user's accent
/// cannot be read. Lightest shade first, as in the registry.
pub const DEFAULT_ACCENT: AccentPalette = [
    [0x99, 0xEB, 0xFF],
    [0x4C, 0xC2, 0xFF],
    [0x00, 0x91, 0xF8],
    [0x00, 0x78, 0xD4],
    [0x00, 0x67, 0xC0],
    [0x00, 0x3E, 0x92],
    [0x00, 0x1A, 0x68],
];

/// Every colour the GUI uses, resolved for one theme and accent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Whether this is the dark theme.
    pub dark: bool,
    /// Window background, shared by the title bar and navigation pane.
    pub bg: egui::Color32,
    /// The page's surface: a layer raised above the background, so the
    /// page reads as separate from the navigation pane.
    pub layer: egui::Color32,
    /// Hairline edge of the page layer.
    pub layer_stroke: egui::Color32,
    /// Card surface.
    pub card: egui::Color32,
    /// Card border.
    pub card_stroke: egui::Color32,
    /// Divider lines inside cards and lists.
    pub divider: egui::Color32,
    /// Resting fill of buttons and inputs.
    pub control: egui::Color32,
    /// Hovered fill of buttons and inputs.
    pub control_hover: egui::Color32,
    /// Pressed fill of buttons and inputs.
    pub control_pressed: egui::Color32,
    /// Border of buttons and inputs.
    pub control_stroke: egui::Color32,
    /// Fill behind the selected navigation item.
    pub subtle: egui::Color32,
    /// Translucent wash behind a hovered item, on any surface. Lighter than
    /// a selection, so hover never looks like a choice.
    pub hover: egui::Color32,
    /// Translucent wash behind the selected item of a menu or list.
    pub selected: egui::Color32,
    /// The selected segment of a segmented control: raised above its well
    /// and clearly brighter than a hovered segment.
    pub raised: egui::Color32,
    /// Recessed fill: the track of segmented controls and empty bars.
    pub well: egui::Color32,
    /// Primary text.
    pub text: egui::Color32,
    /// Supporting text.
    pub text_secondary: egui::Color32,
    /// Captions and placeholders.
    pub text_tertiary: egui::Color32,
    /// Text of disabled controls.
    pub text_disabled: egui::Color32,
    /// Accent fill: primary buttons, switches that are on, the selection mark.
    pub accent: egui::Color32,
    /// Hovered accent fill.
    pub accent_hover: egui::Color32,
    /// Text and icons on an accent fill.
    pub on_accent: egui::Color32,
    /// Accent-coloured text, such as links.
    pub accent_text: egui::Color32,
    /// Success messages.
    pub success: egui::Color32,
    /// Warnings.
    pub caution: egui::Color32,
    /// Errors and real memory pressure; nothing else is red.
    pub critical: egui::Color32,
    /// Memory list: In use.
    pub in_use: egui::Color32,
    /// Memory list: Modified.
    pub modified: egui::Color32,
    /// Memory list: Standby.
    pub standby: egui::Color32,
    /// Memory list: Free.
    pub free: egui::Color32,
}

/// Build a colour from an RGB triple.
const fn rgb(c: Rgb) -> egui::Color32 {
    egui::Color32::from_rgb(c[0], c[1], c[2])
}

/// Shorthand for an opaque colour.
const fn hex(r: u8, g: u8, b: u8) -> egui::Color32 {
    egui::Color32::from_rgb(r, g, b)
}

impl Palette {
    /// The palette for the dark or light theme with the given accent.
    #[must_use]
    pub fn new(dark: bool, accent: &AccentPalette) -> Self {
        if dark {
            let bg = hex(0x20, 0x20, 0x20);
            let accent_fill = rgb(accent[1]);
            let card = hex(0x2D, 0x2D, 0x2D);
            let text = hex(0xFF, 0xFF, 0xFF);
            Self {
                dark,
                bg,
                layer: hex(0x27, 0x27, 0x27),
                layer_stroke: hex(0x1A, 0x1A, 0x1A),
                card,
                card_stroke: hex(0x1C, 0x1C, 0x1C),
                divider: hex(0x38, 0x38, 0x38),
                control: hex(0x2D, 0x2D, 0x2D),
                control_hover: hex(0x32, 0x32, 0x32),
                control_pressed: hex(0x27, 0x27, 0x27),
                control_stroke: hex(0x3A, 0x3A, 0x3A),
                subtle: hex(0x33, 0x33, 0x33),
                hover: egui::Color32::from_white_alpha(8),
                selected: egui::Color32::from_white_alpha(20),
                raised: hex(0x3A, 0x3A, 0x3A),
                well: hex(0x1E, 0x1E, 0x1E),
                text,
                text_secondary: hex(0xC8, 0xC8, 0xC8),
                text_tertiary: hex(0x9E, 0x9E, 0x9E),
                text_disabled: hex(0x6E, 0x6E, 0x6E),
                accent: accent_fill,
                accent_hover: mix(accent_fill, bg, 0.12),
                on_accent: text_on(accent_fill),
                accent_text: readable(
                    &[rgb(accent[0]), rgb(accent[1]), rgb(accent[2])],
                    card,
                    text,
                ),
                success: hex(0x6C, 0xCB, 0x5F),
                caution: hex(0xFC, 0xE1, 0x00),
                critical: hex(0xFF, 0x99, 0xA4),
                in_use: hex(0x93, 0x89, 0xFF),
                modified: hex(0xF2, 0xB0, 0x4C),
                standby: hex(0x3C, 0xC5, 0xB2),
                free: hex(0x4A, 0x4A, 0x4A),
            }
        } else {
            let bg = hex(0xF3, 0xF3, 0xF3);
            let accent_fill = rgb(accent[4]);
            let card = hex(0xFE, 0xFE, 0xFE);
            let text = hex(0x1B, 0x1B, 0x1B);
            Self {
                dark,
                bg,
                layer: hex(0xF9, 0xF9, 0xF9),
                layer_stroke: hex(0xE5, 0xE5, 0xE5),
                card,
                card_stroke: hex(0xE5, 0xE5, 0xE5),
                divider: hex(0xEA, 0xEA, 0xEA),
                control: hex(0xFD, 0xFD, 0xFD),
                control_hover: hex(0xF6, 0xF6, 0xF6),
                control_pressed: hex(0xF0, 0xF0, 0xF0),
                control_stroke: hex(0xDC, 0xDC, 0xDC),
                subtle: hex(0xE5, 0xE5, 0xE5),
                hover: egui::Color32::from_black_alpha(6),
                selected: egui::Color32::from_black_alpha(16),
                raised: hex(0xFF, 0xFF, 0xFF),
                well: hex(0xE6, 0xE6, 0xE6),
                text,
                text_secondary: hex(0x5D, 0x5D, 0x5D),
                text_tertiary: hex(0x66, 0x66, 0x66),
                text_disabled: hex(0xA0, 0xA0, 0xA0),
                accent: accent_fill,
                accent_hover: mix(accent_fill, bg, 0.12),
                on_accent: text_on(accent_fill),
                accent_text: readable(
                    &[rgb(accent[5]), rgb(accent[6]), rgb(accent[4])],
                    card,
                    text,
                ),
                success: hex(0x0F, 0x7B, 0x0F),
                caution: hex(0x9D, 0x5D, 0x00),
                critical: hex(0xC4, 0x2B, 0x1C),
                in_use: hex(0x5B, 0x4F, 0xE0),
                modified: hex(0xB7, 0x70, 0x0A),
                standby: hex(0x0E, 0x8A, 0x7B),
                free: hex(0xD2, 0xD2, 0xD2),
            }
        }
    }

    /// The fixed colour of a memory list.
    #[must_use]
    pub const fn list(&self, list: MemoryList) -> egui::Color32 {
        match list {
            MemoryList::InUse => self.in_use,
            MemoryList::Modified => self.modified,
            MemoryList::Standby => self.standby,
            MemoryList::Free => self.free,
        }
    }

    /// The background as RGB, for the window's title bar.
    #[must_use]
    pub const fn bg_rgb(&self) -> Rgb {
        [self.bg.r(), self.bg.g(), self.bg.b()]
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::new(true, &DEFAULT_ACCENT)
    }
}

thread_local! {
    /// The palette of the current frame. The GUI runs on one thread, so the
    /// active palette lives here like egui's own style does in its context.
    static ACTIVE: Cell<Palette> = Cell::new(Palette::default());
}

/// The active palette.
#[must_use]
pub fn palette() -> Palette {
    ACTIVE.with(Cell::get)
}

/// Make `p` the active palette and restyle egui's built-in widgets with it.
pub fn apply(ctx: &egui::Context, p: Palette) {
    ACTIVE.with(|cell| cell.set(p));
    let theme = if p.dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_visuals_of(theme, build_visuals(&p));
    ctx.set_theme(theme);
}

/// Configure egui's text styles and spacing for both themes.
pub fn configure_style(ctx: &egui::Context) {
    use egui::{FontFamily, FontId, TextStyle};

    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |style| {
            style.text_styles = [
                (
                    TextStyle::Small,
                    FontId::new(CAPTION, FontFamily::Proportional),
                ),
                (TextStyle::Body, FontId::new(BODY, FontFamily::Proportional)),
                (
                    TextStyle::Button,
                    FontId::new(BODY, FontFamily::Proportional),
                ),
                (
                    TextStyle::Heading,
                    FontId::new(TITLE, FontFamily::Proportional),
                ),
                (
                    TextStyle::Monospace,
                    FontId::new(CAPTION, FontFamily::Monospace),
                ),
            ]
            .into();
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(12.0, 5.0);
            style.spacing.interact_size = egui::vec2(32.0, CONTROL_HEIGHT);
            style.spacing.window_margin = egui::Margin::same(12);
            style.spacing.menu_margin = egui::Margin::same(6);
            style.spacing.slider_rail_height = 4.0;
            style.spacing.combo_height = 300.0;
            style.animation_time = 0.15;
        });
    }
}

/// Build egui [`egui::Visuals`] from a palette.
fn build_visuals(p: &Palette) -> egui::Visuals {
    let mut v = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.faint_bg_color = p.card;
    v.extreme_bg_color = p.control;
    v.text_edit_bg_color = Some(p.control);
    v.code_bg_color = p.control;
    v.hyperlink_color = p.accent_text;
    v.warn_fg_color = p.caution;
    v.error_fg_color = p.critical;
    v.override_text_color = None;

    v.selection.bg_fill = p.accent.gamma_multiply(0.45);
    v.selection.stroke = egui::Stroke::new(1.0_f32, p.accent);
    v.slider_trailing_fill = true;

    let radius = egui::CornerRadius::same(CONTROL_RADIUS);
    let widget = |fill, stroke, text| egui::style::WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: egui::Stroke::new(1.0_f32, stroke),
        corner_radius: radius,
        fg_stroke: egui::Stroke::new(1.0_f32, text),
        expansion: 0.0,
    };
    v.widgets.noninteractive = widget(p.card, p.divider, p.text);
    v.widgets.inactive = widget(p.control, p.control_stroke, p.text);
    v.widgets.hovered = widget(p.control_hover, p.control_stroke, p.text);
    v.widgets.active = widget(p.control_pressed, p.accent, p.text);
    v.widgets.open = widget(p.control_hover, p.control_stroke, p.text);

    v.window_corner_radius = egui::CornerRadius::same(CARD_RADIUS);
    v.menu_corner_radius = egui::CornerRadius::same(CARD_RADIUS);
    v.window_stroke = egui::Stroke::new(1.0_f32, p.card_stroke);
    v.window_shadow = egui::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: egui::Color32::from_black_alpha(if p.dark { 110 } else { 40 }),
    };
    v.popup_shadow = egui::Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: egui::Color32::from_black_alpha(if p.dark { 90 } else { 30 }),
    };
    v.resize_corner_size = 8.0;
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);
    v
}

// ─── Text Helpers ────────────────────────────────────────────────────────────

/// Text at `size` in the given weight (300 to 700).
#[must_use]
pub fn weighted(text: impl Into<String>, size: f32, weight: f32) -> egui::RichText {
    egui::RichText::new(text)
        .size(size)
        .variation(b"wght", weight)
}

/// Semibold text at `size`.
#[must_use]
pub fn semibold(text: impl Into<String>, size: f32) -> egui::RichText {
    weighted(text, size, SEMIBOLD)
}

/// A display heading: semibold, with the optical size matched to `size` so
/// large text uses Segoe UI Variable's Display design.
#[must_use]
pub fn display(text: impl Into<String>, size: f32) -> egui::RichText {
    semibold(text, size).variation(b"opsz", (size * 0.75).clamp(8.0, 36.0))
}

// ─── Colour Helpers ──────────────────────────────────────────────────────────

/// Blend `a` towards `b` by `t` (0.0 = `a`, 1.0 = `b`).
#[must_use]
pub fn mix(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    egui::Color32::from_rgb(
        lerp_u8(a.r(), b.r(), t),
        lerp_u8(a.g(), b.g(), t),
        lerp_u8(a.b(), b.b(), t),
    )
}

/// WCAG relative luminance of a colour.
fn luminance(c: egui::Color32) -> f32 {
    let channel = |v: u8| {
        let s = f32::from(v) / 255.0;
        if s <= 0.039_28 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.0722f32.mul_add(
        channel(c.b()),
        0.2126f32.mul_add(channel(c.r()), 0.7152 * channel(c.g())),
    )
}

/// WCAG contrast ratio between two colours (1 to 21).
#[must_use]
pub fn contrast(a: egui::Color32, b: egui::Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Black or white, whichever reads better on `fill`. The better of the two
/// always reaches at least 4.58:1, so any accent gets AA-level text.
fn text_on(fill: egui::Color32) -> egui::Color32 {
    let (black, white) = (egui::Color32::BLACK, egui::Color32::WHITE);
    if contrast(black, fill) >= contrast(white, fill) {
        black
    } else {
        white
    }
}

/// The first of `candidates` with AA contrast (4.5:1) on `surface`, or
/// `fallback` when none has it.
fn readable(
    candidates: &[egui::Color32],
    surface: egui::Color32,
    fallback: egui::Color32,
) -> egui::Color32 {
    candidates
        .iter()
        .copied()
        .find(|c| contrast(*c, surface) >= 4.5)
        .unwrap_or(fallback)
}

/// Linear interpolation between two `u8` values.
pub(crate) fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    let result = f32::from(a).mul_add(1.0 - t, f32::from(b) * t);
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "lerp of two u8s is always in 0..=255"
    )]
    {
        result.round() as u8
    }
}

// ─── Bridge For Panels Not Yet Rebuilt ───────────────────────────────────────
//
// Panels still written against the old colour functions read the active
// palette through these. Each one goes away once no panel uses it.

/// Primary text colour.
#[must_use]
pub fn text_color(_dark: bool) -> egui::Color32 {
    palette().text
}

/// Supporting text colour.
#[must_use]
pub fn muted_color(_dark: bool) -> egui::Color32 {
    palette().text_secondary
}

/// Card surface colour.
#[must_use]
pub fn surface_color(_dark: bool) -> egui::Color32 {
    palette().card
}

/// Divider colour.
#[must_use]
pub fn border_color(_dark: bool) -> egui::Color32 {
    palette().divider
}

/// Accent-coloured text.
#[must_use]
pub fn accent() -> egui::Color32 {
    palette().accent_text
}

/// Success colour.
#[must_use]
pub fn green() -> egui::Color32 {
    palette().success
}

/// Error colour.
#[must_use]
pub fn red() -> egui::Color32 {
    palette().critical
}

/// Warning colour.
#[must_use]
pub fn yellow() -> egui::Color32 {
    palette().caution
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_meets_wcag_aa_on_every_surface() {
        for dark in [true, false] {
            let p = Palette::new(dark, &DEFAULT_ACCENT);
            for surface in [p.bg, p.layer, p.card, p.control, p.subtle, p.well] {
                for text in [p.text, p.text_secondary, p.text_tertiary] {
                    assert!(
                        contrast(text, surface) >= 4.5,
                        "dark={dark}: {text:?} on {surface:?} is {:.2}",
                        contrast(text, surface)
                    );
                }
            }
            for colour in [p.critical, p.success] {
                assert!(contrast(colour, p.card) >= 4.5, "dark={dark}: {colour:?}");
            }
        }
    }

    /// An accent palette shaded from `base` the way Windows does: lighter
    /// shades mixed with white, darker ones with black.
    fn shades(base: egui::Color32) -> AccentPalette {
        let to_rgb = |c: egui::Color32| [c.r(), c.g(), c.b()];
        let (white, black) = (egui::Color32::WHITE, egui::Color32::BLACK);
        [
            to_rgb(mix(base, white, 0.7)),
            to_rgb(mix(base, white, 0.5)),
            to_rgb(mix(base, white, 0.25)),
            to_rgb(base),
            to_rgb(mix(base, black, 0.2)),
            to_rgb(mix(base, black, 0.45)),
            to_rgb(mix(base, black, 0.7)),
        ]
    }

    #[test]
    fn accent_text_and_text_on_accent_stay_readable_for_any_accent() {
        let bases = [
            hex(0xFF, 0xB9, 0x00), // yellow
            hex(0x10, 0x7C, 0x10), // green
            hex(0xE8, 0x11, 0x23), // red
            hex(0xA9, 0x4D, 0xC1), // purple
            hex(0x00, 0xB7, 0xC3), // teal
            hex(0x7A, 0x75, 0x74), // grey
            hex(0x00, 0x78, 0xD4), // Windows blue
        ];
        for base in bases {
            for dark in [true, false] {
                let p = Palette::new(dark, &shades(base));
                assert!(
                    contrast(p.on_accent, p.accent) >= 4.5,
                    "dark={dark}, accent {base:?}: text on accent"
                );
                for surface in [p.card, p.bg] {
                    assert!(
                        contrast(p.accent_text, surface) >= 4.5,
                        "dark={dark}, accent {base:?}: accent text on {surface:?}"
                    );
                }
            }
        }
    }

    /// `wash` painted over `surface`, as the renderer blends it.
    fn over(wash: egui::Color32, surface: egui::Color32) -> egui::Color32 {
        let keep = 1.0 - f32::from(wash.a()) / 255.0;
        let channel = |w: u8, s: u8| f32::from(s).mul_add(keep, f32::from(w)).round() as u8;
        hex(
            channel(wash.r(), surface.r()),
            channel(wash.g(), surface.g()),
            channel(wash.b(), surface.b()),
        )
    }

    #[test]
    fn hover_is_visible_everywhere_and_weaker_than_selection() {
        for dark in [true, false] {
            let p = Palette::new(dark, &DEFAULT_ACCENT);
            assert!(p.hover.a() < p.selected.a(), "dark={dark}");
            for surface in [p.bg, p.layer, p.card, p.well] {
                let hovered = over(p.hover, surface);
                let selected = over(p.selected, surface);
                assert!(
                    hovered.r().abs_diff(surface.r()) >= 3,
                    "dark={dark}: hover invisible on {surface:?}"
                );
                assert!(
                    selected.r().abs_diff(hovered.r()) >= 3,
                    "dark={dark}: selection looks like hover on {surface:?}"
                );
            }
            // The selected navigation item must not look like a hovered one.
            assert!(
                over(p.hover, p.bg).r().abs_diff(p.subtle.r()) >= 8,
                "dark={dark}: selected nav item looks hovered"
            );
            // Nor the selected segment like a hovered segment.
            assert!(
                over(p.hover, p.well).r().abs_diff(p.raised.r()) >= 8,
                "dark={dark}: selected segment looks hovered"
            );
        }
    }

    #[test]
    fn memory_list_colours_stand_out_from_the_card() {
        for dark in [true, false] {
            let p = Palette::new(dark, &DEFAULT_ACCENT);
            for list in [p.in_use, p.modified, p.standby] {
                assert!(contrast(list, p.card) >= 3.0, "dark={dark}: {list:?}");
            }
        }
    }
}
