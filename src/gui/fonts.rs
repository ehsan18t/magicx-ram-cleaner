//! Font setup: Segoe UI, the Windows system font, for all text, with the
//! Phosphor icon subset as a fallback for icon glyphs.
//!
//! Segoe UI is read from the system fonts directory at startup and never
//! shipped with the app, which keeps the exe a single portable file. Windows
//! 11 has Segoe UI Variable (with weight and optical-size axes); Windows 10
//! has the static Segoe UI. If neither can be read, egui's built-in font is
//! used.

use eframe::egui;

/// Name the system font is registered under.
const SYSTEM_FONT: &str = "segoe-ui";

/// Candidate files, best first: the variable font, then the static one.
const SYSTEM_FONT_FILES: [&str; 2] = ["SegUIVar.ttf", "segoeui.ttf"];

/// Register the system font and the icon font with `ctx`.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    if let Some(data) = load_system_font() {
        fonts
            .font_data
            .insert(SYSTEM_FONT.to_owned(), std::sync::Arc::new(data));
        if let Some(proportional) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            proportional.insert(0, SYSTEM_FONT.to_owned());
        }
    }

    // Goes second in the proportional list, after the text font.
    super::icons::regular::add_to_fonts(&mut fonts);
    ctx.set_fonts(fonts);
}

/// Read the first system font file that exists.
fn load_system_font() -> Option<egui::FontData> {
    let dir = crate::platform::paths::fonts_directory().ok()?;
    SYSTEM_FONT_FILES
        .iter()
        .find_map(|file| std::fs::read(dir.join(file)).ok())
        .map(egui::FontData::from_owned)
}
