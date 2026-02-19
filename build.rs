//! Build script — embeds a Windows manifest, application icon, Phosphor
//! context-menu sub-icons, and version metadata.
//!
//! ## Embedded icon resources
//!
//! | ID | File / Source | Usage |
//! |----|--------------|-------|
//! | 1 | `assets/app.ico` | Main application icon (taskbar, Explorer) |
//! | 2 | Phosphor LEAF glyph | Quick Clean context menu entry |
//! | 3 | Phosphor LIGHTNING glyph | Standard Clean context menu entry |
//! | 4 | Phosphor FIRE glyph | Deep Clean context menu entry |
//! | 5 | Phosphor BROOM glyph | Purge Standby List context menu entry |
//! | 6 | Phosphor GAUGE glyph | Memory Status context menu entry |

use embed_manifest::manifest::ExecutionLevel;
use embed_manifest::{embed_manifest, new_manifest};

fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        // Embed UAC admin-elevation manifest
        let manifest = new_manifest("MagicX.RAMCleaner")
            .requested_execution_level(ExecutionLevel::RequireAdministrator);
        embed_manifest(manifest).expect("unable to embed manifest file");

        // ── Render Phosphor glyph ICO files into OUT_DIR ─────────────────
        let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
        let glyphs: &[(u32, char, &str)] = &[
            (2, '\u{E2DA}', "LEAF"),      // Quick Clean
            (3, '\u{E2DE}', "LIGHTNING"), // Standard Clean
            (4, '\u{E242}', "FIRE"),      // Deep Clean
            (5, '\u{EC54}', "BROOM"),     // Purge Standby List
            (6, '\u{E628}', "GAUGE"),     // Memory Status
        ];

        // ── Embed all icons as Win32 resources ───────────────────────────
        //
        // Icon resource ID 1 = app.ico  (main application icon)
        // Icon resource IDs 2–6 = Phosphor glyphs for context menu entries
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");

        for &(id, codepoint, name) in glyphs {
            let ico_bytes = render_glyph_ico(codepoint);
            let ico_path = format!("{out_dir}/phosphor_{id}_{name}.ico");
            std::fs::write(&ico_path, ico_bytes)
                .unwrap_or_else(|e| panic!("failed to write {ico_path}: {e}"));
            res.set_icon_with_id(&ico_path, &id.to_string());
        }

        res.compile()
            .expect("unable to compile Windows resource file");
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=assets/app.ico");
}

// ── Phosphor glyph → ICO rendering (build-time only) ─────────────────────────

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};

/// Icon render size in pixels.
const ICON_SIZE: u32 = 16;

/// Glyph colour — sky blue accent, visible on both light and dark backgrounds.
const GLYPH_RGB: (u8, u8, u8) = (56, 189, 248);

/// Rasterize a Phosphor Regular glyph and encode it as a `.ico` file.
fn render_glyph_ico(codepoint: char) -> Vec<u8> {
    let rgba = rasterize_phosphor_glyph(codepoint);
    encode_ico(&rgba, ICON_SIZE)
}

/// Rasterize a single Phosphor Regular glyph into an `ICON_SIZE`×`ICON_SIZE`
/// RGBA pixel buffer.
fn rasterize_phosphor_glyph(codepoint: char) -> Vec<u8> {
    let font_bytes = egui_phosphor::Variant::Regular.font_bytes();
    let font = FontRef::try_from_slice(font_bytes).expect("failed to load Phosphor font for icons");

    let glyph_id = font.glyph_id(codepoint);
    assert!(
        glyph_id.0 != 0,
        "Phosphor font missing codepoint U+{:04X}",
        codepoint as u32
    );

    let scale = PxScale::from(ICON_SIZE as f32);
    let scaled = font.as_scaled(scale);
    let positioned = glyph_id.with_scale_and_position(scale, point(0.0, scaled.ascent()));

    let outlined = font
        .outline_glyph(positioned)
        .unwrap_or_else(|| panic!("failed to outline glyph U+{:04X}", codepoint as u32));
    let bounds = outlined.px_bounds();

    let (gw, gh) = (bounds.width() as u32, bounds.height() as u32);
    assert!(gw > 0 && gh > 0, "glyph has zero dimensions");

    let canvas = ICON_SIZE;
    let off_x = canvas.saturating_sub(gw) / 2;
    let off_y = canvas.saturating_sub(gh) / 2;

    let mut rgba = vec![0u8; (canvas * canvas * 4) as usize];
    let (r, g, b) = GLYPH_RGB;

    outlined.draw(|x, y, coverage| {
        let px = x + off_x;
        let py = y + off_y;
        if px < canvas && py < canvas {
            let idx = ((py * canvas + px) * 4) as usize;
            let alpha = (coverage * 255.0) as u8;
            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = alpha;
        }
    });

    rgba
}

/// Encode an RGBA pixel buffer as a single-image ICO file (BMP-in-ICO format).
fn encode_ico(rgba: &[u8], size: u32) -> Vec<u8> {
    let row_bytes = (size * 4) as usize;
    // AND mask: 1 bit per pixel, rows padded to 4-byte boundary.
    let mask_row = (size.div_ceil(32) * 4) as usize;
    let mask_size = mask_row * size as usize;
    let bmp_data_size = row_bytes * size as usize + mask_size;
    let bih_size: u32 = 40;
    let image_size = bih_size + bmp_data_size as u32;
    let data_offset: u32 = 6 + 16; // ICO header + 1 directory entry

    let mut buf = Vec::with_capacity((data_offset + image_size) as usize);

    // ── ICO header ───────────────────────────────────────────────────
    buf.extend_from_slice(&0u16.to_le_bytes()); // reserved
    buf.extend_from_slice(&1u16.to_le_bytes()); // type = icon
    buf.extend_from_slice(&1u16.to_le_bytes()); // image count

    // ── Directory entry ──────────────────────────────────────────────
    let dim = if size >= 256 { 0u8 } else { size as u8 };
    buf.push(dim); // width
    buf.push(dim); // height
    buf.push(0); // colour palette count
    buf.push(0); // reserved
    buf.extend_from_slice(&1u16.to_le_bytes()); // colour planes
    buf.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    buf.extend_from_slice(&image_size.to_le_bytes()); // image data size
    buf.extend_from_slice(&data_offset.to_le_bytes()); // offset to image data

    // ── BITMAPINFOHEADER ─────────────────────────────────────────────
    buf.extend_from_slice(&bih_size.to_le_bytes()); // header size
    buf.extend_from_slice(&(size as i32).to_le_bytes()); // width
    // Height is 2× for ICO (XOR image + AND mask combined).
    buf.extend_from_slice(&(size as i32 * 2).to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // planes
    buf.extend_from_slice(&32u16.to_le_bytes()); // bpp
    buf.extend_from_slice(&0u32.to_le_bytes()); // compression (BI_RGB)
    buf.extend_from_slice(&0u32.to_le_bytes()); // image size (can be 0 for BI_RGB)
    buf.extend_from_slice(&0i32.to_le_bytes()); // x pixels per metre
    buf.extend_from_slice(&0i32.to_le_bytes()); // y pixels per metre
    buf.extend_from_slice(&0u32.to_le_bytes()); // colours used
    buf.extend_from_slice(&0u32.to_le_bytes()); // important colours

    // ── Pixel data (BGRA, bottom-up) ─────────────────────────────────
    for y in (0..size).rev() {
        for x in 0..size {
            let src = ((y * size + x) * 4) as usize;
            // RGBA → BGRA
            buf.push(rgba[src + 2]); // B
            buf.push(rgba[src + 1]); // G
            buf.push(rgba[src]); // R
            buf.push(rgba[src + 3]); // A
        }
    }

    // ── AND mask (all transparent — alpha channel handles it) ────────
    buf.resize(buf.len() + mask_size, 0);

    buf
}
