//! The user's Windows appearance choices: the app theme (light or dark) and
//! the accent colour palette, as set in Settings > Personalization.
//!
//! Both are read from the current user's registry, where Explorer keeps them
//! up to date the moment the user changes them.

use super::registry::{self, Hive};

/// Key holding the app theme choice.
const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

/// Key holding the accent colour palette.
const ACCENT_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent";

/// An RGB colour.
pub type Rgb = [u8; 3];

/// The seven shades Windows derives from the user's accent colour, lightest
/// first: light 3, light 2, light 1, the accent itself, dark 1, dark 2,
/// dark 3.
pub type AccentPalette = [Rgb; 7];

/// Whether Windows apps should use the light theme. `None` when the setting
/// cannot be read (it is missing on some older builds).
#[must_use]
pub fn apps_use_light_theme() -> Option<bool> {
    registry::read_u32(Hive::CurrentUser, PERSONALIZE_KEY, "AppsUseLightTheme").map(|v| v != 0)
}

/// The user's accent palette, or `None` when it cannot be read.
#[must_use]
pub fn accent_palette() -> Option<AccentPalette> {
    registry::read_bytes(Hive::CurrentUser, ACCENT_KEY, "AccentPalette")
        .and_then(|data| parse_accent_palette(&data))
}

/// Parse the `AccentPalette` value: eight RGBA entries (the eighth is unused).
fn parse_accent_palette(data: &[u8]) -> Option<AccentPalette> {
    if data.len() < 7 * 4 {
        return None;
    }
    let mut palette = [[0u8; 3]; 7];
    let (entries, _) = data.as_chunks::<4>();
    for (shade, entry) in palette.iter_mut().zip(entries) {
        *shade = [entry[0], entry[1], entry[2]];
    }
    Some(palette)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_palette_reads_rgb_and_skips_alpha() {
        let mut data = Vec::new();
        for i in 0..8u8 {
            data.extend_from_slice(&[i, i + 10, i + 20, 0xFF]);
        }
        let palette = parse_accent_palette(&data).expect("valid palette");
        assert_eq!(palette[0], [0, 10, 20]);
        assert_eq!(palette[6], [6, 16, 26]);
    }

    #[test]
    fn short_accent_palette_is_rejected() {
        assert!(parse_accent_palette(&[0; 12]).is_none());
    }
}
