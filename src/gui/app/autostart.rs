//! The Start with Windows switch: reading and changing the autostart task
//! on a worker thread, so `schtasks.exe` never blocks the window.

use std::sync::mpsc;

use eframe::egui;

use super::MagicXApp;
use crate::integration::autostart::{self, AutostartState};

/// What the switch knows about the autostart task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutostartView {
    /// The first read has not finished yet.
    Checking,
    /// The task's state as last read or set.
    Known(AutostartState),
    /// The task could not be read.
    Unknown(String),
}

/// A worker's answer.
enum AutostartMsg {
    /// The startup read finished.
    Read(Result<AutostartState, String>),
    /// A change the user asked for finished; `wanted` is what they asked.
    Changed {
        /// Whether the user turned autostart on.
        wanted: bool,
        /// The task's state afterwards, or why the change failed.
        result: Result<AutostartState, String>,
    },
}

/// The switch's state plus the channel its workers answer on.
pub struct AutostartUi {
    /// What the switch shows.
    pub view: AutostartView,
    /// Whether a change is in progress (the switch is disabled meanwhile).
    pub busy: bool,
    /// Sender cloned into each worker.
    tx: mpsc::Sender<AutostartMsg>,
    /// Answers from the workers.
    rx: mpsc::Receiver<AutostartMsg>,
}

impl AutostartUi {
    /// Start reading the task in the background. Reading never changes the
    /// task, except for upgrading this copy's task from an older version.
    pub fn start(ctx: &egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let ui = Self {
            view: AutostartView::Checking,
            busy: false,
            tx,
            rx,
        };
        ui.spawn(ctx, || {
            AutostartMsg::Read(autostart::state_with_upgrade().map_err(|e| format!("{e:#}")))
        });
        ui
    }

    /// Run `work` on a worker thread and wake the UI when it answers.
    fn spawn(&self, ctx: &egui::Context, work: impl FnOnce() -> AutostartMsg + Send + 'static) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("gui-autostart".into())
            .spawn(move || {
                drop(tx.send(work()));
                ctx.request_repaint();
            });
        if let Err(e) = spawned {
            drop(self.tx.send(AutostartMsg::Read(Err(format!(
                "failed to start the autostart worker: {e}"
            )))));
        }
    }
}

impl MagicXApp {
    /// Turn autostart on for this copy, or off, in the background.
    pub fn request_autostart(&mut self, ctx: &egui::Context, wanted: bool) {
        if self.autostart.busy {
            return;
        }
        self.autostart.busy = true;
        self.autostart.spawn(ctx, move || {
            let result = autostart::set_enabled(wanted)
                .and_then(|()| autostart::state())
                .map_err(|e| format!("{e:#}"));
            AutostartMsg::Changed { wanted, result }
        });
    }

    /// Apply worker answers to the switch and report the outcome of changes.
    pub(super) fn poll_autostart(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.autostart.rx.try_recv() {
            match msg {
                AutostartMsg::Read(result) => {
                    self.autostart.view =
                        result.map_or_else(AutostartView::Unknown, AutostartView::Known);
                }
                AutostartMsg::Changed { wanted, result } => {
                    self.autostart.busy = false;
                    match result {
                        Ok(state) => {
                            let text = if wanted {
                                crate::strings::gui::settings::MSG_AUTOSTART_ON
                            } else {
                                crate::strings::gui::settings::MSG_AUTOSTART_OFF
                            };
                            self.autostart.view = AutostartView::Known(state);
                            self.show_settings_status(text.to_owned(), false);
                        }
                        Err(e) => {
                            self.show_settings_status(
                                format!("Couldn\u{2019}t change autostart: {e}"),
                                true,
                            );
                            // The switch stays where it was; read the task
                            // again in case the failure changed it anyway.
                            self.autostart.spawn(ctx, || {
                                AutostartMsg::Read(autostart::state().map_err(|e| format!("{e:#}")))
                            });
                        }
                    }
                }
            }
        }
    }
}
