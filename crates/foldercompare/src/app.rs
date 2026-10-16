//! The main application: owns the settings, the view state and the background workers.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use fc_core::compare::{CompareError, DiffEngine, DiffResult};
use fc_core::model::{ContentCompareMode, ScanProgress};
use fc_core::ops;
use tracing::{error, info};

use crate::cli::CommandLineOptions;
use crate::dialogs::{ConfirmAction, Dialog, Outcome};
use crate::settings::{self, AppSettings, Profile, WindowSettings};
use crate::state::{Filters, MainState, format_count, format_size};

const MAX_RECENT_PATHS: usize = 20;
const SEARCH_DELAY: Duration = Duration::from_millis(150);
/// Workers recurse per folder level.
pub const WORKER_STACK: usize = 64 * 1024 * 1024;
pub const INITIAL_STATUS: &str = "Choose two folders and press Compare (F5).";
/// Default widths of the 7 result columns.
pub const DEFAULT_COLUMNS: [f32; 7] = [320.0, 80.0, 135.0, 36.0, 320.0, 80.0, 135.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    Idle,
    Scanning,
    Operating,
}

/// Messages from worker threads to the UI thread.
pub enum Msg {
    ScanProgress {
        job: u64,
        progress: ScanProgress,
    },
    CompareDone {
        job: u64,
        outcome: Box<Result<DiffResult, CompareError>>,
        elapsed: Duration,
    },
    /// A worker panicked; the message is shown and the app goes back to idle.
    WorkerFailed {
        job: u64,
        title: String,
        message: String,
    },
}

pub struct FolderCompareApp {
    pub settings: AppSettings,
    settings_path: PathBuf,
    pub state: MainState,

    // Folder bar
    pub left_path: String,
    pub right_path: String,
    pub left_error: Option<String>,
    pub right_error: Option<String>,
    pub root_error: Option<String>,
    pub recent: Vec<String>,

    // Toolbar inputs
    pub search_text: String,
    search_changed: Option<Instant>,
    pub exclude: String,
    pub include: String,
    pub selected_profile: Option<usize>,
    /// A profile waiting for the "replace?" confirmation.
    pending_profile: Option<Profile>,

    // Status
    pub status_text: String,
    pub counts_text: String,
    pub busy: Busy,
    pub progress: f32,
    pub progress_indeterminate: bool,
    pub file_progress: f32,
    cancel: Arc<AtomicBool>,

    // Workers
    pub tx: Sender<Msg>,
    rx: Receiver<Msg>,
    job: u64,

    // Session overrides from the command line (not saved)
    force_non_recursive: bool,
    force_content: Option<ContentCompareMode>,
    compare_on_startup: bool,

    pub dialogs: Vec<Dialog>,
    pub columns: [f32; 7],
    pub list: crate::result_list::ListView,
    /// Screen rects of the path boxes (drop targets) and the box a file is dragged over.
    pub left_box: Option<egui::Rect>,
    pub right_box: Option<egui::Rect>,
    pub drop_hover: Option<bool>,
    /// Normal (not maximized) window bounds, recorded every frame for saving.
    window_bounds: Option<WindowSettings>,
    first_frame: bool,
    pub ctx: egui::Context,
}

impl FolderCompareApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        options: CommandLineOptions,
        settings: AppSettings,
    ) -> Self {
        crate::theme::install(&cc.egui_ctx);
        let settings_path = settings::settings_path();
        let (tx, rx) = mpsc::channel();
        let mut columns = DEFAULT_COLUMNS;
        if let Some(w) = &settings.column_widths
            && w.len() == 7
            && w.iter().all(|&x| x > 10.0)
        {
            for (c, &x) in columns.iter_mut().zip(w) {
                *c = x as f32;
            }
        }
        let mut app = Self {
            state: MainState::new(Filters::from(&settings.filters), settings.folders_first),
            left_path: settings.last_left.clone(),
            right_path: settings.last_right.clone(),
            left_error: None,
            right_error: None,
            root_error: None,
            recent: settings.recent_paths.clone(),
            search_text: String::new(),
            search_changed: None,
            exclude: settings.exclude_patterns.clone(),
            include: settings.include_patterns.clone(),
            selected_profile: None,
            pending_profile: None,
            status_text: INITIAL_STATUS.to_string(),
            counts_text: String::new(),
            busy: Busy::Idle,
            progress: 0.0,
            progress_indeterminate: false,
            file_progress: 0.0,
            cancel: Arc::new(AtomicBool::new(false)),
            tx,
            rx,
            job: 0,
            force_non_recursive: options.no_recursive,
            force_content: options.content_compare,
            compare_on_startup: options.compare_on_startup(),
            dialogs: Vec::new(),
            columns,
            list: Default::default(),
            left_box: None,
            right_box: None,
            drop_hover: None,
            window_bounds: settings.window.clone(),
            first_frame: true,
            ctx: cc.egui_ctx.clone(),
            settings,
            settings_path,
        };
        if let Some(l) = options.left {
            app.left_path = l;
        }
        if let Some(r) = options.right {
            app.right_path = r;
        }
        app.validate_paths();
        app
    }

    pub fn is_busy(&self) -> bool {
        self.busy != Busy::Idle
    }

    // ---- Paths -------------------------------------------------------------------------------

    pub fn validate_paths(&mut self) {
        self.left_error = path_error(&self.left_path);
        self.right_error = path_error(&self.right_path);
        self.root_error = if self.left_error.is_none()
            && self.right_error.is_none()
            && !self.left_path.trim().is_empty()
            && !self.right_path.trim().is_empty()
        {
            ops::validate_roots(&self.left_path, &self.right_path)
        } else {
            None
        };
    }

    pub fn can_compare(&self) -> bool {
        !self.is_busy()
            && !self.left_path.trim().is_empty()
            && !self.right_path.trim().is_empty()
            && self.left_error.is_none()
            && self.right_error.is_none()
            && self.root_error.is_none()
    }

    pub fn browse(&mut self, left: bool) {
        let current = if left {
            &self.left_path
        } else {
            &self.right_path
        };
        let mut dialog = rfd::FileDialog::new();
        let start = Path::new(current.trim());
        if start.is_dir() {
            dialog = dialog.set_directory(start);
        }
        if let Some(folder) = dialog.pick_folder() {
            let text = folder.to_string_lossy().into_owned();
            if left {
                self.left_path = text;
            } else {
                self.right_path = text;
            }
            self.validate_paths();
        }
    }

    pub fn swap(&mut self) {
        std::mem::swap(&mut self.left_path, &mut self.right_path);
        self.validate_paths();
        if self.state.result.is_some() && self.can_compare() {
            self.start_compare();
        }
    }

    /// A dropped file or folder: a file is replaced by its parent folder.
    pub fn drop_path(&mut self, left: bool, path: &Path) {
        let folder = if path.is_dir() {
            Some(path.to_path_buf())
        } else if path.is_file() {
            path.parent().map(Path::to_path_buf)
        } else {
            None
        };
        if let Some(f) = folder {
            let text = f.to_string_lossy().into_owned();
            if left {
                self.left_path = text;
            } else {
                self.right_path = text;
            }
            self.validate_paths();
        }
    }

    fn add_recent(&mut self, path: &str) {
        let path = path.trim().to_string();
        let ic = fc_core::compare::PathComparison::IgnoreCase;
        self.recent.retain(|p| !ic.eq(p, &path));
        self.recent.insert(0, path);
        self.recent.truncate(MAX_RECENT_PATHS);
    }

    // ---- Search and filters ------------------------------------------------------------------

    pub fn search_edited(&mut self) {
        self.search_changed = Some(Instant::now());
        self.ctx.request_repaint_after(SEARCH_DELAY);
    }

    fn apply_pending_search(&mut self) {
        if let Some(t) = self.search_changed {
            if t.elapsed() >= SEARCH_DELAY {
                self.search_changed = None;
                self.state.search = self.search_text.clone();
                self.state.rebuild_rows(true);
            } else {
                self.ctx.request_repaint_after(SEARCH_DELAY - t.elapsed());
            }
        }
    }

    pub fn filters_changed(&mut self) {
        self.state.rebuild_rows(true);
    }

    // ---- Settings ----------------------------------------------------------------------------

    /// Copies the view state into the settings and saves them.
    pub fn save_settings(&mut self) {
        let s = &mut self.settings;
        s.last_left = self.left_path.clone();
        s.last_right = self.right_path.clone();
        s.exclude_patterns = self.exclude.clone();
        s.include_patterns = self.include.clone();
        self.state.filters.write_to(&mut s.filters);
        s.folders_first = self.state.folders_first;
        s.recent_paths = self.recent.clone();
        settings::save(&self.settings_path, s);
    }

    pub fn save_window_state(&mut self) {
        self.settings.window = self.window_bounds.clone();
        self.settings.column_widths = Some(self.columns.iter().map(|&c| c as f64).collect());
        self.save_settings();
    }

    // ---- Compare -----------------------------------------------------------------------------

    pub fn start_compare(&mut self) {
        if self.is_busy() {
            return;
        }
        if let Some(e) = ops::validate_roots(&self.left_path, &self.right_path) {
            self.root_error = Some(e);
            return;
        }
        // Capture the paths first: they are trimmed and moved to the top of the recent list.
        let left = self.left_path.trim().to_string();
        let right = self.right_path.trim().to_string();
        self.add_recent(&right);
        self.add_recent(&left);
        self.left_path = left.clone();
        self.right_path = right.clone();
        self.save_settings();

        let mut options = self.settings.to_compare_options();
        if self.force_non_recursive {
            options.recursive = false;
        }
        if let Some(mode) = self.force_content {
            options.content_compare = mode;
        }

        self.job += 1;
        let job = self.job;
        self.cancel = Arc::new(AtomicBool::new(false));
        self.busy = Busy::Scanning;
        self.progress = 0.0;
        self.progress_indeterminate = true;
        self.status_text = "Scanning...".to_string();

        let (tx, ctx, cancel) = (self.tx.clone(), self.ctx.clone(), self.cancel.clone());
        spawn_worker(
            self.tx.clone(),
            self.ctx.clone(),
            job,
            "Compare failed",
            move || {
                let started = Instant::now();
                let report = |p: ScanProgress| {
                    let _ = tx.send(Msg::ScanProgress { job, progress: p });
                    ctx.request_repaint();
                };
                let outcome =
                    DiffEngine::new().compare(&left, &right, &options, Some(&report), &cancel);
                let _ = tx.send(Msg::CompareDone {
                    job,
                    outcome: Box::new(outcome),
                    elapsed: started.elapsed(),
                });
                ctx.request_repaint();
            },
        );
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn on_compare_done(&mut self, outcome: Result<DiffResult, CompareError>, elapsed: Duration) {
        self.busy = Busy::Idle;
        self.progress_indeterminate = false;
        self.progress = 0.0;
        match outcome {
            Ok(result) => {
                info!(
                    "Compared {} and {} in {:.1} s",
                    result.left_root.display(),
                    result.right_root.display(),
                    elapsed.as_secs_f64()
                );
                let root_error = (result.tree.root().status == fc_core::model::DiffStatus::Error)
                    .then(|| result.tree.root().error.clone().unwrap_or_default());
                let items = result.tree.descendants(fc_core::model::NodeId::ROOT).len();
                self.state.set_result(result);
                self.update_counts();
                self.status_text = match root_error {
                    Some(e) => format!("Could not read a root folder: {e}"),
                    None => format!(
                        "Compared {} items in {:.1} s.",
                        format_count(items as u64),
                        elapsed.as_secs_f64()
                    ),
                };
            }
            Err(CompareError::Cancelled) => self.status_text = "Scan cancelled.".to_string(),
            Err(CompareError::InvalidRoots(e)) => {
                error!("Compare failed: {e}");
                self.dialogs.push(Dialog::error("Compare failed", e));
                self.status_text = "Compare failed.".to_string();
            }
        }
    }

    fn on_scan_progress(&mut self, p: ScanProgress) {
        if p.bytes_total == 0 {
            self.status_text = format!(
                "Scanning... {} items  {}",
                format_count(p.items_scanned),
                p.current_folder
            );
            return;
        }
        self.progress_indeterminate = false;
        self.progress = p.bytes_compared as f32 / p.bytes_total as f32;
        self.status_text = format!(
            "Verifying contents... {:.0}% ({} of {})  {}",
            self.progress * 100.0,
            format_size(p.bytes_compared),
            format_size(p.bytes_total),
            p.current_folder
        );
    }

    pub fn update_counts(&mut self) {
        self.counts_text = self.state.counts_text();
    }

    fn poll_workers(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::ScanProgress { job, progress } if job == self.job => {
                    self.on_scan_progress(progress)
                }
                Msg::ScanProgress { .. } => {}
                Msg::CompareDone {
                    job,
                    outcome,
                    elapsed,
                } if job == self.job => self.on_compare_done(*outcome, elapsed),
                Msg::CompareDone { .. } => {}
                Msg::WorkerFailed {
                    job,
                    title,
                    message,
                } => {
                    if job == self.job {
                        self.busy = Busy::Idle;
                        self.progress_indeterminate = false;
                        self.status_text = format!("{title}.");
                    }
                    self.dialogs.push(Dialog::error(title, message));
                }
            }
        }
    }

    // ---- Profiles ----------------------------------------------------------------------------

    pub fn apply_profile(&mut self, index: usize) {
        let Some(p) = self.settings.profiles.get(index).cloned() else {
            return;
        };
        self.selected_profile = Some(index);
        self.left_path = p.left_path.clone();
        self.right_path = p.right_path.clone();
        let s = &mut self.settings;
        s.tolerance_seconds = p.tolerance_seconds;
        s.ignore_dst_offset = p.ignore_dst_offset;
        s.content_compare = p.content_compare;
        s.ignore_hidden_and_system = p.ignore_hidden_and_system;
        s.recursive = p.recursive;
        self.exclude = p.exclude_patterns.clone();
        self.include = p.include_patterns.clone();
        self.state.filters = Filters::from(&p.filters);
        self.validate_paths();
        self.state.rebuild_rows(true);
        self.status_text = format!("Profile '{}' loaded. Press F5 to compare.", p.name);
    }

    /// Saves the current folders and options under `name`, asking before replacing an existing profile.
    pub fn save_profile(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let profile = self.current_profile(name);
        let ic = fc_core::compare::PathComparison::IgnoreCase;
        if let Some(i) = self
            .settings
            .profiles
            .iter()
            .position(|p| ic.eq(&p.name, name))
        {
            let existing = self.settings.profiles[i].name.clone();
            self.pending_profile = Some(profile);
            self.dialogs.push(Dialog::confirm(
                "Save profile",
                format!("Replace the existing profile '{existing}'?"),
                ConfirmAction::ReplaceProfile(i),
            ));
            return;
        }
        self.settings.profiles.push(profile);
        self.selected_profile = Some(self.settings.profiles.len() - 1);
        self.save_settings();
    }

    fn current_profile(&self, name: &str) -> Profile {
        let s = &self.settings;
        Profile {
            name: name.to_string(),
            left_path: self.left_path.clone(),
            right_path: self.right_path.clone(),
            tolerance_seconds: s.tolerance_seconds,
            ignore_dst_offset: s.ignore_dst_offset,
            content_compare: s.content_compare,
            ignore_hidden_and_system: s.ignore_hidden_and_system,
            recursive: s.recursive,
            exclude_patterns: self.exclude.clone(),
            include_patterns: self.include.clone(),
            filters: self.state.filters.to_settings(),
            ..Profile::default()
        }
    }

    pub fn delete_profile(&mut self) {
        let Some(i) = self.selected_profile else {
            return;
        };
        let Some(p) = self.settings.profiles.get(i) else {
            return;
        };
        self.dialogs.push(Dialog::confirm(
            "Delete profile",
            format!("Delete the profile '{}'?", p.name),
            ConfirmAction::DeleteProfile(i),
        ));
    }

    fn handle_outcome(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Closed | Outcome::Confirmed(ConfirmAction::None) => {}
            Outcome::Confirmed(ConfirmAction::ReplaceProfile(i)) => {
                if let Some(p) = self.pending_profile.take()
                    && i < self.settings.profiles.len()
                {
                    self.settings.profiles[i] = p;
                    self.selected_profile = Some(i);
                    self.save_settings();
                }
            }
            Outcome::Confirmed(ConfirmAction::DeleteProfile(i)) => {
                if i < self.settings.profiles.len() {
                    self.settings.profiles.remove(i);
                    self.selected_profile = None;
                    self.save_settings();
                }
            }
        }
    }

    // ---- Frame -------------------------------------------------------------------------------

    fn record_window(&mut self, ctx: &egui::Context) {
        let (outer, inner, maximized, minimized) = ctx.input(|i| {
            let v = i.viewport();
            (v.outer_rect, v.inner_rect, v.maximized, v.minimized)
        });
        let maximized = maximized.unwrap_or(false);
        if minimized.unwrap_or(false) {
            return;
        }
        let bounds = self
            .window_bounds
            .get_or_insert_with(WindowSettings::default);
        bounds.maximized = maximized;
        if !maximized && let (Some(o), Some(i)) = (outer, inner) {
            bounds.left = o.min.x as f64;
            bounds.top = o.min.y as f64;
            bounds.width = i.width() as f64;
            bounds.height = i.height() as f64;
        }
    }

    /// Clamps a restored window to the monitor once its size is known.
    fn fit_to_monitor(&self, ctx: &egui::Context) {
        let (outer, monitor) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().monitor_size));
        let (Some(o), Some(m)) = (outer, monitor) else {
            return;
        };
        let x = o.min.x.clamp(0.0, (m.x - o.width()).max(0.0));
        let y = o.min.y.clamp(0.0, (m.y - o.height()).max(0.0));
        if (x - o.min.x).abs() > 1.0 || (y - o.min.y).abs() > 1.0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
        }
    }

    pub fn exit(&self) {
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl FolderCompareApp {
    fn take_dialog_outcome(&mut self, ctx: &egui::Context) {
        if let Some(top) = self.dialogs.last_mut()
            && let Some(outcome) = top.show(ctx)
        {
            self.dialogs.pop();
            self.handle_outcome(outcome);
        }
    }
}

impl eframe::App for FolderCompareApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.first_frame {
            self.first_frame = false;
            self.fit_to_monitor(&ctx);
            if self.compare_on_startup && self.can_compare() {
                self.start_compare();
            }
        }
        self.poll_workers();
        self.apply_pending_search();
        self.record_window(&ctx);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.main_ui(ui);
            self.take_dialog_outcome(&ctx);
        }));
        if let Err(panic) = result {
            let message = crate::panic_message(&panic);
            self.dialogs.push(Dialog::error(
                "FolderCompare",
                format!(
                    "Something went wrong:\n\n{message}\n\nThe details were written to the log folder:\n{}",
                    crate::platform::log_dir().display()
                ),
            ));
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_window_state();
            info!("FolderCompare exiting");
        }
    }
}

/// Runs `work` on a worker thread with a large stack; a panic is reported to the UI as an error.
pub fn spawn_worker(
    tx: Sender<Msg>,
    ctx: egui::Context,
    job: u64,
    title: &'static str,
    work: impl FnOnce() + Send + 'static,
) {
    let spawned = thread::Builder::new()
        .name("worker".into())
        .stack_size(WORKER_STACK)
        .spawn(move || {
            if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
                let _ = tx.send(Msg::WorkerFailed {
                    job,
                    title: title.to_string(),
                    message: crate::panic_message(&panic),
                });
                ctx.request_repaint();
            }
        });
    if let Err(e) = spawned {
        error!("Could not start a worker thread: {e}");
    }
}

fn path_error(path: &str) -> Option<String> {
    let p = path.trim();
    if p.is_empty() {
        return None;
    }
    if p.contains('\0') {
        return Some("Invalid path.".to_string());
    }
    if Path::new(p).is_dir() {
        None
    } else {
        Some("Folder does not exist or is not accessible.".to_string())
    }
}
