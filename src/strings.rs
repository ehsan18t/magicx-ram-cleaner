//! # Centralised User-Facing Text
//!
//! Every display string shown to the user in the GUI or CLI lives here for
//! consistent, single-source management.  Organised by domain in nested
//! modules so call sites read naturally:
//!
//! ```text
//! strings::APP_NAME
//! strings::gui::dashboard::TITLE
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
//! * CLI help-text constants with embedded ANSI codes (`cli.rs`)
//! * NT status code translations (`ntapi.rs`)
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

/// Three-letter monogram used in sidebar badges and hero cards.
pub const MONOGRAM: &str = "MGX";

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
    pub const TITLE_WARNING: &str = "MagicX RAM Cleaner \u{2014} Warning";

    /// Error notification title.
    pub const TITLE_ERROR: &str = "MagicX RAM Cleaner \u{2014} Error";
}

// ─── Clean Levels ────────────────────────────────────────────────────────────

/// Display text for the four cleaning levels, shared by GUI and CLI.
pub mod levels {
    /// Gentle level display name.
    pub const GENTLE_NAME: &str = "Gentle";

    /// Gentle one-line summary.
    pub const GENTLE_SHORT: &str = "Purge all standby pages";

    /// Gentle full description for tooltips.
    pub const GENTLE_DETAIL: &str = "Purges all standby pages (priorities 0\u{2013}7). Standby pages are already \
         outside every process\u{2019}s working set \u{2014} completely safe to run at any time.";

    /// Moderate level display name.
    pub const MODERATE_NAME: &str = "Moderate";

    /// Moderate one-line summary.
    pub const MODERATE_SHORT: &str = "Modified pages + all standby";

    /// Moderate full description for tooltips.
    pub const MODERATE_DETAIL: &str = "Flushes modified pages to disk, then purges all standby pages. No process \
         working sets are touched \u{2014} running apps are unaffected, but expect a brief \
         I/O spike.";

    /// Aggressive level display name.
    pub const AGGRESSIVE_NAME: &str = "Aggressive";

    /// Aggressive one-line summary.
    pub const AGGRESSIVE_SHORT: &str = "Full clean \u{2014} cache, registry, working sets, standby";

    /// Aggressive full description for tooltips.
    pub const AGGRESSIVE_DETAIL: &str = "File cache flush \u{2192} registry flush \u{2192} empty working sets \u{2192} \
         flush modified \u{2192} purge all standby. Frees maximum RAM but may cause a \
         brief I/O spike as apps re-fault pages.";

    /// Nuclear level display name.
    pub const NUCLEAR_NAME: &str = "Nuclear";

    /// Nuclear one-line summary.
    pub const NUCLEAR_SHORT: &str = "Everything + combining + 2nd pass";

    /// Nuclear full description for tooltips.
    pub const NUCLEAR_DETAIL: &str = "All of Aggressive plus memory page combining (dedup) and a second flush+purge \
         pass to catch pages modified during combining. Use when you need every last \
         byte freed.";
}

// ─── GUI ─────────────────────────────────────────────────────────────────────

/// Strings used by the graphical user interface.
pub mod gui {
    /// Window title for the main eframe viewport.
    pub const WINDOW_TITLE: &str = "MagicX RAM Cleaner";

    /// Dashboard panel strings.
    pub mod dashboard {
        /// Panel title shown at the top of the page.
        pub const TITLE: &str = "Dashboard";

        /// Spinner text while the first snapshot loads.
        pub const LOADING: &str = "Loading memory information...";

        /// Section header above the clean buttons.
        pub const SECTION_CLEAN: &str = "Clean Memory";

        /// Progress indicator during a clean.
        pub const CLEANING: &str = "Cleaning in progress...";

        /// Stat label: total physical RAM.
        pub const LABEL_TOTAL_RAM: &str = "Total RAM";

        /// Stat label: page file usage.
        pub const LABEL_PAGE_FILE: &str = "Page File";

        /// Stat label: system thread count.
        pub const LABEL_THREADS: &str = "Threads";

        /// Stat label: bytes freed by a clean.
        pub const LABEL_FREED: &str = "Freed";

        /// Stat label: memory usage percentage.
        pub const LABEL_USAGE: &str = "Usage";

        /// Stat label: available memory.
        pub const LABEL_AVAILABLE: &str = "Available";
    }

    /// Monitor panel strings.
    pub mod monitor {
        /// Panel title.
        pub const TITLE: &str = "Memory Monitor";

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
        pub const TITLE: &str = "Top Processes";

        /// Table column: process name.
        pub const COL_PROCESS: &str = "Process";

        /// Table column: instance count.
        pub const COL_COUNT: &str = "Count";

        /// Table column: private working set.
        pub const COL_MEMORY: &str = "Memory";

        /// Table column: peak working set.
        pub const COL_PEAK: &str = "Peak";

        /// Toolbar label before the top-N slider.
        pub const LABEL_SHOW_TOP: &str = "Show top";

        /// Search box hint text.
        pub const SEARCH_HINT: &str = "search\u{2026}";

        /// Clear-search button tooltip.
        pub const BTN_CLEAR_SEARCH: &str = "Clear search";

        /// Sort column human-readable names (indexed by column).
        pub const COL_NAMES: [&str; 4] = ["name", "count", "memory", "peak"];
    }

    /// Settings panel strings.
    pub mod settings {
        /// Panel title.
        pub const TITLE: &str = "Settings";

        /// Appearance section header.
        pub const SECTION_APPEARANCE: &str = "Appearance";

        /// Theme label.
        pub const LABEL_THEME: &str = "Theme:";

        /// Dark theme button.
        pub const THEME_DARK: &str = "Dark";

        /// Light theme button.
        pub const THEME_LIGHT: &str = "Light";

        /// Integration section header.
        pub const SECTION_INTEGRATION: &str = "Integration";

        /// Tray checkbox label.
        pub const LABEL_MINIMIZE_TO_TRAY: &str = "Minimize to Tray on Close";

        /// Tray checkbox description.
        pub const DESC_MINIMIZE_TO_TRAY: &str =
            "Clicking \u{00d7} hides to the notification area instead of quitting";

        /// Autostart checkbox label.
        pub const LABEL_AUTOSTART: &str = "Launch at Windows Startup";

        /// Autostart checkbox description.
        pub const DESC_AUTOSTART: &str = "Registers in HKCU\\Run for the current user";

        /// Desktop context menu section header.
        pub const SECTION_CONTEXT_MENU: &str = "Desktop Context Menu";

        /// Context menu explanatory text.
        pub const DESC_CONTEXT_MENU: &str = "Adds a \u{201c}MagicX RAM Cleaner\u{201d} submenu when right-clicking \
             the Desktop or any folder background.";

        /// Context menu installed badge.
        pub const STATUS_INSTALLED: &str = "\u{25cf} Installed";

        /// Context menu not-installed badge.
        pub const STATUS_NOT_INSTALLED: &str = "\u{25cb} Not installed";

        /// Install button tooltip.
        pub const TOOLTIP_INSTALL: &str = "Register context menu entries in the Windows registry";

        /// Remove button tooltip.
        pub const TOOLTIP_REMOVE: &str = "Remove context menu entries from the Windows registry";

        /// Preferences section header.
        pub const SECTION_PREFERENCES: &str = "Preferences";

        /// Tooltip-visibility checkbox label.
        pub const LABEL_TOOLTIPS: &str = "Show clean-level tooltips on hover";

        /// Backup section header.
        pub const SECTION_BACKUP: &str = "Backup & Restore";

        /// Backup section description.
        pub const DESC_BACKUP: &str =
            "Export your settings to a JSON file or restore from a previous backup.";

        /// Export button tooltip.
        pub const TOOLTIP_EXPORT: &str = "Save all settings to a JSON file";

        /// Import button tooltip.
        pub const TOOLTIP_IMPORT: &str = "Load settings from a JSON backup file";

        /// Success: settings imported.
        pub const MSG_IMPORT_OK: &str = "Settings imported successfully";

        /// Success: context menu installed.
        pub const MSG_CTX_INSTALLED: &str = "Context menu installed successfully";

        /// Success: context menu removed.
        pub const MSG_CTX_REMOVED: &str = "Context menu removed successfully";
    }

    /// About panel strings.
    pub mod about {
        /// Panel title.
        pub const TITLE: &str = "About";

        /// Project & License section header.
        pub const SECTION_PROJECT: &str = "Project & License";

        /// Developer section header.
        pub const SECTION_DEVELOPER: &str = "Developer";

        /// "View on GitHub" button label.
        pub const BTN_VIEW_GITHUB: &str = "View on GitHub";

        /// Platform metadata chip.
        pub const CHIP_PLATFORM: &str = "Windows x86-64";

        /// License metadata chip.
        pub const CHIP_LICENSE: &str = "MIT License";

        /// Info row: technology label.
        pub const ROW_TECHNOLOGY: &str = "Technology";

        /// Info row: technology value.
        pub const VALUE_RUST: &str = "Rust";

        /// Info row: edition value.
        pub const VALUE_EDITION: &str = "2024 Edition";

        /// Info row: platform label.
        pub const ROW_PLATFORM: &str = "Platform";

        /// Info row: platform value.
        pub const VALUE_PLATFORM: &str = "Windows x86-64";

        /// Info row: repository label.
        pub const ROW_REPOSITORY: &str = "Repository";

        /// Info row: license label.
        pub const ROW_LICENSE: &str = "License";

        /// Info row: license value.
        pub const VALUE_LICENSE: &str = "MIT";

        /// Contribution banner heading.
        pub const BANNER_HEADING: &str = "Open Source";

        /// Contribution banner body.
        pub const BANNER_BODY: &str = "Free and open-source software. Contributions, \
             bug reports, and feature requests are welcome!";

        /// Social link labels.
        pub const SOCIAL_GITHUB: &str = "GitHub";

        /// Social link: `LinkedIn`.
        pub const SOCIAL_LINKEDIN: &str = "LinkedIn";

        /// Social link: Telegram.
        pub const SOCIAL_TELEGRAM: &str = "Telegram";

        /// Social link: personal website.
        pub const SOCIAL_WEBSITE: &str = "Website";
    }

    /// Memory overview widget strings.
    pub mod widgets {
        /// Label next to the large percentage heading.
        pub const LABEL_MEMORY_USED: &str = "Memory Used";

        /// Stat row: available memory.
        pub const LABEL_AVAILABLE: &str = "Available";

        /// Stat row: used memory.
        pub const LABEL_USED: &str = "Used";

        /// Stat row: commit percentage.
        pub const LABEL_COMMIT: &str = "Commit";

        /// Stat row: process count.
        pub const LABEL_PROCESSES: &str = "Processes";
    }

    /// Persistence / file dialog strings.
    pub mod persistence {
        /// Save dialog title for exporting settings.
        pub const EXPORT_TITLE: &str = "Export Settings \u{2014} MagicX RAM Cleaner";

        /// Open dialog title for importing settings.
        pub const IMPORT_TITLE: &str = "Import Settings \u{2014} MagicX RAM Cleaner";
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
    pub const NAV_DASHBOARD: &str = "Dashboard";

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
    pub const STATUS_HEADER: &str = "MagicX RAM Cleaner \u{2014} System Status";

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
    pub const SECTION_PAGE_FILE: &str = "Page File";

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
