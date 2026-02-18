//! # GUI Application State
//!
//! Core [`MagicXApp`] struct implementing [`eframe::App`], background thread
//! management, and the main layout (sidebar + panel routing).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use eframe::egui;
use egui::{FontFamily, FontId, TextStyle};
use egui_phosphor::regular as ph;

use serde::{Deserialize, Serialize};

use crate::cleaner::{self, CleanLevel, SmartCleanResult};
use crate::stats::{self, MemorySnapshot, ProcessMemoryInfo};

use super::{panels, theme, tray};

// ─── Constants ───────────────────────────────────────────────────────────────

/// Maximum number of memory history samples kept in the ring buffer.
const HISTORY_CAPACITY: usize = 300;

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
}

/// A timestamped memory snapshot for the history chart.
#[derive(Clone)]
pub struct HistoryPoint {
    /// Seconds since the GUI was launched.
    pub elapsed_secs: f64,
    /// Used physical memory in bytes.
    pub used_bytes: u64,
    /// Available physical memory in bytes.
    pub available_bytes: u64,
}

/// Persistent user settings.
///
/// Contains several independent boolean preferences; no meaningful two-variant
/// enum reduction exists without obscuring what each field controls.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiSettings {
    /// Minimize to the system tray when the close button is clicked.
    ///
    /// When enabled, clicking ✕ hides the window to the notification area
    /// rather than quitting. The tray icon provides "Open" and "Quit" actions.
    #[serde(alias = "tray_enabled")]
    pub minimize_to_tray: bool,
    /// Launch automatically at Windows startup (current user only).
    ///
    /// Writes (or removes) a value under
    /// `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
    pub auto_start: bool,
    /// Auto-clean threshold percentage (0 = disabled).
    pub monitor_threshold: u32,
    /// Cooldown between auto-cleans (seconds).
    pub monitor_cooldown_secs: u64,
    /// Default cleaning level.
    pub default_clean_level: CleanLevel,
    /// Number of top processes to show.
    pub top_process_count: usize,
    /// Theme preference (`true` = dark).
    pub dark_mode: bool,
    /// Show tooltip with level details on circle hover (`true` = enabled).
    pub show_level_tooltips: bool,
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
        }
    }
}

/// Result of a background cleaning operation sent back to the UI thread.
pub struct CleanResultMsg {
    /// The cleaning result (or error string).
    pub result: std::result::Result<SmartCleanResult, String>,
    /// Which level was requested.
    pub level: CleanLevel,
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

    /// Memory usage history ring buffer (updated by background thread).
    pub history: Arc<Mutex<VecDeque<HistoryPoint>>>,

    /// Background stats thread shutdown signal.
    stats_running: Arc<AtomicBool>,

    /// Channel for receiving cleaning results from worker threads.
    clean_rx: Receiver<CleanResultMsg>,

    /// Channel sender cloned into worker threads.
    clean_tx: Sender<CleanResultMsg>,

    /// Whether a cleaning operation is currently in progress.
    pub cleaning_in_progress: bool,

    /// Last cleaning result (for display).
    pub last_clean_result: Option<CleanResultMsg>,

    /// Top processes list (refreshed periodically).
    pub top_processes: Arc<Mutex<Vec<ProcessMemoryInfo>>>,

    /// Last time processes were refreshed.
    last_process_refresh: Instant,

    /// Whether monitoring auto-clean is active.
    pub monitor_active: bool,

    /// Last time auto-clean triggered (for cooldown).
    last_auto_clean: Option<Instant>,

    /// Last time a periodic status line was appended to the monitor log.
    ///
    /// Used to throttle heartbeat messages so the log is not flooded
    /// while the monitor is idle (memory below threshold).
    last_monitor_status_log: Option<Instant>,

    /// Previous frame's `monitor_active` state — used to detect
    /// start/stop transitions and log them once.
    prev_monitor_active: bool,

    /// Monitor log messages.
    pub monitor_log: Vec<String>,

    /// User settings.
    pub settings: GuiSettings,

    /// Shadow copy of settings used to detect changes and trigger auto-save.
    settings_snapshot: GuiSettings,

    /// Process sort column (0=name, 1=pid, 2=`working_set`, 3=peak).
    pub process_sort_col: usize,

    /// Process sort ascending.
    pub process_sort_asc: bool,

    /// Whether the dark theme was last applied (tracks theme changes).
    theme_applied: bool,

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

    /// Whether the window is currently hidden to the system tray.
    hidden_to_tray: bool,

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
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Register Phosphor icon font so all icon glyphs render correctly.
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        // Load persisted settings before applying the theme so the window
        // starts in the user's preferred mode without a one-frame flash.
        let settings = super::persistence::SettingsManager::load();

        // Apply the persisted theme immediately.
        if settings.dark_mode {
            theme::apply_dark_theme(&cc.egui_ctx);
        } else {
            theme::apply_light_theme(&cc.egui_ctx);
        }

        // Configure text styles for crisp rendering
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.window_margin = egui::Margin::same(10);

        // Tighter text styles for a compact look
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

        cc.egui_ctx.set_style(style);

        let (clean_tx, clean_rx) = mpsc::channel();
        let latest_snapshot = Arc::new(Mutex::new(None));
        let history = Arc::new(Mutex::new(VecDeque::with_capacity(HISTORY_CAPACITY)));
        let stats_running = Arc::new(AtomicBool::new(true));
        let top_processes = Arc::new(Mutex::new(Vec::new()));

        // Spawn background stats collection thread
        let start_time = Instant::now();
        {
            let snapshot_ref = Arc::clone(&latest_snapshot);
            let history_ref = Arc::clone(&history);
            let running_ref = Arc::clone(&stats_running);
            let ctx = cc.egui_ctx.clone();

            std::thread::Builder::new()
                .name("gui-stats".into())
                .spawn(move || {
                    stats_thread(&snapshot_ref, &history_ref, &running_ref, start_time, &ctx);
                })
                .expect("failed to spawn stats thread");
        }

        // Capture dark_mode before settings is moved into Self.
        let initial_dark_mode = settings.dark_mode;

        // Look up our own HWND so the tray-watcher thread can post a synthetic
        // WM_PAINT message that wakes eframe even when WS_VISIBLE is cleared.
        // FindWindowW succeeds here because the window is already created by
        // eframe before the app_creator closure is called.
        let hwnd = crate::console::find_app_window("MagicX RAM Cleaner");

        // Initialize tray icon if minimize-to-tray was previously enabled.
        // Pass the egui context and HWND so the watcher thread can both call
        // request_repaint() (for visible windows) and post a synthetic WM_PAINT
        // (for hidden windows, where request_repaint alone is suppressed by OS).
        let tray_handle = if settings.minimize_to_tray {
            tray::TrayHandle::new(cc.egui_ctx.clone(), hwnd).ok()
        } else {
            None
        };

        // Sync Windows autostart registry entry with the last saved preference.
        let _sync_autostart =
            super::persistence::SettingsManager::set_autostart(settings.auto_start);

        Self {
            active_panel: Panel::Dashboard,
            latest_snapshot,
            history,
            stats_running,
            clean_rx,
            clean_tx,
            cleaning_in_progress: false,
            last_clean_result: None,
            top_processes,
            last_process_refresh: Instant::now()
                .checked_sub(Duration::from_secs(PROCESS_REFRESH_SECS + 1))
                .unwrap_or_else(Instant::now),
            monitor_active: false,
            last_auto_clean: None,
            last_monitor_status_log: None,
            prev_monitor_active: false,
            monitor_log: Vec::new(),
            settings_snapshot: settings.clone(),
            settings,
            process_sort_col: 2,
            process_sort_asc: false,
            theme_applied: initial_dark_mode,
            window_revealed: false,
            settings_status: None,
            tray_handle,
            hidden_to_tray: false,
            quit_requested: false,
            context_menu_installed: crate::context_menu::is_installed(),
            hwnd,
        }
    }

    /// Start a cleaning operation on a background thread.
    pub fn start_clean(&mut self, level: CleanLevel) {
        if self.cleaning_in_progress {
            return;
        }
        self.cleaning_in_progress = true;
        self.last_clean_result = None;

        let tx = self.clean_tx.clone();
        std::thread::Builder::new()
            .name("gui-clean".into())
            .spawn(move || {
                let result = cleaner::smart_clean(level, false, &[]).map_err(|e| format!("{e:#}"));
                drop(tx.send(CleanResultMsg { result, level }));
            })
            .expect("failed to spawn clean thread");
    }

    /// Poll for completed cleaning results.
    ///
    /// When monitoring is active the result is also logged to the
    /// activity log exactly once (here, not in `update()`).
    fn poll_clean_results(&mut self) {
        if let Ok(msg) = self.clean_rx.try_recv() {
            self.cleaning_in_progress = false;

            // Log auto-clean outcome to the monitor activity log.
            if self.monitor_active {
                let log_msg = match &msg.result {
                    Ok(r) => format!(
                        "Auto-clean complete: freed {}",
                        stats::format_bytes(if r.total_freed > 0 {
                            r.total_freed as u64
                        } else {
                            0
                        }),
                    ),
                    Err(e) => format!("Auto-clean failed: {e}"),
                };
                self.monitor_log.push(log_msg);
            }

            self.last_clean_result = Some(msg);
        }
    }

    /// Refresh the process list if enough time has passed.
    fn maybe_refresh_processes(&mut self) {
        if self.last_process_refresh.elapsed() >= Duration::from_secs(PROCESS_REFRESH_SECS) {
            self.last_process_refresh = Instant::now();
            let procs_ref = Arc::clone(&self.top_processes);
            let count = self.settings.top_process_count;
            std::thread::Builder::new()
                .name("gui-procs".into())
                .spawn(move || {
                    if let Ok(procs) = stats::query_top_processes(count)
                        && let Ok(mut lock) = procs_ref.lock()
                    {
                        *lock = procs;
                    }
                })
                .expect("failed to spawn process thread");
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

        // Check cooldown
        if let Some(last) = self.last_auto_clean
            && last.elapsed() < Duration::from_secs(self.settings.monitor_cooldown_secs)
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
                    "Memory load {load}% >= threshold {}% \u{2014} auto-cleaning ({})...",
                    self.settings.monitor_threshold, self.settings.default_clean_level,
                );
                self.monitor_log.push(msg);
                self.last_auto_clean = Some(Instant::now());
                self.last_monitor_status_log = Some(Instant::now());
                self.start_clean(self.settings.default_clean_level);
            } else {
                // Periodic heartbeat so the user knows the monitor is alive.
                let should_log = self
                    .last_monitor_status_log
                    .is_none_or(|t| t.elapsed() >= Duration::from_secs(60));

                if should_log {
                    self.monitor_log.push(format!(
                        "Checked: memory at {load}%, below threshold {}% \u{2014} no action needed.",
                        self.settings.monitor_threshold,
                    ));
                    self.last_monitor_status_log = Some(Instant::now());
                }
            }
        }
    }

    /// Poll the tray icon event queues and dispatch any pending [`TrayAction`].
    ///
    /// Called every frame from [`eframe::App::update`].
    fn poll_tray_events(&mut self, ctx: &egui::Context) {
        let Some(action) = self.tray_handle.as_ref().and_then(tray::TrayHandle::poll) else {
            return;
        };
        match action {
            tray::TrayAction::Show => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.hidden_to_tray = false;
            }
            tray::TrayAction::Clean(level) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.hidden_to_tray = false;
                self.active_panel = Panel::Dashboard;
                self.start_clean(level);
            }
            tray::TrayAction::Navigate(panel) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.hidden_to_tray = false;
                self.active_panel = panel;
            }
            tray::TrayAction::Quit => {
                self.quit_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Synchronise the tray handle and autostart registry when the user
    /// changes the relevant settings in the Settings panel.
    ///
    /// Compares live settings against the snapshot so this only acts on the
    /// frame the toggle is flipped — it is a no-op every other frame.
    fn sync_integration_settings(&mut self, ctx: &egui::Context) {
        let tray_changed =
            self.settings.minimize_to_tray != self.settings_snapshot.minimize_to_tray;
        let autostart_changed = self.settings.auto_start != self.settings_snapshot.auto_start;

        if tray_changed {
            if self.settings.minimize_to_tray {
                self.tray_handle = tray::TrayHandle::new(ctx.clone(), self.hwnd).ok();
            } else {
                self.tray_handle = None;
                // Un-hide if the window was minimised while the setting was on.
                if self.hidden_to_tray {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    self.hidden_to_tray = false;
                }
            }
        }

        if autostart_changed {
            let _sync =
                super::persistence::SettingsManager::set_autostart(self.settings.auto_start);
        }
    }
}

impl eframe::App for MagicXApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
        // closing it.
        if ctx.input(|i| i.viewport().close_requested())
            && self.settings.minimize_to_tray
            && !self.quit_requested
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.hidden_to_tray = true;
        }

        // Poll background results
        self.poll_clean_results();

        // Detect monitor start / stop transitions.
        if self.monitor_active != self.prev_monitor_active {
            if self.monitor_active {
                self.monitor_log.push(format!(
                    "Monitoring started \u{2014} threshold {}%, cooldown {}s, level {}",
                    self.settings.monitor_threshold,
                    self.settings.monitor_cooldown_secs,
                    self.settings.default_clean_level,
                ));
                // Immediately eligible for a status heartbeat.
                self.last_monitor_status_log = None;
            } else {
                self.monitor_log.push("Monitoring stopped.".to_owned());
            }
            self.prev_monitor_active = self.monitor_active;
        }

        // Auto-clean if monitoring
        self.handle_monitor_auto_clean();

        // Maybe refresh processes
        if self.active_panel == Panel::Processes {
            self.maybe_refresh_processes();
        }

        // Apply theme when toggled
        if self.settings.dark_mode && !self.theme_applied {
            theme::apply_dark_theme(ctx);
            self.theme_applied = true;
        } else if !self.settings.dark_mode && self.theme_applied {
            theme::apply_light_theme(ctx);
            self.theme_applied = false;
        }

        // Request repaint for live updates
        ctx.request_repaint_after(Duration::from_millis(500));

        // ── Layout ───────────────────────────────────────────────────
        draw_sidebar(ctx, self);
        draw_main_panel(ctx, self);

        // ── Settings sync ────────────────────────────────────────────
        // Sync tray handle and autostart registry when settings change.
        self.sync_integration_settings(ctx);

        // Persist settings immediately whenever the user changes anything.
        if self.settings != self.settings_snapshot {
            super::persistence::SettingsManager::save(&self.settings);
            self.settings_snapshot = self.settings.clone();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stats_running.store(false, Ordering::Release);
        super::persistence::SettingsManager::save(&self.settings);
    }
}

// ─── Background Stats Thread ─────────────────────────────────────────────────

/// Background thread that periodically captures memory snapshots.
fn stats_thread(
    snapshot: &Arc<Mutex<Option<MemorySnapshot>>>,
    history: &Arc<Mutex<VecDeque<HistoryPoint>>>,
    running: &Arc<AtomicBool>,
    start_time: Instant,
    ctx: &egui::Context,
) {
    while running.load(Ordering::Acquire) {
        if let Ok(snap) = MemorySnapshot::capture() {
            let point = HistoryPoint {
                elapsed_secs: start_time.elapsed().as_secs_f64(),
                used_bytes: snap.used_physical,
                available_bytes: snap.available_physical,
            };

            if let Ok(mut lock) = snapshot.lock() {
                *lock = Some(snap);
            }
            if let Ok(mut lock) = history.lock() {
                if lock.len() >= HISTORY_CAPACITY {
                    lock.pop_front();
                }
                lock.push_back(point);
            }

            ctx.request_repaint();
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
    (Panel::Dashboard, ph::GAUGE, "Dashboard"),
    (Panel::Monitor, ph::ACTIVITY, "Monitor"),
    (Panel::Processes, ph::CPU, "Processes"),
    (Panel::Settings, ph::GEAR, "Settings"),
];

/// Draw the sidebar with navigation and branding.
fn draw_sidebar(ctx: &egui::Context, app: &mut MagicXApp) {
    let dark = app.settings.dark_mode;

    egui::SidePanel::left("sidebar")
        .resizable(false)
        .exact_width(theme::SIDEBAR_WIDTH)
        .frame(
            egui::Frame::new()
                .fill(theme::sidebar_bg(dark))
                .inner_margin(egui::Margin::symmetric(8, 10))
                .stroke(egui::Stroke::new(0.5, theme::border_color(dark))),
        )
        .show(ctx, |ui| {
            draw_sidebar_brand(ui);
            ui.add_space(8.0);
            draw_sidebar_nav(ui, app);
            // Pin version string to the bottom of the sidebar.
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .size(9.0)
                        .color(theme::muted_color(dark)),
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
            "MGX",
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
fn draw_main_panel(ctx: &egui::Context, app: &mut MagicXApp) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new())
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(20))
                    .show(ui, |ui| match app.active_panel {
                        Panel::Dashboard => panels::dashboard::draw(ui, app),
                        Panel::Monitor => panels::monitor::draw(ui, app),
                        Panel::Processes => panels::processes::draw(ui, app),
                        Panel::Settings => panels::settings::draw(ui, app),
                    });
            });
        });
}
