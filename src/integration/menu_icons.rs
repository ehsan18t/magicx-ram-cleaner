// The context menu's icon resources: one table shared by `build.rs`, which
// renders each Phosphor glyph and embeds it under its resource ID, and by
// `context_menu`, which points each menu entry at its ID. `build.rs` pulls
// this file in with `include!`, so it holds only plain constants.

/// Resource ID of the Quick Clean icon (Phosphor LEAF).
pub const ICON_QUICK_CLEAN: u32 = 2;
/// Resource ID of the Standard Clean icon (Phosphor LIGHTNING).
pub const ICON_STANDARD_CLEAN: u32 = 3;
/// Resource ID of the Deep Clean icon (Phosphor FIRE).
pub const ICON_DEEP_CLEAN: u32 = 4;
/// Resource ID of the Purge Standby List icon (Phosphor BROOM).
pub const ICON_PURGE_STANDBY: u32 = 5;
/// Resource ID of the Memory Status icon (Phosphor GAUGE).
pub const ICON_MEMORY_STATUS: u32 = 6;

/// Every menu icon: resource ID, Phosphor Regular codepoint, glyph name.
/// Resource ID 1 is the app icon (`assets/app.ico`).
// Read by build.rs and by the tests; the app itself only needs the IDs.
#[allow(dead_code)]
pub const MENU_ICONS: &[(u32, char, &str)] = &[
    (ICON_QUICK_CLEAN, '\u{E2DA}', "LEAF"),
    (ICON_STANDARD_CLEAN, '\u{E2DE}', "LIGHTNING"),
    (ICON_DEEP_CLEAN, '\u{E242}', "FIRE"),
    (ICON_PURGE_STANDBY, '\u{EC54}', "BROOM"),
    (ICON_MEMORY_STATUS, '\u{E628}', "GAUGE"),
];
