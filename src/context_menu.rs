//! # `MagicX` RAM Cleaner — Windows Context Menu Integration
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
//!     01boost\
//!       MUIVerb   = "Boost"
//!       Icon      = "C:\...\magicx-ram-cleaner.exe,-2"
//!       command\  (default) = '"C:\...\exe" clean --level gentle --notify'
//!     02moderate\
//!       …
//!     03aggressive\
//!       …
//!     04purge_standby\
//!       …
//!     05status\
//!       …
//! ```
//!
//! ## Icon resource IDs
//!
//! The `.ico` files are embedded as Win32 `ICON` resources with numeric IDs:
//! - ID `1` — `app.ico` (default/main icon, also used for root menu + purge/status)
//! - ID `2` — `lite.ico` (gentle / moderate entries)
//! - ID `3` — `aggressive.ico` (aggressive entry)
//!
//! Registry `Icon` values reference them as `"<exe_path>,-<resource_id>"`.
//! Adding a new icon: embed it in `build.rs` with the next sequential ID,
//! then reference it in `ENTRIES`.

use anyhow::{Context, Result, bail};
use colored::Colorize;

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
    /// Win32 resource ID for the icon embedded in the exe.
    ///
    /// Referenced in the registry as `"<exe_path>,-<id>"` (negative = resource ID).
    icon_resource_id: u32,
    /// CLI arguments appended after the exe path in the `command` key.
    args: &'static str,
}

/// All context menu entries, in display order.
/// Nuclear is intentionally excluded — it is too destructive for a one-click action.
const ENTRIES: &[MenuEntry] = &[
    MenuEntry {
        key: "01boost",
        label: "Boost",
        icon_resource_id: 2, // lite.ico
        args: "clean --level gentle --notify",
    },
    MenuEntry {
        key: "02moderate",
        label: "Moderate Boost",
        icon_resource_id: 2, // lite.ico
        args: "clean --level moderate --notify",
    },
    MenuEntry {
        key: "03aggressive",
        label: "Aggressive Boost",
        icon_resource_id: 3, // aggressive.ico
        args: "clean --level aggressive --notify",
    },
    MenuEntry {
        key: "04purge_standby",
        label: "Purge Standby",
        icon_resource_id: 1, // app.ico
        args: "purge-standby --notify",
    },
    MenuEntry {
        key: "05status",
        label: "Memory Status",
        icon_resource_id: 1, // app.ico
        args: "status --notify",
    },
];

// ─── Windows registry FFI ────────────────────────────────────────────────────

// Minimal registry bindings from windows-sys.
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CLASSES_ROOT, KEY_ALL_ACCESS, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegDeleteTreeW, RegOpenKeyExW, RegSetValueExW,
};

/// RAII wrapper for a registry `HKEY`.
struct RegKeyGuard {
    hkey: HKEY,
}

impl RegKeyGuard {
    /// Wrap a raw `HKEY`. The caller is responsible for providing a valid,
    /// open key handle.
    const fn new(hkey: HKEY) -> Self {
        Self { hkey }
    }

    /// Borrow the raw handle.
    const fn raw(&self) -> HKEY {
        self.hkey
    }
}

impl Drop for RegKeyGuard {
    fn drop(&mut self) {
        if !self.hkey.is_null() {
            // SAFETY: hkey is a valid, open registry key that must be closed.
            unsafe { RegCloseKey(self.hkey) };
        }
    }
}

use crate::stats::to_wide;

/// Open or create a registry key under `HKEY_CLASSES_ROOT`.
///
/// # Safety contract
///
/// All pointers passed to `RegCreateKeyExW` point to valid, properly sized
/// stack/heap allocations. The returned handle is wrapped in `RegKeyGuard`
/// for automatic cleanup.
fn create_key(parent: HKEY, sub_path: &str) -> Result<RegKeyGuard> {
    let wide = to_wide(sub_path);
    let mut hkey: HKEY = std::ptr::null_mut();
    let mut disposition: u32 = 0;
    // SAFETY: All parameters are valid: wide is null-terminated UTF-16,
    // hkey and disposition are valid stack-allocated output variables.
    let rc = unsafe {
        RegCreateKeyExW(
            parent,
            wide.as_ptr(),
            0,
            std::ptr::null_mut(),
            REG_OPTION_NON_VOLATILE,
            KEY_ALL_ACCESS,
            std::ptr::null(),
            &raw mut hkey,
            &raw mut disposition,
        )
    };
    if rc != 0 {
        bail!("RegCreateKeyExW failed for '{sub_path}': error {rc}");
    }
    Ok(RegKeyGuard::new(hkey))
}

/// Set a `REG_SZ` (string) value on an open registry key.
///
/// `name` is the value name; use `""` for the default `(Default)` value.
fn set_string(hkey: HKEY, name: &str, value: &str) -> Result<()> {
    let wide_name = to_wide(name);
    let wide_value = to_wide(value);
    // Byte length including the null terminator (REG_SZ requires it).
    let byte_len = (wide_value.len() * std::mem::size_of::<u16>()) as u32;
    // SAFETY: wide_name and wide_value are valid null-terminated UTF-16 buffers.
    // byte_len correctly reflects the buffer size in bytes.
    let rc = unsafe {
        RegSetValueExW(
            hkey,
            wide_name.as_ptr(),
            0,
            REG_SZ,
            wide_value.as_ptr().cast(),
            byte_len,
        )
    };
    if rc != 0 {
        bail!("RegSetValueExW failed for value '{name}': error {rc}");
    }
    Ok(())
}

/// Recursively delete a registry key and all its sub-keys.
///
/// `RegDeleteTreeW` requires `HKEY_CLASSES_ROOT` and the path to the key to
/// delete. Returns `Ok(())` if the key does not exist (idempotent).
fn delete_key_tree(parent: HKEY, sub_path: &str) -> Result<()> {
    let wide = to_wide(sub_path);
    // SAFETY: wide is a valid null-terminated UTF-16 string. HKEY_CLASSES_ROOT
    // is a predefined root key that is always valid.
    let rc = unsafe { RegDeleteTreeW(parent, wide.as_ptr()) };
    // 2 = ERROR_FILE_NOT_FOUND — key already absent, treat as success
    if rc != 0 && rc != 2 {
        bail!("RegDeleteTreeW failed for '{sub_path}': error {rc}");
    }
    Ok(())
}

/// Check whether the root context menu key already exists.
fn key_exists(parent: HKEY, sub_path: &str) -> bool {
    let wide = to_wide(sub_path);
    let mut hkey: HKEY = std::ptr::null_mut();
    // SAFETY: wide is a valid null-terminated UTF-16 string.
    let rc = unsafe { RegOpenKeyExW(parent, wide.as_ptr(), 0, KEY_ALL_ACCESS, &raw mut hkey) };
    if rc == 0 && !hkey.is_null() {
        // SAFETY: hkey is a valid open key; drop to close it immediately.
        let _guard = RegKeyGuard::new(hkey);
        true
    } else {
        false
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Install the `MagicX RAM Cleaner` context menu entries.
///
/// Writes entries under both `HKCR\DesktopBackground\Shell` and
/// `HKCR\Directory\Background\Shell` so the menu is visible when
/// right-clicking the Desktop background **and** inside folder windows.
///
/// The caller must be running as Administrator (HKCR writes require elevation).
/// If entries already exist they are replaced cleanly (delete + recreate).
pub fn install(exe_path: &str) -> Result<()> {
    for root_path in ROOT_PATHS {
        install_at(exe_path, root_path)
            .with_context(|| format!("failed to install context menu at '{root_path}'"))?;
    }

    println!();
    println!(
        "  {} Context menu installed successfully!",
        "\u{2713}".green().bold()
    );
    println!(
        "  {} Right-click your Desktop or inside any folder to see the {} submenu.",
        "\u{2192}".cyan(),
        "MagicX RAM Cleaner".white().bold()
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
    delete_key_tree(HKEY_CLASSES_ROOT, root_path)
        .context("failed to remove existing context menu entries")?;

    // ── Root submenu key ──────────────────────────────────────────────────
    let root = create_key(HKEY_CLASSES_ROOT, root_path)
        .context("failed to create root context menu key")?;

    // MUIVerb is the display name; do NOT set (Default) on the root key
    // because the shell interprets it as a verb name and an unexpected
    // value can prevent the cascading submenu from expanding.
    set_string(root.raw(), "MUIVerb", "MagicX RAM Cleaner").context("failed to set MUIVerb")?;
    set_string(root.raw(), "SubCommands", "").context("failed to set SubCommands")?;
    set_string(root.raw(), "Icon", &format!("{exe_path},-1")).context("failed to set root Icon")?;

    // ── Shell sub-key ─────────────────────────────────────────────────────
    let shell_path = format!(r"{root_path}\Shell");
    let _shell =
        create_key(HKEY_CLASSES_ROOT, &shell_path).context("failed to create Shell sub-key")?;

    // ── Individual entries ────────────────────────────────────────────────
    for entry in ENTRIES {
        let entry_path = format!(r"{shell_path}\{}", entry.key);
        let cmd_path = format!(r"{entry_path}\command");

        let entry_key = create_key(HKEY_CLASSES_ROOT, &entry_path)
            .with_context(|| format!("failed to create entry key '{}'", entry.key))?;

        // Use MUIVerb for the display label (consistent with the root key).
        set_string(entry_key.raw(), "MUIVerb", entry.label)
            .with_context(|| format!("failed to set MUIVerb for '{}'", entry.key))?;
        set_string(
            entry_key.raw(),
            "Icon",
            &format!("{exe_path},-{}", entry.icon_resource_id),
        )
        .with_context(|| format!("failed to set icon for '{}'", entry.key))?;

        let cmd_key = create_key(HKEY_CLASSES_ROOT, &cmd_path)
            .with_context(|| format!("failed to create command key for '{}'", entry.key))?;

        let command = format!(r#""{exe_path}" {}"#, entry.args);
        set_string(cmd_key.raw(), "", &command)
            .with_context(|| format!("failed to set command for '{}'", entry.key))?;
    }

    Ok(())
}

/// Uninstall all `MagicX RAM Cleaner` context menu entries.
///
/// Removes the `MagicXRAMCleaner` key from all registered roots
/// (`DesktopBackground` and `Directory\Background`). Idempotent —
/// succeeds even if the keys do not exist.
pub fn uninstall() -> Result<()> {
    let existed = is_installed();

    for root_path in ROOT_PATHS {
        delete_key_tree(HKEY_CLASSES_ROOT, root_path)
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
        .any(|path| key_exists(HKEY_CLASSES_ROOT, path))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_have_valid_icon_resource_ids() {
        for entry in ENTRIES {
            assert!(
                entry.icon_resource_id >= 1 && entry.icon_resource_id <= 3,
                "entry '{}' has out-of-range icon_resource_id {}",
                entry.key,
                entry.icon_resource_id
            );
        }
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

    #[test]
    fn to_wide_roundtrip() {
        let s = "MagicX RAM Cleaner";
        let wide = to_wide(s);
        // Last element must be null terminator
        assert_eq!(*wide.last().unwrap(), 0u16);
        // Round-trip back to UTF-8
        let len = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
        let back = String::from_utf16_lossy(&wide[..len]);
        assert_eq!(back, s);
    }
}
