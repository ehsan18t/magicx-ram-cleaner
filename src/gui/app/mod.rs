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

use self::appearance::Appearance;
pub use self::autostart::AutostartView;
use self::background::{PROCESS_REFRESH_SECS, stats_thread};
use self::cleaning::MONITOR_LOG_CAPACITY;
pub use self::cleaning::{CleanProgress, CleanResultMsg, EventKind, MapTransition, MonitorEvent};
pub use self::trim::TrimState;
use super::settings::GuiSettings;
use super::{fonts, nav, theme, tray};
use crate::engine::auto_clean::AutoCleanPolicy;
use crate::memory::{MemorySnapshot, ProcessMemoryInfo};

mod appearance;
mod autostart;
mod background;
mod cleaning;
pub mod history;
mod tray_events;
mod trim;

// ─── Constants ───────────────────────────────────────────────────────────────

// ─── Types ───────────────────────────────────────────────────────────────────

/// Which panel is currently shown in the main content area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    /// Memory map, clean level picker and quick stats.
    Overview,
    /// Continuous monitoring with auto-clean.
    Monitor,
    /// Top processes by memory usage.
    Processes,
    /// User preferences.
    Settings,
    /// Application information, credits, and links.
    About,
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

    /// Recent readings for the Monitor chart (updated by background thread).
    pub history: Arc<Mutex<history::History>>,

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

    /// Progress of the running clean, updated by the worker thread.
    pub clean_progress: cleaning::SharedProgress,

    /// The memory map's move from before to after the last clean, while it
    /// is animating.
    pub map_transition: Option<MapTransition>,

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

    /// Set when the process list is known to be out of date (after a trim),
    /// so the next refresh runs without waiting for the interval.
    processes_stale: Arc<AtomicBool>,

    /// Running and recent trims on the Processes page.
    pub trim_log: trim::SharedTrimLog,

    /// Whether monitoring auto-clean is active.
    pub monitor_active: bool,

    /// Auto-clean timing rules (threshold, cooldown and backoff), shared
    /// with the CLI monitor.
    auto_clean: AutoCleanPolicy,

    /// Previous frame's `monitor_active` state - used to detect
    /// start/stop transitions and log them once.
    prev_monitor_active: bool,

    /// Auto-clean events, oldest first, capped at 500.
    pub monitor_log: VecDeque<MonitorEvent>,

    /// User settings.
    pub settings: GuiSettings,

    /// Shadow copy of settings used to detect changes and trigger auto-save.
    settings_snapshot: GuiSettings,

    /// `true` when settings changed but have not been written to disk yet.
    ///
    /// Saving is deferred while a widget is being dragged (e.g. a slider) so
    /// the file is not rewritten on every frame, and retried after a failed
    /// write.
    settings_dirty: bool,

    /// When a failed save may be retried, and the error it failed with (shown
    /// once, not on every retry).
    save_failure: Option<(Instant, String)>,

    /// Process sort column (0=name, 1=count, 2=memory, 3=peak).
    pub process_sort_col: usize,

    /// Process sort ascending.
    pub process_sort_asc: bool,

    /// Real-time search / filter text for the processes panel.
    pub process_search: String,

    /// The Windows appearance as last read, and the palette in use.
    appearance: Appearance,

    /// Whether the navigation pane is open over the page (narrow windows).
    pub nav_overlay_open: bool,

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

    /// Set to `true` when the user selects "Quit" from the tray menu.
    ///
    /// Allows the close-intercept logic to distinguish between a user quitting
    /// via tray and a regular window close (which should minimize instead).
    quit_requested: bool,

    /// Whether the Desktop context menu is currently installed in the registry.
    ///
    /// Cached from [`crate::integration::context_menu::is_installed`] at startup and updated
    /// by the Settings panel after each install or uninstall operation.
    pub context_menu_installed: bool,

    /// Win32 `HWND` of the main application window, stored as `isize`.
    hwnd: isize,

    /// The Start with Windows switch and its background task reads.
    pub autostart: autostart::AutostartUi,

    /// Whether to start hidden in the tray (the logon task's `--tray`).
    /// Only honoured when the tray icon exists, so the window can come back.
    start_in_tray: bool,
}

impl MagicXApp {
    /// Hide the window to the tray. The window is truly hidden, so it has no
    /// taskbar button and eframe runs only `logic()` for it, with no egui
    /// pass or painting, until something requests a repaint.
    fn hide_to_tray(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        self.hidden_to_tray = true;
    }

    /// Show the window again and bring it to the front. A minimized window
    /// is restored; a maximized one stays maximized.
    pub(super) fn show_window(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.hidden_to_tray = false;
    }

    /// Show `text` under the Settings cards for a few seconds.
    pub fn show_settings_status(&mut self, text: String, is_error: bool) {
        self.settings_status = Some((text, is_error, Instant::now()));
    }

    /// The main window's `HWND`, for dialogs that must be modal to it.
    #[must_use]
    pub const fn hwnd(&self) -> isize {
        self.hwnd
    }

    /// Create the app, spawn background threads, and apply the initial theme.
    ///
    /// # Errors
    ///
    /// Returns an error if the background stats thread cannot be spawned.
    pub fn new(cc: &eframe::CreationContext<'_>, start_in_tray: bool) -> anyhow::Result<Self> {
        fonts::install(&cc.egui_ctx);
        theme::configure_style(&cc.egui_ctx);

        // Load persisted settings before applying the theme so the window
        // starts in the user's preferred mode without a one-frame flash.
        let (settings, settings_status) = load_settings();
        let appearance = Appearance::read(settings.theme);

        let (clean_tx, clean_rx) = mpsc::channel();
        let latest_snapshot = Arc::new(Mutex::new(None));
        let history = Arc::new(Mutex::new(history::History::default()));
        let stats_running = Arc::new(AtomicBool::new(true));
        let needs_repaint = Arc::new(AtomicBool::new(true));
        let needs_capture = Arc::new(AtomicBool::new(true));
        let top_processes = Arc::new(Mutex::new(None));

        // Spawn background stats collection thread
        {
            let snapshot_ref = Arc::clone(&latest_snapshot);
            let history_ref = Arc::clone(&history);
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
                            &history_ref,
                            &running_ref,
                            &repaint_ref,
                            &capture_ref,
                            &ctx,
                        );
                    })
                    .context("failed to spawn stats thread")?,
            );
        }

        let initial_dark_mode = appearance.palette.dark;

        // Take our own HWND from eframe for the Win32 window calls (theme,
        // visibility, dialogs). A title lookup (FindWindowW) could match an
        // unrelated window with the same title, such as an Explorer folder.
        let hwnd = window_hwnd(cc);

        // Initialize tray icon if minimize-to-tray was previously enabled.
        // The egui context lets tray clicks wake the event loop.
        let (tray_handle, tray_error) = if settings.minimize_to_tray {
            match tray::TrayHandle::new(cc.egui_ctx.clone(), initial_dark_mode) {
                Ok(handle) => (Some(handle), None),
                Err(e) => (None, Some(e)),
            }
        } else {
            (None, None)
        };

        let app = Self {
            active_panel: Panel::Overview,
            latest_snapshot,
            history,
            stats_running,
            needs_repaint,
            needs_capture,
            clean_rx,
            clean_tx,
            cleaning_in_progress: false,
            last_clean_result: None,
            clean_progress: Arc::new(Mutex::new(None)),
            map_transition: None,
            top_processes,
            last_process_refresh: Instant::now()
                .checked_sub(Duration::from_secs(PROCESS_REFRESH_SECS + 1))
                .unwrap_or_else(Instant::now),
            process_refresh_in_flight: Arc::new(AtomicBool::new(false)),
            processes_stale: Arc::new(AtomicBool::new(false)),
            trim_log: Arc::default(),
            monitor_active: settings.auto_clean_enabled,
            auto_clean: AutoCleanPolicy::new(
                settings.monitor_threshold,
                Duration::from_secs(settings.monitor_cooldown_secs),
            ),
            prev_monitor_active: false,
            monitor_log: VecDeque::with_capacity(MONITOR_LOG_CAPACITY),
            settings_snapshot: settings.clone(),
            settings_dirty: false,
            save_failure: None,
            settings,
            process_sort_col: 2,
            process_sort_asc: false,
            process_search: String::new(),
            appearance,
            nav_overlay_open: false,
            window_revealed: false,
            settings_status,
            tray_handle,
            tray_error,
            hidden_to_tray: false,
            quit_requested: false,
            context_menu_installed: crate::integration::context_menu::is_installed(),
            hwnd,
            autostart: autostart::AutostartUi::start(&cc.egui_ctx),
            start_in_tray,
        };
        // Native menus, the title bar and egui all match from the first frame.
        app.apply_appearance(&cc.egui_ctx);
        Ok(app)
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
                // Show the window if it was hidden while the setting was on.
                if self.hidden_to_tray {
                    self.show_window(ctx);
                }
            }
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

        // Follow the theme setting and the Windows theme and accent.
        let ctx = ui.ctx().clone();
        self.refresh_appearance(&ctx);

        // Enforce the resolved theme every visible frame: eframe's own
        // system-theme detection can override it between frames when the
        // app's theme differs from the OS theme.
        let desired = if self.dark() {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
        if ui.theme() != desired {
            ctx.set_theme(desired);
        }

        // ── Layout ───────────────────────────────────────────
        nav::draw_pane(ui, self);
        nav::draw_page(ui, self);
    }

    /// Save settings to disk after the user changes anything.
    ///
    /// Writes are deferred while the pointer is dragging a widget (e.g. a
    /// slider) so the file is not rewritten every frame; the final value is
    /// saved on the first frame after release. A failed write (read-only
    /// media, a locked file) is reported once in the Settings panel and
    /// retried every few seconds until it succeeds.
    fn persist_settings_if_changed(&mut self, ctx: &egui::Context) {
        /// Wait between attempts after a failed save.
        const RETRY: Duration = Duration::from_secs(5);

        if self.settings != self.settings_snapshot {
            self.settings_snapshot = self.settings.clone();
            self.settings_dirty = true;
        }
        let retry_due = self
            .save_failure
            .as_ref()
            .is_none_or(|(at, _)| at.elapsed() >= RETRY);
        if !self.settings_dirty || ctx.egui_is_using_pointer() || !retry_due {
            return;
        }
        match super::persistence::save(&self.settings) {
            Ok(()) => {
                self.settings_dirty = false;
                self.save_failure = None;
            }
            Err(e) => {
                let error = format!("{e:#}");
                let new_error = self
                    .save_failure
                    .as_ref()
                    .is_none_or(|(_, last)| *last != error);
                if new_error {
                    self.settings_status = Some((
                        format!(
                            "Settings couldn\u{2019}t be saved and will reset next time: {error}"
                        ),
                        true,
                        Instant::now(),
                    ));
                }
                self.save_failure = Some((Instant::now(), error));
                ctx.request_repaint_after(RETRY);
            }
        }
    }
}

/// Load persisted settings, with an optional status message for the
/// Settings panel when the file had problems.
fn load_settings() -> (GuiSettings, Option<(String, bool, Instant)>) {
    use super::persistence::Loaded;

    match super::persistence::load() {
        Loaded::Read {
            settings,
            reset_fields,
        } => {
            let status = (!reset_fields.is_empty()).then(|| {
                (
                    format!(
                        "Some settings had invalid values and were reset to their defaults: {}",
                        reset_fields.join(", ")
                    ),
                    true,
                    Instant::now(),
                )
            });
            (settings, status)
        }
        Loaded::Missing => (GuiSettings::default(), None),
        Loaded::Unusable { error, kept_as } => {
            let kept = kept_as.map_or_else(String::new, |path| {
                format!(" The old file was kept as {}.", path.display())
            });
            let status = (
                format!("Settings file could not be loaded, using defaults. {error}{kept}"),
                true,
                Instant::now(),
            );
            (GuiSettings::default(), Some(status))
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
        .unwrap_or_else(|| crate::platform::window::find_app_window(crate::ids::WINDOW_TITLE))
}

impl eframe::App for MagicXApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Reveal the window on the first frame (anti-flash). A `--tray`
        // start with a live tray icon stays hidden in the tray instead (the
        // window is created hidden, so it never shows).
        if !self.window_revealed {
            if self.start_in_tray && self.tray_handle.is_some() {
                self.hidden_to_tray = true;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }
            self.window_revealed = true;
        }

        // Poll tray icon events FIRST so that `quit_requested` is set
        // before the close intercept runs. Without this ordering, the
        // close intercept would cancel the close and hide the window
        // before the Quit action could be processed.
        self.poll_tray_events(ctx);

        // ── External restore detection ───────────────────────────────
        // A second launch shows the hidden window directly (ShowWindow).
        // Notice that, and send the matching viewport commands so eframe's
        // own idea of the window agrees and it gets focus. This runs before
        // the close intercept: a hide requested this frame only takes effect
        // after `logic()` returns, so the window still looks visible here.
        if self.hidden_to_tray && crate::platform::window::is_window_visible(self.hwnd) {
            self.show_window(ctx);
        }

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
            self.hide_to_tray(ctx);
        }

        // Poll background results
        self.poll_clean_results();
        self.poll_autostart(ctx);

        // Detect monitor start / stop transitions.
        if self.monitor_active != self.prev_monitor_active {
            if self.monitor_active {
                self.push_monitor_log(
                    EventKind::Info,
                    format!(
                        "Auto-clean turned on: {} when memory reaches {}%, at most every {} s",
                        self.settings.default_clean_level.title_case_name(),
                        self.settings.monitor_threshold,
                        self.settings.monitor_cooldown_secs,
                    ),
                );
                // A fresh backoff; the cooldown since the last clean still
                // applies.
                self.auto_clean.reset_backoff();
            } else {
                self.push_monitor_log(EventKind::Info, "Auto-clean turned off".to_owned());
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
        // rendering work). With monitoring off, no timer is scheduled: tray
        // clicks and a second instance wake the loop explicitly. For a
        // hidden window eframe runs only `logic()` on these wake-ups.
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
        // Save only what is not on disk yet. Writing unchanged settings
        // would replace a file that failed to load with defaults even though
        // the user changed nothing. A failure here has nowhere to be shown.
        if self.settings_dirty || self.settings != self.settings_snapshot {
            drop(super::persistence::save(&self.settings));
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // The default eframe implementation returns a hardcoded near-black
        // colour, which bleeds through any panel that uses a transparent
        // frame fill. Return the app's own background colour instead so
        // the viewport clear colour always matches the in-app theme.
        theme::palette().bg.to_normalized_gamma_f32()
    }
}

// ─── Background Stats Thread ─────────────────────────────────────────────────

// ─── Sidebar ─────────────────────────────────────────────────────────────────

// ─── Main Content ────────────────────────────────────────────────────────────
