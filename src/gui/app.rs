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

use crate::cleaner::{self, CleanLevel, SmartCleanResult};
use crate::stats::{self, MemorySnapshot, ProcessMemoryInfo};

use super::{panels, theme};

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
    /// Memory overview with chart and stats.
    Dashboard,
    /// One-click cleaning with severity levels.
    Clean,
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
#[derive(Clone)]
pub struct GuiSettings {
    /// Enable system tray icon (disabled by default).
    pub tray_enabled: bool,
    /// Enable context menu integration (disabled by default).
    pub context_menu_enabled: bool,
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
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            tray_enabled: false,
            context_menu_enabled: false,
            monitor_threshold: DEFAULT_THRESHOLD,
            monitor_cooldown_secs: DEFAULT_COOLDOWN_SECS,
            default_clean_level: DEFAULT_CLEAN_LEVEL,
            top_process_count: DEFAULT_TOP_PROCESSES,
            dark_mode: true,
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
// Five independent boolean flags that represent unrelated on/off states.
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

    /// Monitor log messages.
    pub monitor_log: Vec<String>,

    /// User settings.
    pub settings: GuiSettings,

    /// Process sort column (0=name, 1=pid, 2=`working_set`, 3=peak).
    pub process_sort_col: usize,

    /// Process sort ascending.
    pub process_sort_asc: bool,

    /// Whether the dark theme was last applied (tracks theme changes).
    theme_applied: bool,

    /// Whether the window has been revealed (for anti-flash).
    window_revealed: bool,
}

impl MagicXApp {
    /// Create the app, spawn background threads, and apply the initial theme.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Apply modern dark theme immediately
        theme::apply_dark_theme(&cc.egui_ctx);

        // Configure text styles for crisp rendering
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.window_margin = egui::Margin::same(10);

        // Tighter text styles for a compact look
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(18.0, FontFamily::Proportional),
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
            monitor_log: Vec::new(),
            settings: GuiSettings::default(),
            process_sort_col: 2,
            process_sort_asc: false,
            theme_applied: true,
            window_revealed: false,
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
    fn poll_clean_results(&mut self) {
        if let Ok(msg) = self.clean_rx.try_recv() {
            self.cleaning_in_progress = false;
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

        if let Some(load) = load
            && load >= self.settings.monitor_threshold
        {
            let msg = format!(
                "Memory load {load}% >= threshold {}% \u{2014} auto-cleaning ({})...",
                self.settings.monitor_threshold, self.settings.default_clean_level
            );
            self.monitor_log.push(msg);
            self.last_auto_clean = Some(Instant::now());
            self.start_clean(self.settings.default_clean_level);
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

        // Poll background results
        self.poll_clean_results();

        // Log completed auto-clean results
        if let Some(ref result) = self.last_clean_result
            && self.monitor_active
        {
            let msg = match &result.result {
                Ok(r) => format!(
                    "Auto-clean complete: freed {}",
                    stats::format_bytes(if r.total_freed > 0 {
                        r.total_freed as u64
                    } else {
                        0
                    })
                ),
                Err(e) => format!("Auto-clean failed: {e}"),
            };
            self.monitor_log.push(msg);
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
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stats_running.store(false, Ordering::Release);
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
/// Icons use simple Unicode glyphs that render reliably in egui's
/// built-in font (no emoji variation selectors).
const NAV_ITEMS: [(Panel, &str, &str); 5] = [
    (Panel::Dashboard, "\u{2261}", "Dashboard"), // ≡
    (Panel::Clean, "\u{2726}", "Clean"),         // ✦
    (Panel::Monitor, "\u{25C9}", "Monitor"),     // ◉
    (Panel::Processes, "\u{2630}", "Processes"), // ☰
    (Panel::Settings, "\u{2699}", "Settings"),   // ⚙ (no FE0F)
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
                .inner_margin(egui::Margin::symmetric(10, 12))
                .stroke(egui::Stroke::new(0.5, theme::border_color(dark))),
        )
        .show(ctx, |ui| {
            draw_sidebar_brand(ui, dark);
            ui.add_space(14.0);
            draw_sidebar_nav(ui, app);
        });
}

/// Draw the `MagicX` branding text.
fn draw_sidebar_brand(ui: &mut egui::Ui, dark: bool) {
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("MagicX")
                .strong()
                .size(20.0)
                .color(theme::ACCENT),
        );
        ui.label(
            egui::RichText::new("RAM Cleaner")
                .size(10.0)
                .color(theme::muted_color(dark)),
        );
        ui.add_space(1.0);
        ui.label(
            egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                .size(9.0)
                .color(theme::muted_color(dark)),
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
        ui.add_space(1.0);
    }
}

/// Draw a single navigation button with icon and label.
///
/// When `selected`, the button gets an accent-coloured left bar and
/// a highlighted background.
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

    if response.clicked() {
        on_click();
    }

    let hovered = response.hovered();
    let painter = ui.painter();

    // Background highlight
    if selected || hovered {
        let bg = if selected {
            theme::ACCENT.gamma_multiply(0.15)
        } else {
            theme::ACCENT.gamma_multiply(0.06)
        };
        painter.rect_filled(rect, egui::CornerRadius::same(6), bg);
    }

    // Left accent bar when selected
    if selected {
        let bar = egui::Rect::from_min_size(rect.left_top(), egui::vec2(3.0, rect.height()));
        painter.rect_filled(bar, egui::CornerRadius::same(1), theme::ACCENT);
    }

    // Text colours (theme-aware)
    let text_color = if selected {
        theme::ACCENT
    } else if hovered {
        theme::ACCENT_HOVER
    } else {
        theme::muted_color(dark)
    };

    let icon_pos = egui::pos2(rect.left() + 12.0, rect.center().y);
    painter.text(
        icon_pos,
        egui::Align2::LEFT_CENTER,
        icon,
        egui::FontId::proportional(14.0),
        text_color,
    );

    let label_pos = egui::pos2(rect.left() + 32.0, rect.center().y);
    painter.text(
        label_pos,
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        text_color,
    );
}

// ─── Main Content ────────────────────────────────────────────────────────────

/// Draw the main content area based on the active panel.
fn draw_main_panel(ctx: &egui::Context, app: &mut MagicXApp) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().inner_margin(egui::Margin::same(14)))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match app.active_panel {
                Panel::Dashboard => panels::dashboard::draw(ui, app),
                Panel::Clean => panels::clean::draw(ui, app),
                Panel::Monitor => panels::monitor::draw(ui, app),
                Panel::Processes => panels::processes::draw(ui, app),
                Panel::Settings => panels::settings::draw(ui, app),
            });
        });
}
