//! Fixed names other code and Windows look things up by.
//!
//! These are identifiers, not display text: changing one breaks whatever
//! finds the app by it (a second launch finding the running window, the
//! autostart task, cleanup of the old Run value). They deliberately do not
//! come from the display name in `strings`, so rewording the UI can never
//! break them.

/// Title of the main window. A second launch finds the running instance's
/// window by this title (and the same executable), so the title must stay
/// fixed. eframe also uses it as the app id.
pub const WINDOW_TITLE: &str = "MagicX RAM Cleaner";

/// Name of the Task Scheduler task that starts the app at sign-in.
pub const AUTOSTART_TASK_NAME: &str = "MagicX RAM Cleaner";

/// Name of the `HKCU\...\Run` value older versions wrote for autostart,
/// removed whenever autostart is changed.
pub const LEGACY_RUN_VALUE: &str = "MagicX RAM Cleaner";
