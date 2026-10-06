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
use self::background::{PROCESS_REFRESH_SECS, stats_thread};
pub use self::cleaning::{CleanProgress, CleanResultMsg, MapTransition};
use self::cleaning::MONITOR_LOG_CAPACITY;
use super::settings::GuiSettings;
use super::{fonts, nav, theme, tray};
use crate::engine::auto_clean::AutoCleanPolicy;
use crate::memory::{MemorySnapshot, ProcessMemoryInfo};
use crate::strings;

mod appearance;
mod background;
mod cleaning;
mod tray_events;

// ─── Constants ───────────────────────────────────────────────────────────────

// ─── Types ───────────────────────────────────────────────────────────────────

/// Which panel is currently shown in the main content area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    /// Whether monitoring auto-clean is active.
    pub monitor_active: bool,

    /// Auto-clean timing rules (threshold, cooldown and backoff), shared
    /// with the CLI monitor.
    auto_clean: AutoCleanPolicy,

    /// Last time a periodic status line was appended to the monitor log.
    ///
    /// Used to throttle heartbeat messages so the log is not flooded
    /// while the monitor is idle (memory below threshold).
    last_monitor_status_log: Option<Instant>,

    /// Previous frame's `monitor_active` state - used to detect
    /// start/stop transitions and log them once.
    prev_monitor_active: bool,

    /// Monitor log messages, oldest first, capped at 500 lines.
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
    /// Cached from [`crate::integration::context_menu::is_installed`] at startup and updated
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
        fonts::install(&cc.egui_ctx);
        theme::configure_style(&cc.egui_ctx);

        // Load persisted settings before applying the theme so the window
        // starts in the user's preferred mode without a one-frame flash.
        let (settings, settings_status) = load_settings_and_sync_autostart();
        let appearance = Appearance::read(settings.theme);

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

        let initial_dark_mode = appearance.palette.dark;

        // Take our own HWND from eframe so the tray-watcher thread can post a
        // synthetic WM_PAINT message that wakes eframe even when WS_VISIBLE
        // is cleared. A title lookup (FindWindowW) could match an unrelated
        // window with the same title, such as an Explorer folder.
        let hwnd = window_hwnd(cc);

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

        let app = Self {
            active_panel: Panel::Overview,
            latest_snapshot,
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
            monitor_active: settings.auto_clean_enabled,
            auto_clean: AutoCleanPolicy::new(
                settings.monitor_threshold,
                Duration::from_secs(settings.monitor_cooldown_secs),
            ),
            last_monitor_status_log: None,
            prev_monitor_active: false,
            monitor_log: VecDeque::with_capacity(MONITOR_LOG_CAPACITY),
            settings_snapshot: settings.clone(),
            settings_dirty: false,
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
            hide_requested_at: None,
            quit_requested: false,
            context_menu_installed: crate::integration::context_menu::is_installed(),
            hwnd,
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
                // Uncloak if the window was hidden while the setting was on.
                if self.hidden_to_tray {
                    crate::platform::window::uncloak_window(self.hwnd);
                    self.hidden_to_tray = false;
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
            let status = crate::integration::autostart::set_enabled(settings.auto_start)
                .map_err(|e| format!("{e:#}"))
                .err()
                .map(|e| (format!("Autostart sync failed: {e}"), true, Instant::now()));
            (settings, status)
        }
        Ok(None) => {
            let settings = GuiSettings {
                auto_start: crate::integration::autostart::is_enabled(),
                ..GuiSettings::default()
            };
            (settings, None)
        }
        Err(e) => {
            let settings = GuiSettings {
                auto_start: crate::integration::autostart::is_enabled(),
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
                // backoff; the cooldown since the last clean still applies.
                self.last_monitor_status_log = None;
                self.auto_clean.reset_backoff();
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
        theme::palette().bg.to_normalized_gamma_f32()
    }
}

// ─── Background Stats Thread ─────────────────────────────────────────────────

// ─── Sidebar ─────────────────────────────────────────────────────────────────

// ─── Main Content ────────────────────────────────────────────────────────────
