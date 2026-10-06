//! # Settings persistence
//!
//! Loading, saving, importing and exporting [`GuiSettings`]. (Autostart is
//! Windows integration, see [`crate::integration::autostart`].)
//!
//! The settings live in `settings.json` next to the running executable, so a
//! portable copy carries its settings with it. Import and export open native
//! Win32 file-picker dialogs (COMDLG32).
//!
//! A damaged file never stops the app from starting, and is never silently
//! replaced: a field with an invalid value falls back to its default on its
//! own, and a file that cannot be used at all is kept as `settings.json.bad`.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::settings::GuiSettings;
use crate::platform::dialog;
use crate::strings;

/// `<exe directory>\settings.json`, or `settings.json` in the current
/// directory if the executable path cannot be resolved.
fn default_settings_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("settings.json")
}

/// The result of loading the settings file.
#[derive(Debug)]
pub enum Loaded {
    /// There is no settings file yet (first run).
    Missing,
    /// The file was read. `reset_fields` names fields whose stored value was
    /// invalid (wrong type, unknown option) and that took their default.
    Read {
        /// The loaded, sanitised settings.
        settings: GuiSettings,
        /// Fields reset to their defaults.
        reset_fields: Vec<String>,
    },
    /// The file exists but could not be used; defaults apply.
    Unusable {
        /// Why the file could not be used.
        error: String,
        /// Where the damaged file was moved so it is not overwritten, when
        /// it could be moved (a file that could not even be read stays put).
        kept_as: Option<PathBuf>,
    },
}

/// Load the settings from the default path next to the executable.
#[must_use]
pub fn load() -> Loaded {
    load_from(&default_settings_path())
}

/// Load the settings from `path` (see [`load`]).
fn load_from(path: &Path) -> Loaded {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Loaded::Missing,
        Err(e) => {
            return Loaded::Unusable {
                error: format!("Cannot read {}: {e}", path.display()),
                kept_as: None,
            };
        }
    };
    match parse_settings(&content) {
        Ok((settings, reset_fields)) => Loaded::Read {
            settings,
            reset_fields,
        },
        Err(e) => {
            let mut bad = path.as_os_str().to_owned();
            bad.push(".bad");
            let bad = PathBuf::from(bad);
            Loaded::Unusable {
                error: format!("{e:#}"),
                kept_as: std::fs::rename(path, &bad).is_ok().then_some(bad),
            }
        }
    }
}

/// Parse settings JSON, upgrade fields from older versions, and clamp the
/// result to valid ranges. Returns the settings and the fields that held an
/// invalid value and were reset to their defaults.
///
/// Fails only when the text is not a JSON object at all.
fn parse_settings(content: &str) -> Result<(GuiSettings, Vec<String>)> {
    let mut value: serde_json::Value =
        serde_json::from_str(content).context("Invalid settings file")?;
    migrate_legacy_fields(&mut value);
    let serde_json::Value::Object(stored) = value else {
        anyhow::bail!("Invalid settings file: expected a JSON object");
    };

    // Start from the defaults and take each stored field that is valid on
    // its own, so one bad value cannot cost the user every other setting.
    let serde_json::Value::Object(mut merged) = serde_json::to_value(GuiSettings::default())?
    else {
        unreachable!("settings serialise to a JSON object");
    };
    let mut reset_fields = Vec::new();
    for (key, stored_value) in stored {
        if !merged.contains_key(&key) {
            continue; // unknown field (e.g. from a newer version): ignored
        }
        let mut candidate = merged.clone();
        candidate.insert(key.clone(), stored_value.clone());
        if serde_json::from_value::<GuiSettings>(serde_json::Value::Object(candidate)).is_ok() {
            merged.insert(key, stored_value);
        } else {
            reset_fields.push(key);
        }
    }
    let mut settings: GuiSettings = serde_json::from_value(serde_json::Value::Object(merged))?;
    settings.sanitize();
    Ok((settings, reset_fields))
}

/// Upgrade field names and values written by older versions.
///
/// - `tray_enabled` was renamed to `minimize_to_tray`.
/// - Files written before the `theme` field existed stored
///   `"dark_mode": bool`; it becomes the equivalent fixed theme so an update
///   never flips the user's theme. Files with neither field get the default
///   (System).
fn migrate_legacy_fields(value: &mut serde_json::Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if let Some(tray) = object.remove("tray_enabled") {
        object.entry("minimize_to_tray").or_insert(tray);
    }
    if object.contains_key("theme") {
        return;
    }
    if let Some(dark) = object.get("dark_mode").and_then(serde_json::Value::as_bool) {
        let theme = if dark { "dark" } else { "light" };
        object.insert("theme".to_owned(), serde_json::Value::from(theme));
    }
}

/// Serialise `settings` as pretty JSON to `path`, creating parent directories.
///
/// The JSON is written to a temporary sibling file, flushed to disk, and then
/// renamed over `path`, so a crash or power loss leaves either the old file
/// or the new one, never a truncated mix. The temporary name includes the
/// process ID, so two instances saving at once cannot write the same file.
fn write_settings_file(path: &Path, settings: &GuiSettings) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(settings)?;

    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(format!(".{}.tmp", std::process::id()));
    let tmp_path = path.with_file_name(tmp_name);

    let written = std::fs::File::create(&tmp_path).and_then(|mut file| {
        file.write_all(json.as_bytes())?;
        file.sync_all()
    });
    let result = written
        .with_context(|| format!("Cannot write {}", tmp_path.display()))
        .and_then(|()| {
            std::fs::rename(&tmp_path, path)
                .with_context(|| format!("Cannot replace {}", path.display()))
        });
    if result.is_err() {
        // Best-effort cleanup; the error that matters is already in `result`.
        drop(std::fs::remove_file(&tmp_path));
    }
    result
}

/// Save `settings` to the default path next to the executable.
///
/// # Errors
///
/// Fails if the file cannot be written, e.g. on read-only media.
pub fn save(settings: &GuiSettings) -> Result<()> {
    write_settings_file(&default_settings_path(), settings)
}

/// Export `settings` to a file the user picks in a native Save dialog,
/// modal to `owner`. Returns the path written, or `Ok(None)` if cancelled.
///
/// # Errors
///
/// Fails if the dialog fails or the file cannot be written.
pub fn export(owner: isize, settings: &GuiSettings) -> Result<Option<PathBuf>> {
    let Some(path) = dialog::pick_save_json(
        owner,
        strings::gui::persistence::EXPORT_TITLE,
        "magicx-settings.json",
    )?
    else {
        return Ok(None);
    };
    write_settings_file(&path, settings)?;
    Ok(Some(path))
}

/// Import settings from a file the user picks in a native Open dialog,
/// modal to `owner`. Returns the settings and any fields reset to their
/// defaults, or `Ok(None)` if cancelled.
///
/// # Errors
///
/// Fails if the dialog fails, or the file cannot be read or is not a
/// settings file at all.
pub fn import(owner: isize) -> Result<Option<(GuiSettings, Vec<String>)>> {
    let Some(path) = dialog::pick_open_json(owner, strings::gui::persistence::IMPORT_TITLE)? else {
        return Ok(None);
    };
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Cannot read {}", path.display()))?;
    parse_settings(&content).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::CleanLevel;
    use crate::gui::settings::ThemeMode;

    /// A fresh, empty directory for one test.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "magicx-settings-test-{name}-{}",
            std::process::id()
        ));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn parse(content: &str) -> GuiSettings {
        parse_settings(content).expect("loads").0
    }

    #[test]
    fn legacy_dark_mode_becomes_a_fixed_theme() {
        assert_eq!(parse(r#"{ "dark_mode": true }"#).theme, ThemeMode::Dark);
        assert_eq!(parse(r#"{ "dark_mode": false }"#).theme, ThemeMode::Light);
    }

    #[test]
    fn theme_field_wins_over_the_legacy_flag() {
        let settings = parse(r#"{ "dark_mode": true, "theme": "system" }"#);
        assert_eq!(settings.theme, ThemeMode::System);
    }

    #[test]
    fn files_without_either_field_follow_the_system() {
        assert_eq!(parse("{}").theme, ThemeMode::System);
    }

    #[test]
    fn legacy_tray_field_name_is_accepted() {
        assert!(parse(r#"{ "tray_enabled": true }"#).minimize_to_tray);
    }

    #[test]
    fn saved_settings_do_not_write_the_legacy_flag() {
        let json = serde_json::to_string(&GuiSettings::default()).expect("serializes");
        assert!(!json.contains("dark_mode"));
        assert!(json.contains(r#""theme":"system""#));
    }

    #[test]
    fn one_invalid_field_keeps_every_other_setting() {
        let (settings, reset) = parse_settings(
            r#"{ "monitor_threshold": "high", "theme": "purple", "auto_start": true,
                 "default_clean_level": "Gentle" }"#,
        )
        .expect("loads");
        assert!(settings.auto_start);
        assert_eq!(settings.default_clean_level, CleanLevel::Gentle);
        assert_eq!(settings.theme, ThemeMode::System, "invalid theme reset");
        assert_eq!(reset, ["monitor_threshold", "theme"]);
    }

    #[test]
    fn text_that_is_not_an_object_is_rejected() {
        assert!(parse_settings("not json").is_err());
        assert!(parse_settings("[1, 2]").is_err());
    }

    #[test]
    fn a_missing_file_is_a_first_run() {
        let dir = temp_dir("missing");
        assert!(matches!(
            load_from(&dir.join("settings.json")),
            Loaded::Missing
        ));
    }

    #[test]
    fn settings_round_trip_through_the_file() {
        let dir = temp_dir("round-trip");
        let path = dir.join("settings.json");
        let settings = GuiSettings {
            auto_start: true,
            monitor_threshold: 91,
            ..GuiSettings::default()
        };
        write_settings_file(&path, &settings).expect("writes");
        match load_from(&path) {
            Loaded::Read {
                settings: loaded,
                reset_fields,
            } => {
                assert_eq!(loaded, settings);
                assert_eq!(reset_fields, Vec::<String>::new());
            }
            other => panic!("unexpected {other:?}"),
        }
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .expect("lists")
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
            .collect();
        assert_eq!(leftovers.len(), 0, "temporary file left behind");
    }

    #[test]
    fn a_damaged_file_is_kept_aside_instead_of_overwritten() {
        let dir = temp_dir("damaged");
        let path = dir.join("settings.json");
        std::fs::write(&path, "{ this is not json").expect("writes");
        match load_from(&path) {
            Loaded::Unusable {
                kept_as: Some(kept),
                ..
            } => {
                assert_eq!(
                    std::fs::read_to_string(kept).expect("kept"),
                    "{ this is not json"
                );
                assert!(!path.exists(), "the damaged file was moved, not copied");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn saving_into_a_missing_folder_creates_it() {
        let dir = temp_dir("nested");
        let path = dir.join("a").join("b").join("settings.json");
        write_settings_file(&path, &GuiSettings::default()).expect("writes");
        assert!(path.exists());
    }
}
