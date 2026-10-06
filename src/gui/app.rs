//! # GUI Application State
//!
//! Core [`MagicXApp`] struct implementing [`eframe::App`], background thread
//! management, and the main layout (sidebar + panel routing).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use anyhow::Context;

use eframe::egui;
use egui::{FontFamily, FontId, TextStyle};
use egui_phosphor::regular as ph;

use serde::{Deserialize, Serialize};

use crate::cleaner::{self, CleanLevel, SmartCleanResult};
use crate::stats::{self, MemorySnapshot, ProcessMemoryInfo};
use crate::strings;

use super::{panels, theme, tray};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Maximum number of lines kept in the monitor activity log.
const MONITOR_LOG_CAPACITY: usize = 500;

/// Upper bound for the auto-clean cooldown backoff multiplier.
const MAX_COOLDOWN_BACKOFF: u32 = 8;

/// Valid range of the monitor threshold slider (percent).
const THRESHOLD_RANGE: std::ops::RangeInclusive<u32> = 50..=99;

/// Valid range of the monitor cooldown slider (seconds).
const COOLDOWN_RANGE_SECS: std::ops::RangeInclusive<u64> = 10..=300;

/// Valid range of the "Show top" process count slider.
const TOP_PROCESSES_RANGE: std::ops::RangeInclusive<usize> = 5..=50;

/// How often the background stats thread captures a snapshot (ms).
const STATS_POLL_INTERVAL_MS: u64 = 1000;

/// How often the process list refreshes (seconds).
const PROCESS_REFRESH_SECS: u64 = 5;

/// Default monitor threshold percentage.
const DEFAULT_THRESHOLD: u32 = 80;

/// Default monitor cooldown in seconds.
const DEFAULT_COOLDOWN_SECS: u64 = 30;

/// Default auto-clean level.
const DEFAULT_CLEAN_LEVEL: CleanLevel = CleanLevel::Aggressive;

/// Default number of top processes to display.
const DEFAULT_TOP_PROCESSES: usize = 20;

// ─── Types ───────────────────────────────────────────────────────────────────

/// Which panel is currently shown in the main content area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// Memory overview, cleaning, and quick stats.
    Dashboard,
    /// Continuous monitoring with auto-clean.
    Monitor,
    /// Top processes by memory usage.
    Processes,
    /// User preferences.
    Settings,
    /// Application information, credits, and links.
    About,
}

/// Persistent user settings.
///
/// Contains several independent boolean preferences; no meaningful two-variant
/// enum reduction exists without obscuring what each field controls.
///
/// Fields missing from a saved file take their [`Default`] values.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GuiSettings {
    /// Minimize to the system tray when the close button is clicked.
    ///
    /// When enabled, clicking ✕ hides the window to the notification area
    /// rather than quitting. The tray icon provides "Open" and "Quit" actions.
    #[serde(alias = "tray_enabled")]
    pub minimize_to_tray: bool,
    /// Launch automatically at Windows startup (current user only).
    ///
    /// Creates (or removes) a Task Scheduler logon task.
    pub auto_start: bool,
    /// Auto-clean threshold percentage (50 to 99).
    pub monitor_threshold: u32,
    /// Cooldown between auto-cleans (10 to 300 seconds).
    pub monitor_cooldown_secs: u64,
    /// Default cleaning level.
    pub default_clean_level: CleanLevel,
    /// Number of top processes to show.
    pub top_process_count: usize,
    /// Theme preference (`true` = dark).
    pub dark_mode: bool,
    /// Show tooltip with level details on circle hover (`true` = enabled).
    pub show_level_tooltips: bool,
    /// Whether auto-clean monitoring is enabled.
    ///
    /// Persisted so the monitor resumes automatically when the app is
    /// restarted.
    pub auto_clean_enabled: bool,
}

impl GuiSettings {
    /// Clamp numeric settings to the ranges their sliders allow.
    ///
    /// Applied after loading or importing a file, which may have been edited
    /// by hand or written by another version.
    pub fn sanitize(&mut self) {
        self.monitor_threshold = self
            .monitor_threshold
            .clamp(*THRESHOLD_RANGE.start(), *THRESHOLD_RANGE.end());
        self.monitor_cooldown_secs = self
            .monitor_cooldown_secs
            .clamp(*COOLDOWN_RANGE_SECS.start(), *COOLDOWN_RANGE_SECS.end());
        self.top_process_count = self
            .top_process_count
            .clamp(*TOP_PROCESSES_RANGE.start(), *TOP_PROCESSES_RANGE.end());
    }
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            minimize_to_tray: false,
            auto_start: false,
            monitor_threshold: DEFAULT_THRESHOLD,
            monitor_cooldown_secs: DEFAULT_COOLDOWN_SECS,
            default_clean_level: DEFAULT_CLEAN_LEVEL,
            top_process_count: DEFAULT_TOP_PROCESSES,
            dark_mode: true,
            show_level_tooltips: true,
            auto_clean_enabled: false,
        }
    }
}

/// Result of a background cleaning operation sent back to the UI thread.
pub struct CleanResultMsg {
    /// The cleaning result (or error string).
    pub result: std::result::Result<SmartCleanResult, String>,
    /// Which level was requested.
    pub level: CleanLevel,
    /// `true` when the clean was started by monitor auto-clean.
    pub auto: bool,
}

// ─── Application State ───────────────────────────────────────────────────────

/// The main GUI application state.
// Seven independent boolean flags that represent unrelated on/off states.
// Collapsing them into enums would hurt readability without real benefit.
#[allow(clippy::struct_excessive_bools)]
pub struct MagicXApp {
    /// Currently active sidebar panel.
    pub active_panel: Panel,

    /// Latest memory snapshot (updated by background thread).
    pub latest_snapshot: Arc<Mutex<Option<MemorySnapshot>>>,

    /// Background stats thread shutdown signal.
    stats_running: Arc<AtomicBool>,

    /// Shared flag: `true` when the UI needs periodic repaints.
    ///
    /// Set every frame in [`logic()`] based on window visibility.
    /// The background stats thread reads this to decide whether to call
    /// [`egui::Context::request_repaint`].  When `false` the app goes
    /// truly idle (zero CPU / GPU) unless the monitor is collecting data.
    needs_repaint: Arc<AtomicBool>,

    /// Shared flag: `true` when the stats thread should capture snapshots
    /// even if the window is not visible (e.g. auto-clean monitoring).
    ///
    /// When this is `true` but `needs_repaint` is `false`, the stats
    /// thread captures data but does **not** request repaints, avoiding
    /// unnecessary UI wake-ups while hidden/minimized.
    needs_capture: Arc<AtomicBool>,

    /// Channel for receiving cleaning results from worker threads.
    clean_rx: Receiver<CleanResultMsg>,

    /// Channel sender cloned into worker threads.
    clean_tx: Sender<CleanResultMsg>,

    /// Whether a cleaning operation is currently in progress.
    pub cleaning_in_progress: bool,

    /// Last cleaning result (for display).
    pub last_clean_result: Option<CleanResultMsg>,

    /// Top processes list (refreshed periodically).
    ///
    /// `None` until the first background query completes, so the panel can
    /// show a loading state instead of an empty table.
    pub top_processes: Arc<Mutex<Option<Vec<ProcessMemoryInfo>>>>,

    /// Last time processes were refreshed.
    last_process_refresh: Instant,

    /// `true` while a process-list refresh thread is running, so refreshes
    /// never overlap.
    process_refresh_in_flight: Arc<AtomicBool>,

    /// Whether monitoring auto-clean is active.
    pub monitor_active: bool,

    /// Last time an auto-clean finished (for cooldown).
    last_auto_clean: Option<Instant>,

    /// Cooldown multiplier for the next auto-clean (1, 2, 4 or 8).
    ///
    /// Doubles each time an auto-clean finishes with memory load still at or
    /// above the threshold, and resets to 1 once load drops below it, so a
    /// futile clean is not repeated back to back.
    auto_clean_backoff: u32,

    /// Last time a periodic status line was appended to the monitor log.
    ///
    /// Used to throttle heartbeat messages so the log is not flooded
    /// while the monitor is idle (memory below threshold).
    last_monitor_status_log: Option<Instant>,

    /// Previous frame's `monitor_active` state - used to detect
    /// start/stop transitions and log them once.
    prev_monitor_active: bool,

    /// Monitor log messages, oldest first, capped at
    /// [`MONITOR_LOG_CAPACITY`] lines.
    pub monitor_log: VecDeque<String>,

    /// User settings.
    pub settings: GuiSettings,

    /// Shadow copy of settings used to detect changes and trigger auto-save.
    settings_snapshot: GuiSettings,

    /// `true` when settings changed but have not been written to disk yet.
    ///
    /// Saving is deferred while a widget is being dragged (e.g. a slider) so
    /// the file is not rewritten on every frame.
    settings_dirty: bool,

    /// Process sort column (0=name, 1=count, 2=memory, 3=peak).
    pub process_sort_col: usize,

    /// Process sort ascending.
    pub process_sort_asc: bool,

    /// Real-time search / filter text for the processes panel.
    pub process_search: String,

    /// Tracks the last `dark_mode` value written to the egui context so
    /// the theme is only switched on the frame the setting changes.
    last_applied_dark: bool,

    /// Whether the window has been revealed (for anti-flash).
    window_revealed: bool,

    /// Transient feedback shown in the Settings panel after an import/export action.
    ///
    /// Tuple of `(message, is_error, shown_at)`. The Settings panel auto-dismisses
    /// this after 8 seconds.
    pub settings_status: Option<(String, bool, std::time::Instant)>,

    /// System-tray icon handle.
    ///
    /// `Some` while [`GuiSettings::minimize_to_tray`] is enabled; `None`
    /// otherwise.  Dropping this value removes the tray icon from the
    /// notification area.
    tray_handle: Option<tray::TrayHandle>,

    /// Error from the last failed attempt to create the tray icon.
    ///
    /// Shown in the Settings panel; `None` when the icon was created or is
    /// not wanted.
    pub tray_error: Option<String>,

    /// Whether the window is currently hidden to the system tray.
    hidden_to_tray: bool,

    /// Timestamp when `hidden_to_tray` was last set to `true` via the
    /// close intercept.  Used to suppress the external-restore detection
    /// for a short grace period so that the cloaking calls have time to
    /// settle before we poll `IsIconic`.
    hide_requested_at: Option<Instant>,

    /// Set to `true` when the user selects "Quit" from the tray menu.
    ///
    /// Allows the close-intercept logic to distinguish between a user quitting
    /// via tray and a regular window close (which should minimize instead).
    quit_requested: bool,

    /// Whether the Desktop context menu is currently installed in the registry.
    ///
    /// Cached from [`crate::context_menu::is_installed`] at startup and updated
    /// by the Settings panel after each install or uninstall operation.
    pub context_menu_installed: bool,

    /// Win32 `HWND` of the main application window, stored as `isize` for
    /// `Send`-safe access from background threads.
    ///
    /// Used by the tray-watcher thread to post a synthetic `WM_PAINT` message
    /// that wakes eframe's event loop even while the window is invisible.
    hwnd: isize,
}

impl MagicXApp {
    /// Create the app, spawn background threads, and apply the initial theme.
    ///
    /// # Errors
    ///
    /// Returns an error if the background stats thread cannot be spawned.
    pub fn new(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        // Register Phosphor icon font so all icon glyphs render correctly.
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        // Load persisted settings before applying the theme so the window
        // starts in the user's preferred mode without a one-frame flash.
        let (settings, settings_status) = load_settings_and_sync_autostart();

        // Register and configure both themes, then activate the saved one.
        configure_themes(&cc.egui_ctx, settings.dark_mode);

        let (clean_tx, clean_rx) = mpsc::channel();
        let latest_snapshot = Arc::new(Mutex::new(None));
        let stats_running = Arc::new(AtomicBool::new(true));
        let needs_repaint = Arc::new(AtomicBool::new(true));
        let needs_capture = Arc::new(AtomicBool::new(true));
        let top_processes = Arc::new(Mutex::new(None));

        // Spawn background stats collection thread
        {
            let snapshot_ref = Arc::clone(&latest_snapshot);
            let running_ref = Arc::clone(&stats_running);
            let repaint_ref = Arc::clone(&needs_repaint);
            let capture_ref = Arc::clone(&needs_capture);
            let ctx = cc.egui_ctx.clone();

            drop(
                std::thread::Builder::new()
                    .name("gui-stats".into())
                    .spawn(move || {
                        stats_thread(
                            &snapshot_ref,
                            &running_ref,
                            &repaint_ref,
                            &capture_ref,
                            &ctx,
                        );
                    })
                    .context("failed to spawn stats thread")?,
            );
        }

        // Capture dark_mode before settings is moved into Self.
        let initial_dark_mode = settings.dark_mode;

        // Take our own HWND from eframe so the tray-watcher thread can post a
        // synthetic WM_PAINT message that wakes eframe even when WS_VISIBLE
        // is cleared. A title lookup (FindWindowW) could match an unrelated
        // window with the same title, such as an Explorer folder.
        let hwnd = window_hwnd(cc);

        // Force Windows dark mode at the process level so native menus
        // and the title bar match the user's in-app theme from the start.
        crate::platform::window::set_process_dark_mode(initial_dark_mode);
        crate::platform::window::set_title_bar_dark_mode(hwnd, initial_dark_mode);

        // Initialize tray icon if minimize-to-tray was previously enabled.
        // Pass the egui context and HWND so the watcher thread can call
        // request_repaint() to wake the event loop on tray events.
        let (tray_handle, tray_error) = if settings.minimize_to_tray {
            match tray::TrayHandle::new(cc.egui_ctx.clone(), hwnd, initial_dark_mode) {
                Ok(handle) => (Some(handle), None),
                Err(e) => (None, Some(e)),
            }
        } else {
            (None, None)
        };

        Ok(Self {
            active_panel: Panel::Dashboard,
            latest_snapshot,
            stats_running,
            needs_repaint,
            needs_capture,
            clean_rx,
            clean_tx,
            cleaning_in_progress: false,
            last_clean_result: None,
            top_processes,
            last_process_refresh: Instant::now()
                .checked_sub(Duration::from_secs(PROCESS_REFRESH_SECS + 1))
                .unwrap_or_else(Instant::now),
            process_refresh_in_flight: Arc::new(AtomicBool::new(false)),
            monitor_active: settings.auto_clean_enabled,
            last_auto_clean: None,
            auto_clean_backoff: 1,
            last_monitor_status_log: None,
            prev_monitor_active: false,
            monitor_log: VecDeque::with_capacity(MONITOR_LOG_CAPACITY),
            settings_snapshot: settings.clone(),
            settings_dirty: false,
            settings,
            process_sort_col: 2,
            process_sort_asc: false,
            process_search: String::new(),
            last_applied_dark: initial_dark_mode,
            window_revealed: false,
            settings_status,
            tray_handle,
            tray_error,
            hidden_to_tray: false,
            hide_requested_at: None,
            quit_requested: false,
            context_menu_installed: crate::context_menu::is_installed(),
            hwnd,
        })
    }

    /// Start a manual cleaning operation on a background thread.
    pub fn start_clean(&mut self, level: CleanLevel) {
        self.spawn_clean(level, false);
    }

    /// Start a cleaning operation on a background thread.
    ///
    /// `auto` marks cleans triggered by the monitor so the result is logged
    /// and the cooldown applied only for those.
    fn spawn_clean(&mut self, level: CleanLevel, auto: bool) {
        if self.cleaning_in_progress {
            return;
        }
        self.cleaning_in_progress = true;
        self.last_clean_result = None;

        let tx = self.clean_tx.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("gui-clean".into())
            .spawn(move || {
                // Catch a panic so a result is always sent; otherwise
                // `cleaning_in_progress` would stay set forever. This only
                // matters in dev builds: the release profile uses
                // `panic = "abort"`, where a panic ends the process instead.
                let result = std::panic::catch_unwind(|| {
                    cleaner::smart_clean(level, false, &[]).map_err(|e| format!("{e:#}"))
                })
                .unwrap_or_else(|_| Err("clean worker panicked".to_owned()));
                drop(tx.send(CleanResultMsg {
                    result,
                    level,
                    auto,
                }));
            })
        {
            drop(self.clean_tx.send(CleanResultMsg {
                result: Err(format!("failed to spawn clean thread: {e}")),
                level,
                auto,
            }));
        }
    }

    /// Append a line to the monitor activity log, dropping the oldest line
    /// once [`MONITOR_LOG_CAPACITY`] is reached.
    fn push_monitor_log(&mut self, msg: String) {
        if self.monitor_log.len() >= MONITOR_LOG_CAPACITY {
            self.monitor_log.pop_front();
        }
        self.monitor_log.push_back(msg);
    }

    /// Poll for completed cleaning results.
    ///
    /// Auto-clean results are logged to the activity log exactly once (here,
    /// not in `logic()`) and start the cooldown, with backoff when the clean
    /// did not bring memory load below the threshold.
    fn poll_clean_results(&mut self) {
        let Ok(msg) = self.clean_rx.try_recv() else {
            return;
        };
        self.cleaning_in_progress = false;

        if msg.auto {
            let log_msg = match &msg.result {
                Ok(r) => format!(
                    "Auto-clean complete: freed {}",
                    stats::format_bytes(r.reclaimed_bytes().max(0) as u64),
                ),
                Err(e) => format!("Auto-clean failed: {e}"),
            };
            self.push_monitor_log(log_msg);

            // Start the cooldown when the clean finishes, not when it starts,
            // so a long clean does not eat into the cooldown.
            self.last_auto_clean = Some(Instant::now());

            let load_after = msg.result.as_ref().map_or_else(
                |_| {
                    self.latest_snapshot
                        .lock()
                        .ok()
                        .and_then(|s| s.as_ref().map(|s| s.memory_load_percent))
                },
                |r| Some(r.overall_after.memory_load_percent),
            );
            if load_after.is_some_and(|load| load >= self.settings.monitor_threshold) {
                self.auto_clean_backoff = (self.auto_clean_backoff * 2).min(MAX_COOLDOWN_BACKOFF);
                let msg = format!(
                    "Memory load still at or above {}%; next auto-clean in {}s.",
                    self.settings.monitor_threshold,
                    self.effective_cooldown().as_secs(),
                );
                self.push_monitor_log(msg);
            } else {
                self.auto_clean_backoff = 1;
            }
        }

        self.last_clean_result = Some(msg);
    }

    /// Cooldown before the next auto-clean: the configured cooldown times
    /// the current backoff multiplier.
    fn effective_cooldown(&self) -> Duration {
        Duration::from_secs(
            self.settings
                .monitor_cooldown_secs
                .saturating_mul(u64::from(self.auto_clean_backoff)),
        )
    }

    /// Refresh the process list if enough time has passed and no refresh is
    /// already running.
    fn maybe_refresh_processes(&mut self) {
        if self.last_process_refresh.elapsed() < Duration::from_secs(PROCESS_REFRESH_SECS)
            || self.process_refresh_in_flight.swap(true, Ordering::AcqRel)
        {
            return;
        }
        self.last_process_refresh = Instant::now();
        let procs_ref = Arc::clone(&self.top_processes);
        let in_flight = Arc::clone(&self.process_refresh_in_flight);
        let spawned = std::thread::Builder::new()
            .name("gui-procs".into())
            .spawn(move || {
                if let Ok(procs) = stats::query_all_processes()
                    && let Ok(mut lock) = procs_ref.lock()
                {
                    *lock = Some(procs);
                }
                in_flight.store(false, Ordering::Release);
            });
        if spawned.is_err() {
            self.process_refresh_in_flight
                .store(false, Ordering::Release);
        }
    }

    /// Handle auto-clean in monitor mode.
    ///
    /// Checks the current memory load against the configured threshold and
    /// triggers a clean when exceeded.  Also emits periodic heartbeat
    /// messages to the activity log so the user can see the monitor is
    /// actively checking (every 60 seconds).
    fn handle_monitor_auto_clean(&mut self) {
        if !self.monitor_active || self.cleaning_in_progress {
            return;
        }

        // Check cooldown (scaled by backoff after futile cleans).
        if let Some(last) = self.last_auto_clean
            && last.elapsed() < self.effective_cooldown()
        {
            return;
        }

        // Check threshold
        let load = self
            .latest_snapshot
            .lock()
            .ok()
            .and_then(|s| s.as_ref().map(|s| s.memory_load_percent));

        if let Some(load) = load {
            if load >= self.settings.monitor_threshold {
                let msg = format!(
                    "Memory load {load}% >= threshold {}%, auto-cleaning ({})...",
                    self.settings.monitor_threshold,
                    self.settings.default_clean_level.title_case_name(),
                );
                self.push_monitor_log(msg);
                self.last_monitor_status_log = Some(Instant::now());
                self.spawn_clean(self.settings.default_clean_level, true);
            } else {
                self.auto_clean_backoff = 1;

                // Periodic heartbeat so the user knows the monitor is alive.
                let should_log = self
                    .last_monitor_status_log
                    .is_none_or(|t| t.elapsed() >= Duration::from_secs(60));

                if should_log {
                    self.push_monitor_log(format!(
                        "Checked: memory at {load}%, below threshold {}%. No action needed.",
                        self.settings.monitor_threshold,
                    ));
                    self.last_monitor_status_log = Some(Instant::now());
                }
            }
        }
    }

    /// Poll the tray icon event queues and dispatch every pending [`TrayAction`].
    ///
    /// Called every frame from [`eframe::App::logic`].
    fn poll_tray_events(&mut self, ctx: &egui::Context) {
        while let Some(action) = self.tray_handle.as_ref().and_then(tray::TrayHandle::poll) {
            self.handle_tray_action(ctx, &action);
        }
    }

    /// Apply a single [`TrayAction`] received from the tray watcher thread.
    fn handle_tray_action(&mut self, ctx: &egui::Context, action: &tray::TrayAction) {
        match *action {
            tray::TrayAction::Show => {
                crate::platform::window::uncloak_window(self.hwnd);
                self.hidden_to_tray = false;
            }
            tray::TrayAction::Clean(level) => {
                crate::platform::window::uncloak_window(self.hwnd);
                self.hidden_to_tray = false;
                self.active_panel = Panel::Dashboard;
                self.start_clean(level);
            }
            tray::TrayAction::Navigate(panel) => {
                crate::platform::window::uncloak_window(self.hwnd);
                self.hidden_to_tray = false;
                self.active_panel = panel;
            }
            tray::TrayAction::Quit => {
                self.quit_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Synchronise the tray handle when the user changes the minimise-to-tray
    /// setting.
    ///
    /// Compares live settings against the snapshot so this only acts on the
    /// frame the toggle is flipped - it is a no-op every other frame.
    ///
    /// Autostart registry writes are NOT handled here - they are done
    /// directly by the Settings panel checkbox handler (with error rollback)
    /// and by the import path in `draw_backup`.
    fn sync_integration_settings(&mut self, ctx: &egui::Context) {
        let tray_changed =
            self.settings.minimize_to_tray != self.settings_snapshot.minimize_to_tray;

        // Rebuild the tray handle when the tray toggle changes.
        // Glyph colours match the in-app theme: the process-wide menu
        // theme is forced dark/light via set_process_dark_mode(), so the
        // glyphs must match the app's dark_mode, not the OS theme.
        if tray_changed {
            if self.settings.minimize_to_tray {
                self.rebuild_tray(ctx);
            } else {
                self.tray_handle = None;
                self.tray_error = None;
                // Uncloak if the window was hidden while the setting was on.
                if self.hidden_to_tray {
                    crate::platform::window::uncloak_window(self.hwnd);
                    self.hidden_to_tray = false;
                }
            }
        }
    }

    /// (Re)create the tray icon for the current theme.
    ///
    /// The old handle is dropped first so its icon and watcher thread are
    /// gone before the new ones register. A failure is kept in
    /// [`Self::tray_error`] for the Settings panel.
    fn rebuild_tray(&mut self, ctx: &egui::Context) {
        self.tray_handle = None;
        match tray::TrayHandle::new(ctx.clone(), self.hwnd, self.settings.dark_mode) {
            Ok(handle) => {
                self.tray_handle = Some(handle);
                self.tray_error = None;
            }
            Err(e) => self.tray_error = Some(e),
        }
    }

    /// Run the parts of the UI that are only needed when the window is
    /// actually visible (not minimized, not hidden to tray).
    ///
    /// Splitting this out of [`eframe::App::ui`] keeps both
    /// functions below the `too_many_lines` lint threshold and makes the
    /// visibility gate explicit.
    fn draw_visible_ui(&mut self, ui: &mut egui::Ui) {
        // Refresh processes only when the panel is shown.
        if self.active_panel == Panel::Processes {
            self.maybe_refresh_processes();
        }

        // Switch theme when the user toggles the preference.
        if self.settings.dark_mode != self.last_applied_dark {
            theme::set_active_theme(ui, self.settings.dark_mode);
            self.last_applied_dark = self.settings.dark_mode;

            crate::platform::window::set_process_dark_mode(self.settings.dark_mode);
            crate::platform::window::set_title_bar_dark_mode(self.hwnd, self.settings.dark_mode);

            if self.settings.minimize_to_tray {
                let ctx = ui.ctx().clone();
                self.rebuild_tray(&ctx);
            }
        }

        // Enforce the app's theme preference every visible frame.
        // Eframe's system-theme detection can silently override
        // our set_theme between frames when the OS theme differs.
        let desired = if self.settings.dark_mode {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
        if ui.theme() != desired {
            theme::set_active_theme(ui, self.settings.dark_mode);
        }

        // ── Layout ───────────────────────────────────────────
        draw_sidebar(ui, self);
        draw_main_panel(ui, self);
    }

    /// Save settings to disk after the user changes anything.
    ///
    /// Writes are deferred while the pointer is dragging a widget (e.g. a
    /// slider) so the file is not rewritten every frame; the final value is
    /// saved on the first frame after release, and always on exit.
    fn persist_settings_if_changed(&mut self, ctx: &egui::Context) {
        if self.settings != self.settings_snapshot {
            self.settings_snapshot = self.settings.clone();
            self.settings_dirty = true;
        }
        if self.settings_dirty && !ctx.egui_is_using_pointer() {
            super::persistence::SettingsManager::save(&self.settings);
            self.settings_dirty = false;
        }
    }
}

/// Load persisted settings and bring the autostart task in line with them.
///
/// Returns the settings plus an optional status message for the Settings
/// panel. Autostart is only synced from a file that was actually loaded: when
/// the file is missing or corrupt, `auto_start` is instead read back from the
/// existing logon task so the user's autostart is never wiped.
fn load_settings_and_sync_autostart() -> (GuiSettings, Option<(String, bool, Instant)>) {
    use super::persistence::SettingsManager;

    match SettingsManager::load() {
        Ok(Some(settings)) => {
            let status = SettingsManager::set_autostart(settings.auto_start)
                .err()
                .map(|e| (format!("Autostart sync failed: {e}"), true, Instant::now()));
            (settings, status)
        }
        Ok(None) => {
            let settings = GuiSettings {
                auto_start: SettingsManager::is_autostart_enabled(),
                ..GuiSettings::default()
            };
            (settings, None)
        }
        Err(e) => {
            let settings = GuiSettings {
                auto_start: SettingsManager::is_autostart_enabled(),
                ..GuiSettings::default()
            };
            let status = (
                format!("Settings file could not be loaded, using defaults. {e}"),
                true,
                Instant::now(),
            );
            (settings, Some(status))
        }
    }
}

/// Native `HWND` of the eframe main window, as `isize`.
///
/// Taken from eframe's raw window handle. Falls back to a title lookup only
/// if eframe cannot provide a Win32 handle, which does not happen on the
/// native Windows backend.
fn window_hwnd(cc: &eframe::CreationContext<'_>) -> isize {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    cc.window_handle()
        .ok()
        .and_then(|handle| match handle.as_raw() {
            RawWindowHandle::Win32(win32) => Some(win32.hwnd.get()),
            _ => None,
        })
        .unwrap_or_else(|| crate::platform::window::find_app_window(strings::APP_NAME))
}

impl eframe::App for MagicXApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Reveal the window on the first frame (anti-flash).
        if !self.window_revealed {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            self.window_revealed = true;
        }

        // Poll tray icon events FIRST so that `quit_requested` is set
        // before the close intercept runs. Without this ordering, the
        // close intercept would cancel the close and re-hide the window
        // before the Quit action could be processed.
        self.poll_tray_events(ctx);

        // ── Close intercept ─────────────────────────────────────────
        // When minimize-to-tray is active and the user has not explicitly
        // selected "Quit" from the tray menu, hide the window instead of
        // closing it. Requires a live tray icon, otherwise the window would
        // vanish with no way to bring it back.
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested
            && self.settings.minimize_to_tray
            && self.tray_handle.is_some()
            && !self.quit_requested
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            // Cloak the window instead of hiding it (SW_HIDE) or using the
            // eframe Visible(false) API.  Both of those clear WS_VISIBLE,
            // which causes eframe's event loop to enter ControlFlow::Poll
            // (emilk/egui#7776), spinning the CPU at full speed.  Cloaking
            // minimizes the window as an invisible tool window while keeping
            // WS_VISIBLE set, so the event loop stays in ControlFlow::Wait
            // and request_repaint_after() properly gates the wakeup interval.
            crate::platform::window::cloak_window(self.hwnd);
            self.hidden_to_tray = true;
            self.hide_requested_at = Some(Instant::now());
        }

        // ── External restore detection ───────────────────────────────
        // When the app is cloaked to tray, a second instance (or other
        // external caller) may restore the window via ShowWindow(SW_RESTORE).
        // Detect the un-minimized state and reconcile our internal state
        // so the UI renders and the close button works normally.
        //
        // A 500 ms grace period after the close intercept prevents this
        // check from firing on the same or nearby frames.
        let hide_settled = self
            .hide_requested_at
            .is_none_or(|t| t.elapsed() >= Duration::from_millis(500));
        if self.hidden_to_tray
            && hide_settled
            && !crate::platform::window::is_window_minimized(self.hwnd)
        {
            // The window was un-minimized externally.  Restore the
            // extended styles (WS_EX_APPWINDOW, remove WS_EX_TOOLWINDOW)
            // so the taskbar button reappears.
            crate::platform::window::uncloak_window(self.hwnd);
            self.hidden_to_tray = false;
            self.hide_requested_at = None;
        }

        // Poll background results
        self.poll_clean_results();

        // Detect monitor start / stop transitions.
        if self.monitor_active != self.prev_monitor_active {
            if self.monitor_active {
                self.push_monitor_log(format!(
                    "Monitoring started: threshold {}%, cooldown {}s, level {}",
                    self.settings.monitor_threshold,
                    self.settings.monitor_cooldown_secs,
                    self.settings.default_clean_level.title_case_name(),
                ));
                // Immediately eligible for a status heartbeat, with a fresh
                // cooldown backoff.
                self.last_monitor_status_log = None;
                self.auto_clean_backoff = 1;
            } else {
                self.push_monitor_log("Monitoring stopped.".to_owned());
            }
            self.prev_monitor_active = self.monitor_active;
        }

        // Auto-clean if monitoring
        self.handle_monitor_auto_clean();

        // ── Visibility gate ────────────────────────────────────────
        // Use Win32 IsIconic for reliable minimized detection  -
        // egui's ViewportInfo::minimized can return None when the
        // platform does not report the state.
        let minimized = crate::platform::window::is_window_minimized(self.hwnd);
        let window_visible = !self.hidden_to_tray && !minimized;

        // Tell the stats thread whether the UI needs periodic data.
        // When the window is not visible and monitoring is off, skip all
        // work - no Win32 calls, no mutex locks, no repaints.
        self.needs_repaint.store(window_visible, Ordering::Release);
        self.needs_capture
            .store(window_visible || self.monitor_active, Ordering::Release);

        // When the window is hidden to tray or minimized with auto-clean
        // enabled, schedule a low-frequency wake-up so the threshold check
        // in handle_monitor_auto_clean() runs without the stats thread
        // having to call request_repaint() (which would cause unnecessary
        // rendering work). With monitoring off, no timer is scheduled: the
        // tray watcher and a second instance wake the loop explicitly.
        //
        // A tray-hidden window is cloaked (minimized tool window with
        // WS_VISIBLE), so eframe's event loop stays in ControlFlow::Wait
        // instead of the buggy ControlFlow::Poll that fires for truly
        // invisible windows (egui#7776), and request_repaint_after()
        // properly gates wakeups.
        if !window_visible && self.monitor_active {
            ctx.request_repaint_after(Duration::from_secs(2));
        }

        // ── Settings sync ────────────────────────────────────────────
        // Sync the tray handle when settings change.
        self.sync_integration_settings(ctx);
        self.persist_settings_if_changed(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Skip ALL rendering when the window is not visible.  This
        // prevents widgets like spinners and toggle animations from
        // calling request_repaint() which would otherwise create a
        // perpetual layout → repaint → layout loop even while
        // minimized or hidden to tray.
        let minimized = crate::platform::window::is_window_minimized(self.hwnd);
        let window_visible = !self.hidden_to_tray && !minimized;
        if window_visible {
            self.draw_visible_ui(ui);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stats_running.store(false, Ordering::Release);
        super::persistence::SettingsManager::save(&self.settings);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // The default eframe implementation returns a hardcoded near-black
        // colour, which bleeds through any panel that uses a transparent
        // frame fill. Return the app's own background colour instead so
        // the viewport clear colour always matches the in-app theme.
        theme::bg_color(self.settings.dark_mode).to_normalized_gamma_f32()
    }
}

// ─── One-Time Theme Configuration ────────────────────────────────────────────

/// Register custom visuals for both dark and light themes, apply shared text
/// styles and spacing, then activate the user's preferred variant.
///
/// Called once from [`MagicXApp::new`]. The `set_visuals_of` + `set_theme`
/// approach (rather than the legacy `set_visuals`) prevents the OS dark/light
/// preference from silently overriding the in-app selection.
fn configure_themes(ctx: &egui::Context, dark_mode: bool) {
    theme::register_themes(ctx);
    theme::set_active_theme(ctx, dark_mode);

    // `set_global_style` only modifies the *active* theme's style, so we
    // temporarily activate each variant, clone-and-patch it, then
    // restore the user's saved preference.
    for variant in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.set_theme(variant);
        let mut style = (*ctx.global_style()).clone();
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.window_margin = egui::Margin::same(10);

        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(21.0, FontFamily::Proportional),
        );
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(13.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(10.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        );

        ctx.set_global_style(style);
    }

    theme::set_active_theme(ctx, dark_mode);
}

// ─── Background Stats Thread ─────────────────────────────────────────────────

/// Background thread that periodically captures memory snapshots.
fn stats_thread(
    snapshot: &Arc<Mutex<Option<MemorySnapshot>>>,
    running: &Arc<AtomicBool>,
    needs_repaint: &Arc<AtomicBool>,
    needs_capture: &Arc<AtomicBool>,
    ctx: &egui::Context,
) {
    while running.load(Ordering::Acquire) {
        // Only capture when the UI is visible or the monitor needs
        // logic() to run for auto-clean threshold checks.  When the
        // window is hidden/minimized with monitoring off, skip all
        // work - no Win32 calls, no mutex locks, no repaints.
        if needs_capture.load(Ordering::Acquire)
            && let Ok(snap) = MemorySnapshot::capture()
        {
            if let Ok(mut lock) = snapshot.lock() {
                *lock = Some(snap);
            }

            // Only request a repaint when the window is actually visible.
            // When the monitor is running but the window is hidden, we
            // still capture data above for threshold checks, but skip
            // the repaint to avoid unnecessary UI wake-ups and CPU usage.
            if needs_repaint.load(Ordering::Acquire) {
                ctx.request_repaint();
            }
        }

        std::thread::sleep(Duration::from_millis(STATS_POLL_INTERVAL_MS));
    }
}

// ─── Sidebar ─────────────────────────────────────────────────────────────────

/// Navigation items: `(panel, icon, label)`.
///
/// Icons are sourced from the Phosphor icon font (`egui_phosphor::regular`),
/// which is registered at startup in [`MagicXApp::new`].
const NAV_ITEMS: [(Panel, &str, &str); 4] = [
    (Panel::Dashboard, ph::GAUGE, strings::tray::NAV_DASHBOARD),
    (Panel::Monitor, ph::ACTIVITY, strings::tray::NAV_MONITOR),
    (Panel::Processes, ph::CPU, strings::tray::NAV_PROCESSES),
    (Panel::Settings, ph::GEAR, strings::tray::NAV_SETTINGS),
];

/// Draw the sidebar with navigation and branding.
fn draw_sidebar(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;

    egui::Panel::left("sidebar")
        .resizable(false)
        .exact_size(theme::SIDEBAR_WIDTH)
        .frame(
            egui::Frame::new()
                .fill(theme::sidebar_bg(dark))
                .inner_margin(egui::Margin::symmetric(8, 10))
                .stroke(egui::Stroke::new(0.5_f32, theme::border_color(dark))),
        )
        .show_inside(ui, |ui| {
            draw_sidebar_brand(ui);
            ui.add_space(8.0);
            draw_sidebar_nav(ui, app);
            // Pin the About button to the bottom of the sidebar.
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                let selected = app.active_panel == Panel::About;
                draw_nav_button(
                    ui,
                    ph::INFO,
                    strings::gui::about::TITLE,
                    selected,
                    dark,
                    || {
                        app.active_panel = Panel::About;
                    },
                );
            });
        });
}

/// Draw a compact `MX` monogram badge at the top of the sidebar.
///
/// Replaces the full word-mark to save horizontal space in the icon-rail layout.
fn draw_sidebar_brand(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        let badge_size = egui::vec2(36.0, 36.0);
        let (rect, _) = ui.allocate_exact_size(badge_size, egui::Sense::hover());
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(10),
            theme::ACCENT.gamma_multiply(0.18),
        );
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            strings::MONOGRAM,
            egui::FontId::proportional(13.0),
            theme::ACCENT,
        );
    });
}

/// Draw the navigation buttons.
fn draw_sidebar_nav(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;
    for (panel, icon, label) in NAV_ITEMS {
        let selected = app.active_panel == panel;
        draw_nav_button(ui, icon, label, selected, dark, || {
            app.active_panel = panel;
        });
        ui.add_space(2.0);
    }
}

/// Draw a single icon-only navigation button.
///
/// The button fills the sidebar width and is square (height == [`theme::SIDEBAR_BUTTON_HEIGHT`]).
/// A pill-shaped background highlights the active or hovered state.
/// Hovering reveals a tooltip with the full panel name.
fn draw_nav_button(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    selected: bool,
    dark: bool,
    on_click: impl FnOnce(),
) {
    let desired_size = egui::vec2(ui.available_width(), theme::SIDEBAR_BUTTON_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
    let response = response.on_hover_text(label);

    if response.clicked() {
        on_click();
    }

    let hovered = response.hovered();
    let painter = ui.painter();

    // Rounded pill background for active / hovered state.
    let pill = rect.shrink(4.0);
    if selected {
        painter.rect_filled(
            pill,
            egui::CornerRadius::same(8),
            theme::ACCENT.gamma_multiply(0.20),
        );
    } else if hovered {
        painter.rect_filled(
            pill,
            egui::CornerRadius::same(8),
            theme::ACCENT.gamma_multiply(0.08),
        );
    }

    // Icon colour: accent when active, primary text on hover, muted otherwise.
    let icon_color = if selected {
        theme::ACCENT
    } else if hovered {
        theme::text_color(dark)
    } else {
        theme::muted_color(dark)
    };

    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(20.0),
        icon_color,
    );
}

// ─── Main Content ────────────────────────────────────────────────────────────

/// Draw the main content area based on the active panel.
fn draw_main_panel(ui: &mut egui::Ui, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::bg_color(dark)))
        .show_inside(ui, |ui| {
            egui::ScrollArea::vertical()
                .content_margin(egui::Margin::same(20))
                .show(ui, |ui| match app.active_panel {
                    Panel::Dashboard => panels::dashboard::draw(ui, app),
                    Panel::Monitor => panels::monitor::draw(ui, app),
                    Panel::Processes => panels::processes::draw(ui, app),
                    Panel::Settings => panels::settings::draw(ui, app),
                    Panel::About => panels::about::draw(ui, app),
                });
        });
}
