//! # Centralised User-Facing Text
//!
//! Every display string shown to the user in the GUI or CLI lives here for
//! consistent, single-source management.  Organised by domain in nested
//! modules so call sites read naturally:
//!
//! ```text
//! strings::APP_NAME
//! strings::gui::overview::TITLE
//! strings::cli::PAUSE_PROMPT
//! ```
//!
//! ## What belongs here
//!
//! * Application identity (name, tagline, copyright)
//! * Developer profile info and URLs
//! * GUI panel titles, section headers, labels, button text
//! * CLI section headers, status labels, monitor messages
//! * System-tray and Desktop context-menu labels
//! * Notification balloon titles
//!
//! ## What stays in its source module
//!
//! * `format!()` templates with runtime values (only static parts extracted)
//! * CLI help-text constants with embedded ANSI codes (`cli/args.rs`)
//! * NT status code translations (`platform/nt.rs`)
//! * Error `.context()` messages (too granular)
//! * Registry paths, mutex names, and other implementation details

// ─── Application Identity ────────────────────────────────────────────────────

/// Application display name used in window titles, tooltips, and headings.
pub const APP_NAME: &str = "MagicX RAM Cleaner";

/// Short tagline shown in the GUI about-page hero section.
pub const APP_TAGLINE: &str = "The most powerful Windows RAM cleaner";

/// GitHub repository URL.
pub const REPO_URL: &str = "https://github.com/ehsan18t/magicx-ram-cleaner";

/// Short repository path for compact inline display.
pub const REPO_SHORT: &str = "ehsan18t/magicx-ram-cleaner";

/// Copyright notice.
pub const COPYRIGHT: &str = "\u{00a9} 2026 MagicXMod";

// ─── Developer ───────────────────────────────────────────────────────────────

/// Developer profile strings shown on the about page.
pub mod developer {
    /// Full display name.
    pub const NAME: &str = "Ehsan Khan";

    /// GitHub username with `@` prefix.
    pub const HANDLE: &str = "@ehsan18t";

    /// Initials used for the avatar monogram circle.
    pub const INITIALS: &str = "EK";

    /// GitHub profile URL.
    pub const GITHUB_URL: &str = "https://github.com/ehsan18t";

    /// `LinkedIn` profile URL.
    pub const LINKEDIN_URL: &str = "https://linkedin.com/in/ehsan18t";

    /// Telegram profile URL.
    pub const TELEGRAM_URL: &str = "https://t.me/ehsan18t";

    /// Personal website URL.
    pub const WEBSITE_URL: &str = "https://ehsankhan.me";

    /// Bio tag labels displayed as pills beneath the developer handle.
    pub const BIO_TAGS: [&str; 2] = ["Software Engineer", "Open Source Enthusiast"];
}

// ─── Notifications ───────────────────────────────────────────────────────────

/// Balloon notification title variants.
pub mod notification {
    /// Standard (success) notification title.
    pub const TITLE: &str = "MagicX RAM Cleaner";

    /// Warning notification title.
    pub const TITLE_WARNING: &str = "MagicX RAM Cleaner - Warning";

    /// Error notification title.
    pub const TITLE_ERROR: &str = "MagicX RAM Cleaner - Error";
}

// ─── Clean Levels ────────────────────────────────────────────────────────────

/// Display text for the four cleaning levels, shared by GUI and CLI.
pub mod levels {
    /// Gentle level display name.
    pub const GENTLE_NAME: &str = "Gentle";

    /// What Gentle does, in plain words.
    pub const GENTLE_DESC: &str = "Empties the standby cache. Apps keep running untouched.";

    /// Moderate level display name.
    pub const MODERATE_NAME: &str = "Moderate";

    /// What Moderate does, in plain words.
    pub const MODERATE_DESC: &str = "Writes changed pages to disk, then empties the standby cache. Apps keep running untouched.";

    /// Aggressive level display name.
    pub const AGGRESSIVE_NAME: &str = "Aggressive";

    /// What Aggressive does, in plain words.
    pub const AGGRESSIVE_DESC: &str = "Also flushes the file and registry caches and trims every app\u{2019}s \
         memory. Apps may pause briefly while they reload.";

    /// Nuclear level display name.
    pub const NUCLEAR_NAME: &str = "Nuclear";

    /// What Nuclear does, in plain words.
    pub const NUCLEAR_DESC: &str = "Everything in Aggressive, plus page combining and a second pass. Expect a \
         short slowdown.";
}

// ─── GUI ─────────────────────────────────────────────────────────────────────

/// Strings used by the graphical user interface.
pub mod gui {
    /// Window title for the main eframe viewport.
    pub const WINDOW_TITLE: &str = "MagicX RAM Cleaner";

    /// Tooltip of the button that expands or collapses the navigation pane.
    pub const NAV_TOGGLE: &str = "Expand or collapse navigation";

    /// Overview panel strings.
    pub mod overview {
        /// Panel title shown at the top of the page.
        pub const TITLE: &str = "Overview";

        /// Shown until the first memory reading arrives.
        pub const LOADING: &str = "Reading memory\u{2026}";

        /// Memory list name: In use.
        pub const LIST_IN_USE: &str = "In use";

        /// Memory list name: Modified.
        pub const LIST_MODIFIED: &str = "Modified";

        /// Memory list name: Standby.
        pub const LIST_STANDBY: &str = "Standby";

        /// Memory list name: Free.
        pub const LIST_FREE: &str = "Free";

        /// Legend name for free plus standby, when the lists are unknown.
        pub const LIST_AVAILABLE: &str = "Available";

        /// Primary button that runs the selected level.
        pub const BTN_CLEAN: &str = "Clean now";

        /// The primary button while a clean runs.
        pub const BTN_CLEANING: &str = "Cleaning\u{2026}";

        /// Shown instead of an estimate when the memory lists are unknown.
        pub const NO_ESTIMATE: &str = "No estimate: Windows didn\u{2019}t report its memory lists.";

        /// Note under a finished clean that purged standby memory.
        pub const REFILL_NOTE: &str =
            "Windows refills the standby cache as you open files. That\u{2019}s normal.";

        /// Collapsible list of the steps a clean ran.
        pub const DETAILS: &str = "Details";

        /// Quick stat: commit charge.
        pub const STAT_COMMIT: &str = "Commit charge";

        /// Quick stat: process count.
        pub const STAT_PROCESSES: &str = "Processes";

        /// Quick stat: thread count.
        pub const STAT_THREADS: &str = "Threads";
    }

    /// Monitor panel strings.
    pub mod monitor {
        /// Panel title.
        pub const TITLE: &str = "Monitor";

        /// Toggle label.
        pub const LABEL_AUTO_CLEAN: &str = "Auto-Clean";

        /// Status text when the monitor is running.
        pub const STATUS_RUNNING: &str = "Running";

        /// Status text when the monitor is stopped.
        pub const STATUS_STOPPED: &str = "Stopped";

        /// Description below the toggle.
        pub const DESCRIPTION: &str =
            "Automatically cleans memory when usage exceeds the threshold.";

        /// Configuration section header.
        pub const SECTION_CONFIG: &str = "Configuration";

        /// Threshold slider label.
        pub const LABEL_THRESHOLD: &str = "Threshold:";

        /// Cooldown slider label.
        pub const LABEL_COOLDOWN: &str = "Cooldown:";

        /// Clean-level combo label.
        pub const LABEL_CLEAN_LEVEL: &str = "Clean Level:";

        /// Activity log section header.
        pub const SECTION_LOG: &str = "Activity Log";

        /// Activity log clear button.
        pub const BTN_CLEAR: &str = "Clear";

        /// Placeholder when the log is empty.
        pub const EMPTY_LOG: &str = "No activity yet.";
    }

    /// Processes panel strings.
    pub mod processes {
        /// Panel title.
        pub const TITLE: &str = "Processes";

        /// Column: program name.
        pub const COL_PROCESS: &str = "Name";

        /// Column: number of running instances.
        pub const COL_COUNT: &str = "Instances";

        /// Column: private working set.
        pub const COL_MEMORY: &str = "Memory";

        /// Column: peak working set.
        pub const COL_PEAK: &str = "Peak";

        /// Sort column names for the footer, indexed by column.
        pub const COL_NAMES: [&str; 4] = ["name", "instances", "memory", "peak"];

        /// Search box placeholder.
        pub const SEARCH_HINT: &str = "Search programs";

        /// Clear-search button tooltip.
        pub const BTN_CLEAR_SEARCH: &str = "Clear search";

        /// Shown until the first process query completes.
        pub const LOADING: &str = "Reading processes\u{2026}";

        /// Row action that trims a program.
        pub const BTN_TRIM: &str = "Trim";

        /// Trim button tooltip.
        pub const TOOLTIP_TRIM: &str = "Trim every instance of this program. Nothing is lost: it \
             reloads pages as it needs them.";

        /// Shown in the row while a trim runs.
        pub const TRIMMING: &str = "Trimming\u{2026}";
    }

    /// Settings panel strings.
    pub mod settings {
        /// Panel title.
        pub const TITLE: &str = "Settings";

        /// Appearance group heading.
        pub const SECTION_APPEARANCE: &str = "Appearance";

        /// Theme row title.
        pub const LABEL_THEME: &str = "App theme";

        /// Theme row description.
        pub const DESC_THEME: &str = "System follows your Windows setting";

        /// System theme option.
        pub const THEME_SYSTEM: &str = "System";

        /// Light theme option.
        pub const THEME_LIGHT: &str = "Light";

        /// Dark theme option.
        pub const THEME_DARK: &str = "Dark";

        /// Windows integration group heading.
        pub const SECTION_INTEGRATION: &str = "Windows integration";

        /// Tray row title.
        pub const LABEL_MINIMIZE_TO_TRAY: &str = "Minimize to tray on close";

        /// Tray row description.
        pub const DESC_MINIMIZE_TO_TRAY: &str =
            "Closing the window keeps the app running in the notification area";

        /// Autostart row title.
        pub const LABEL_AUTOSTART: &str = "Start with Windows";

        /// Autostart row description.
        pub const DESC_AUTOSTART: &str =
            "Starts when you sign in, through a Task Scheduler logon task";

        /// Context menu row title.
        pub const LABEL_CONTEXT_MENU: &str = "Desktop context menu";

        /// Context menu row description.
        pub const DESC_CONTEXT_MENU: &str =
            "Adds MagicX RAM Cleaner to the right-click menu of the desktop and folders";

        /// Context menu status: installed.
        pub const STATUS_INSTALLED: &str = "Installed";

        /// Context menu status: not installed.
        pub const STATUS_NOT_INSTALLED: &str = "Not installed";

        /// Button that installs the context menu.
        pub const BTN_INSTALL: &str = "Install";

        /// Button that removes the context menu.
        pub const BTN_REMOVE: &str = "Remove";

        /// Install button tooltip.
        pub const TOOLTIP_INSTALL: &str = "Add the context menu entries to the Windows registry";

        /// Remove button tooltip.
        pub const TOOLTIP_REMOVE: &str =
            "Remove the context menu entries from the Windows registry";

        /// Backup group heading.
        pub const SECTION_BACKUP: &str = "Backup";

        /// Backup row title.
        pub const LABEL_BACKUP: &str = "Settings file";

        /// Backup row description.
        pub const DESC_BACKUP: &str = "Save your settings to a file, or load them from one";

        /// Export button.
        pub const BTN_EXPORT: &str = "Export";

        /// Import button.
        pub const BTN_IMPORT: &str = "Import";

        /// Export button tooltip.
        pub const TOOLTIP_EXPORT: &str = "Save all settings to a JSON file";

        /// Import button tooltip.
        pub const TOOLTIP_IMPORT: &str = "Load settings from a JSON file";

        /// Confirmation: settings imported.
        pub const MSG_IMPORT_OK: &str = "Settings imported";

        /// Confirmation: context menu installed.
        pub const MSG_CTX_INSTALLED: &str = "Context menu installed";

        /// Confirmation: context menu removed.
        pub const MSG_CTX_REMOVED: &str = "Context menu removed";

        /// Confirmation: autostart turned on.
        pub const MSG_AUTOSTART_ON: &str = "MagicX RAM Cleaner will start when you sign in";

        /// Confirmation: autostart turned off.
        pub const MSG_AUTOSTART_OFF: &str =
            "MagicX RAM Cleaner won\u{2019}t start when you sign in";
    }

    /// About panel strings.
    pub mod about {
        /// Panel title.
        pub const TITLE: &str = "About";

        /// Developer group heading.
        pub const SECTION_DEVELOPER: &str = "Developer";

        /// Project group heading.
        pub const SECTION_PROJECT: &str = "Project";

        /// Button that opens the source repository.
        pub const BTN_VIEW_GITHUB: &str = "View on GitHub";

        /// Row: technology.
        pub const ROW_TECHNOLOGY: &str = "Technology";

        /// Technology value.
        pub const VALUE_TECHNOLOGY: &str = "Rust, 2024 edition";

        /// Row: platform.
        pub const ROW_PLATFORM: &str = "Platform";

        /// Platform value.
        pub const VALUE_PLATFORM: &str = "Windows x86-64";

        /// Row: repository.
        pub const ROW_REPOSITORY: &str = "Repository";

        /// Row: license.
        pub const ROW_LICENSE: &str = "License";

        /// License value.
        pub const VALUE_LICENSE: &str = "MIT";

        /// Row: open source.
        pub const ROW_OPEN_SOURCE: &str = "Open source";

        /// Open source row description.
        pub const DESC_OPEN_SOURCE: &str =
            "Contributions, bug reports and feature requests are welcome.";

        /// Social link: GitHub.
        pub const SOCIAL_GITHUB: &str = "GitHub";

        /// Social link: `LinkedIn`.
        pub const SOCIAL_LINKEDIN: &str = "LinkedIn";

        /// Social link: Telegram.
        pub const SOCIAL_TELEGRAM: &str = "Telegram";

        /// Social link: personal website.
        pub const SOCIAL_WEBSITE: &str = "Website";
    }

    /// Persistence / file dialog strings.
    pub mod persistence {
        /// Save dialog title for exporting settings.
        pub const EXPORT_TITLE: &str = "Export Settings - MagicX RAM Cleaner";

        /// Open dialog title for importing settings.
        pub const IMPORT_TITLE: &str = "Import Settings - MagicX RAM Cleaner";
    }
}

// ─── System Tray ─────────────────────────────────────────────────────────────

/// Tray icon menu labels.
pub mod tray {
    /// Tray icon hover tooltip.
    pub const TOOLTIP: &str = "MagicX RAM Cleaner";

    /// "Open" menu item.
    pub const OPEN: &str = "Open MagicX RAM Cleaner";

    /// "Quit" menu item.
    pub const QUIT: &str = "Quit";

    /// "Clean RAM" submenu title.
    pub const SUBMENU_CLEAN: &str = "Clean RAM";

    /// Sidebar / tray navigation labels (must match panel names).
    pub const NAV_OVERVIEW: &str = "Overview";

    /// Monitor navigation label.
    pub const NAV_MONITOR: &str = "Monitor";

    /// Processes navigation label.
    pub const NAV_PROCESSES: &str = "Processes";

    /// Settings navigation label.
    pub const NAV_SETTINGS: &str = "Settings";
}

// ─── Desktop Context Menu ────────────────────────────────────────────────────

/// Desktop right-click context menu entry labels.
pub mod context_menu {
    /// Root cascading menu display name.
    pub const ROOT_LABEL: &str = "MagicX RAM Cleaner";

    /// Quick-clean entry label.
    pub const QUICK_CLEAN: &str = "Quick Clean";

    /// Standard-clean entry label.
    pub const STANDARD_CLEAN: &str = "Standard Clean";

    /// Deep-clean entry label.
    pub const DEEP_CLEAN: &str = "Deep Clean";

    /// Purge standby entry label.
    pub const PURGE_STANDBY: &str = "Purge Standby List";

    /// Memory status entry label.
    pub const MEMORY_STATUS: &str = "Memory Status";
}

// ─── CLI Display ─────────────────────────────────────────────────────────────

/// Strings used by the CLI terminal display and monitor.
pub mod cli {
    /// Pause prompt for standalone console mode.
    pub const PAUSE_PROMPT: &str = "Press Enter to exit...";

    /// Box-drawn status report header.
    pub const STATUS_HEADER: &str = "MagicX RAM Cleaner - System Status";

    /// Physical memory section header.
    pub const SECTION_PHYSICAL: &str = "Physical Memory";

    /// Memory page lists section header.
    pub const SECTION_PAGE_LISTS: &str = "Memory Page Lists";

    /// Standby list section header.
    pub const SECTION_STANDBY: &str = "Standby List (by priority)";

    /// File system cache section header.
    pub const SECTION_FILE_CACHE: &str = "File System Cache";

    /// Commit charge section header.
    pub const SECTION_COMMIT: &str = "Commit Charge";

    /// Page file section header.
    pub const SECTION_PAGE_FILE: &str = "Commit Limit (RAM + Page File)";

    /// Kernel memory pools section header.
    pub const SECTION_KERNEL: &str = "Kernel Memory Pools";

    /// System counters section header.
    pub const SECTION_SYSTEM: &str = "System Counters";

    /// Top processes section header.
    pub const SECTION_TOP_PROCESSES: &str = "Top Processes by Memory";

    /// Cleaning summary section header.
    pub const SECTION_CLEAN_SUMMARY: &str = "Cleaning Summary";

    /// Before/after comparison section header.
    pub const SECTION_BEFORE_AFTER: &str = "Memory Before/After";

    /// Dry-run footer instruction.
    pub const DRY_RUN_FOOTER: &str = "No operations were executed. Remove --dry-run to clean.";

    /// CLI monitor strings.
    pub mod monitor {
        /// Displayed when the monitor loop starts.
        pub const STARTED: &str = "MagicX RAM Monitor started";

        /// Hint shown alongside the start message.
        pub const CTRL_C_HINT: &str = "Press Ctrl+C to stop.";

        /// Displayed when the monitor loop exits.
        pub const STOPPED: &str = "Monitor stopped.";
    }
}
