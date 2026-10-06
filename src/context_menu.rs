//! # `MagicX` RAM Cleaner - Windows Context Menu Integration
//!
//! Installs and uninstalls right-click context menu entries so that the
//! cleaning operations are accessible without opening a terminal.
//!
//! The menu appears when right-clicking on:
//! - **Desktop background** (`HKCR\DesktopBackground\Shell`)
//! - **Folder window background** (`HKCR\Directory\Background\Shell`)
//!
//! ## Registry layout (per root)
//!
//! ```text
//! HKCR\<root>\Shell\zMagicXRAMCleaner\
//!   MUIVerb     = "MagicX RAM Cleaner"
//!   SubCommands = ""
//!   Icon        = "C:\...\magicx-ram-cleaner.exe,-1"
//!   Shell\
//!     01quick\
//!       MUIVerb   = "Quick Clean"
//!       Icon      = "C:\...\magicx-ram-cleaner.exe,-2"
//!       command\  (default) = '"C:\...\exe" clean --level gentle --notify'
//!     02standard\
//!       …
//!     03deep\
//!       …
//!     04purge_standby\
//!       …
//!     05status\
//!       …
//! ```
//!
//! ## Icon resource IDs
//!
//! All icons are embedded as Win32 `ICON` resources in the executable at build
//! time (see `build.rs`). The root cascading menu uses `app.ico` (resource
//! ID 1). Each sub-entry references a Phosphor glyph icon rendered during
//! compilation and embedded with resource IDs 2–6.

use anyhow::{Context, Result};
use colored::Colorize;

use crate::platform::registry::{self, Hive, RegKey};
use crate::strings;

// ─── Registry key paths ──────────────────────────────────────────────────────

/// Registry roots where the context menu is installed.
///
/// Each path is a location under `HKEY_CLASSES_ROOT` where a background
/// right-click context menu can be registered.
const ROOT_PATHS: &[&str] = &[
    r"DesktopBackground\Shell\zMagicXRAMCleaner",
    r"Directory\Background\Shell\zMagicXRAMCleaner",
];

// ─── Menu entry definitions ──────────────────────────────────────────────────

/// A single context menu entry.
struct MenuEntry {
    /// Registry sub-key name under `…\MagicXRAMCleaner\Shell\` (prefixed for ordering).
    key: &'static str,
    /// Label shown in the context menu.
    label: &'static str,
    /// Win32 icon resource ID embedded in the executable (see `build.rs`).
    icon_resource_id: u32,
    /// CLI arguments appended after the exe path in the `command` key.
    args: &'static str,
}

/// All context menu entries, in display order.
/// Nuclear is intentionally excluded - it is too destructive for a one-click action.
const ENTRIES: &[MenuEntry] = &[
    MenuEntry {
        key: "01quick",
        label: strings::context_menu::QUICK_CLEAN,
        icon_resource_id: 2, // Phosphor LEAF
        args: "clean --level gentle --notify",
    },
    MenuEntry {
        key: "02standard",
        label: strings::context_menu::STANDARD_CLEAN,
        icon_resource_id: 3, // Phosphor LIGHTNING
        args: "clean --level moderate --notify",
    },
    MenuEntry {
        key: "03deep",
        label: strings::context_menu::DEEP_CLEAN,
        icon_resource_id: 4, // Phosphor FIRE
        args: "clean --level aggressive --notify",
    },
    MenuEntry {
        key: "04purge_standby",
        label: strings::context_menu::PURGE_STANDBY,
        icon_resource_id: 5, // Phosphor BROOM
        args: "purge-standby --notify",
    },
    MenuEntry {
        key: "05status",
        label: strings::context_menu::MEMORY_STATUS,
        icon_resource_id: 6, // Phosphor GAUGE
        args: "status --notify",
    },
];

// ─── Public API ──────────────────────────────────────────────────────────────

/// Install the `MagicX RAM Cleaner` context menu entries.
///
/// Writes entries under both `HKCR\DesktopBackground\Shell` and
/// `HKCR\Directory\Background\Shell` so the menu is visible when
/// right-clicking the Desktop background **and** inside folder windows.
///
/// The caller must be running as Administrator (HKCR writes require elevation).
/// If entries already exist they are replaced cleanly (delete + recreate).
/// Installation is all-or-nothing: if any root fails, every root is removed
/// again so the menu never ends up half-installed.
pub fn install(exe_path: &str) -> Result<()> {
    for root_path in ROOT_PATHS {
        if let Err(e) = install_at(exe_path, root_path) {
            for cleanup_path in ROOT_PATHS {
                drop(registry::delete_tree(Hive::ClassesRoot, cleanup_path));
            }
            return Err(e)
                .with_context(|| format!("failed to install context menu at '{root_path}'"));
        }
    }

    println!();
    println!(
        "  {} Context menu installed successfully!",
        "\u{2713}".green().bold()
    );
    println!(
        "  {} Right-click your Desktop or inside any folder to see the {} submenu.",
        "\u{2192}".cyan(),
        strings::context_menu::ROOT_LABEL.white().bold()
    );
    println!(
        "  {} {} entries registered:",
        "\u{2192}".cyan(),
        ENTRIES.len()
    );
    for entry in ENTRIES {
        println!("      {} {}", "\u{00b7}".dimmed(), entry.label.white());
    }
    println!();

    Ok(())
}

/// Write the cascading menu tree under a single registry root path.
fn install_at(exe_path: &str, root_path: &str) -> Result<()> {
    // Remove any stale installation first for a clean slate
    registry::delete_tree(Hive::ClassesRoot, root_path)
        .context("failed to remove existing context menu entries")?;

    // ── Root submenu key ──────────────────────────────────────────────────
    let root = RegKey::create(Hive::ClassesRoot, root_path)
        .context("failed to create root context menu key")?;

    // MUIVerb is the display name; do NOT set (Default) on the root key
    // because the shell interprets it as a verb name and an unexpected
    // value can prevent the cascading submenu from expanding.
    root.set_string("MUIVerb", strings::context_menu::ROOT_LABEL)
        .context("failed to set MUIVerb")?;
    root.set_string("SubCommands", "")
        .context("failed to set SubCommands")?;
    root.set_string("Icon", &format!("{exe_path},-1"))
        .context("failed to set root Icon")?;

    // ── Shell sub-key ─────────────────────────────────────────────────────
    let shell_path = format!(r"{root_path}\Shell");
    let _shell =
        RegKey::create(Hive::ClassesRoot, &shell_path).context("failed to create Shell sub-key")?;

    // ── Individual entries ────────────────────────────────────────────────
    for entry in ENTRIES {
        let entry_path = format!(r"{shell_path}\{}", entry.key);
        let cmd_path = format!(r"{entry_path}\command");

        let entry_key = RegKey::create(Hive::ClassesRoot, &entry_path)
            .with_context(|| format!("failed to create entry key '{}'", entry.key))?;

        // Use MUIVerb for the display label (consistent with the root key).
        entry_key
            .set_string("MUIVerb", entry.label)
            .with_context(|| format!("failed to set MUIVerb for '{}'", entry.key))?;

        // Reference the Phosphor glyph icon embedded in the exe at build time.
        let icon_value = format!("{exe_path},-{}", entry.icon_resource_id);
        entry_key
            .set_string("Icon", &icon_value)
            .with_context(|| format!("failed to set icon for '{}'", entry.key))?;

        let cmd_key = RegKey::create(Hive::ClassesRoot, &cmd_path)
            .with_context(|| format!("failed to create command key for '{}'", entry.key))?;

        let command = format!(r#""{exe_path}" {}"#, entry.args);
        cmd_key
            .set_string("", &command)
            .with_context(|| format!("failed to set command for '{}'", entry.key))?;
    }

    Ok(())
}

/// Uninstall all `MagicX RAM Cleaner` context menu entries.
///
/// Removes the `MagicXRAMCleaner` key from all registered roots
/// (`DesktopBackground` and `Directory\Background`). Idempotent  -
/// succeeds even if the keys do not exist.
pub fn uninstall() -> Result<()> {
    let existed = is_installed();

    for root_path in ROOT_PATHS {
        registry::delete_tree(Hive::ClassesRoot, root_path)
            .with_context(|| format!("failed to remove context menu at '{root_path}'"))?;
    }

    println!();
    if existed {
        println!(
            "  {} Context menu entries removed successfully.",
            "\u{2713}".green().bold()
        );
    } else {
        println!(
            "  {} Context menu entries were not installed.",
            "\u{00b7}".dimmed()
        );
    }
    println!();

    Ok(())
}

/// Return the absolute path to the current executable.
///
/// Used by [`install`] to write the correct `Icon` and `command` values.
pub fn current_exe_path() -> Result<String> {
    let path = std::env::current_exe().context("failed to determine current executable path")?;
    path.to_str()
        .context("executable path contains non-UTF-8 characters")
        .map(str::to_owned)
}

/// Check whether the context menu is currently installed.
///
/// Returns `true` if **any** of the registered root paths exist.
#[must_use]
pub fn is_installed() -> bool {
    ROOT_PATHS
        .iter()
        .any(|path| registry::key_exists(Hive::ClassesRoot, path))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_have_valid_resource_ids() {
        for entry in ENTRIES {
            assert!(
                entry.icon_resource_id >= 2,
                "entry '{}' resource ID must be >= 2 (ID 1 is app.ico)",
                entry.key
            );
        }
    }

    #[test]
    fn entries_resource_ids_are_unique() {
        let mut ids: Vec<u32> = ENTRIES.iter().map(|e| e.icon_resource_id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(
            ids.len(),
            ENTRIES.len(),
            "entry icon_resource_id values must be unique"
        );
    }

    #[test]
    fn entries_keys_are_ordered_and_unique() {
        let keys: Vec<&str> = ENTRIES.iter().map(|e| e.key).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        // Dedup after sort to check uniqueness
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len(), "entry keys must be unique");
        // Keys must already be in sorted (display) order
        assert_eq!(keys, sorted, "entry keys must be in ascending order");
    }

    #[test]
    fn entries_args_are_not_empty() {
        for entry in ENTRIES {
            assert!(
                !entry.args.is_empty(),
                "entry '{}' has empty args",
                entry.key
            );
        }
    }

    #[test]
    fn no_nuclear_entry() {
        for entry in ENTRIES {
            assert!(
                !entry.args.contains("nuclear"),
                "nuclear level must not appear in context menu entries (entry: '{}')",
                entry.key
            );
        }
    }

    #[test]
    fn root_paths_end_with_expected_key_name() {
        for path in ROOT_PATHS {
            assert!(
                path.ends_with("zMagicXRAMCleaner"),
                "root path '{path}' must end with the z-prefixed key name \
                 (the z prefix pushes the entry to the bottom of the context menu)"
            );
        }
    }

    #[test]
    fn root_paths_are_unique() {
        let mut paths = ROOT_PATHS.to_vec();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(
            paths.len(),
            ROOT_PATHS.len(),
            "ROOT_PATHS must not contain duplicates"
        );
    }
}
