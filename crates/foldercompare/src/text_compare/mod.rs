//! Side-by-side text compare: two editable panes kept line-aligned with filler rows, recompared while typing,
//! with navigation between difference sections and copying of sections to the other side.

mod buffer;
mod layout;
mod pane;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use egui::{
    Align, Align2, Button, CursorIcon, Event, FontId, Key, Layout, Modifiers, Rect, Sense, Stroke,
    Ui, ViewportClass, ViewportCommand, ViewportId, pos2, vec2,
};
use fc_core::textdiff::{
    DiffBlockKind, LoadedText, TextComparison, TextDiffOptions, codec, compute_block_copy,
};
use tracing::{error, info};

use self::pane::{Metrics, Pane, band_color};
use crate::settings::{TextCompareSettings, WindowSettings};
use crate::state::format_count;
use crate::theme::{self, icons};

const LARGE_FILE: u64 = 20 * 1024 * 1024;
const RECOMPARE_DELAY: Duration = Duration::from_millis(250);
const OVERVIEW_WIDTH: f32 = 14.0;
const STRIP_WIDTH: f32 = 34.0;
const SCROLLBAR: f32 = 12.0;
const HEADER_HEIGHT: f32 = 40.0;

/// A file pair to open. A side that does not exist yet starts empty.
#[derive(Debug, Clone)]
pub struct TextCompareRequest {
    pub left: PathBuf,
    pub left_exists: bool,
    pub right: PathBuf,
    pub right_exists: bool,
    pub title: String,
    /// Relative path of the row in the main window that is rescanned after a save.
    pub relative_path: Option<String>,
}

/// One side as loaded from disk.
struct TextSide {
    path: PathBuf,
    exists: bool,
    text: LoadedText,
    mtime: Option<SystemTime>,
}

/// The file that is larger than 20 MB, if any (the caller asks before comparing it).
pub fn large_file(request: &TextCompareRequest) -> Option<PathBuf> {
    [
        (&request.left, request.left_exists),
        (&request.right, request.right_exists),
    ]
    .into_iter()
    .find(|(p, e)| *e && std::fs::metadata(p).is_ok_and(|m| m.len() > LARGE_FILE))
    .map(|(p, _)| p.clone())
}

pub fn large_file_message(path: &Path) -> String {
    format!(
        "{}\n\nis larger than 20 MB. Comparing it may be slow. Continue?",
        path.display()
    )
}

/// Loads both sides. A missing side gets the other side's encoding and line endings.
fn load(request: &TextCompareRequest) -> Result<(TextSide, TextSide), String> {
    let read = |p: &Path, exists: bool| -> Result<Option<LoadedText>, String> {
        if exists {
            codec::load(p)
                .map(Some)
                .map_err(|e| fc_core::compare::io_message(&e))
        } else {
            Ok(None)
        }
    };
    let l = read(&request.left, request.left_exists)?;
    let r = read(&request.right, request.right_exists)?;
    if let Some(binary) = [(&l, &request.left), (&r, &request.right)]
        .into_iter()
        .find(|(t, _)| t.as_ref().is_some_and(|t| t.is_binary))
        .map(|(_, p)| p)
    {
        return Err(format!(
            "{}\n\nis not a text file and cannot be compared as text.",
            binary.display()
        ));
    }
    let side = |path: &Path, text: Option<LoadedText>, other: Option<&LoadedText>| match text {
        Some(t) => TextSide {
            path: path.to_path_buf(),
            exists: true,
            mtime: std::fs::metadata(path).and_then(|m| m.modified()).ok(),
            text: t,
        },
        None => TextSide {
            path: path.to_path_buf(),
            exists: false,
            text: LoadedText::empty_like(other.unwrap_or(&LoadedText::empty())),
            mtime: None,
        },
    };
    let left = side(&request.left, l.clone(), r.as_ref());
    let right = side(&request.right, r, l.as_ref());
    Ok((left, right))
}

#[derive(Debug)]
enum TextDialog {
    Message {
        title: String,
        text: String,
    },
    /// The file changed on disk since it was loaded: overwrite it?
    DiskChanged {
        side: usize,
        then_close: bool,
    },
    Reload,
    Close,
}

struct ComparisonResult {
    job: u64,
    versions: (u64, u64),
    result: Result<TextComparison, String>,
}

pub struct TextCompareWindow {
    pub viewport: ViewportId,
    request: TextCompareRequest,
    panes: [Pane; 2],
    focused: usize,
    comparison: Option<Arc<TextComparison>>,
    /// Buffer versions the comparison was computed for.
    comparison_versions: (u64, u64),
    comparison_id: u64,
    job: u64,
    results_tx: mpsc::Sender<ComparisonResult>,
    results: Receiver<ComparisonResult>,
    recompare_at: Option<Instant>,
    options: TextDiffOptions,
    show_whitespace: bool,
    current_block: Option<usize>,
    v_offset: f32,
    h_offset: f32,
    viewport_height: f32,
    metrics: Metrics,
    status_error: Option<String>,
    section_text: String,
    dialog: Option<TextDialog>,
    scroll_to_caret: bool,
    dragging: Option<usize>,
    started: bool,
    /// Set after a successful save; the main window rescans the row.
    pub saved: bool,
    /// The window is gone; the owner drops it.
    pub closed: bool,
    /// The main window wants to exit: ask about unsaved changes, then close.
    pub close_requested: bool,
    /// The user cancelled a close that the main window asked for.
    pub close_cancelled: bool,
    pub settings: TextCompareSettings,
    bounds: Option<WindowSettings>,
    ctx: egui::Context,
}

impl TextCompareWindow {
    /// Loads the files; errors are messages for the user.
    pub fn open(
        ctx: &egui::Context,
        request: TextCompareRequest,
        settings: TextCompareSettings,
    ) -> Result<Self, String> {
        let (left, right) = load(&request)?;
        info!(
            "Opening text compare for {} and {}",
            request.left.display(),
            request.right.display()
        );
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        let mut w = Self {
            viewport: ViewportId::from_hash_of(("text-compare", id)),
            panes: [
                Pane::new(true, left.path, left.exists, left.text, left.mtime),
                Pane::new(false, right.path, right.exists, right.text, right.mtime),
            ],
            request,
            focused: 0,
            comparison: None,
            comparison_versions: (u64::MAX, u64::MAX),
            comparison_id: 0,
            job: 0,
            results_tx: tx,
            results: rx,
            recompare_at: None,
            options: TextDiffOptions {
                ignore_whitespace: settings.ignore_whitespace,
                ignore_case: settings.ignore_case,
            },
            show_whitespace: settings.show_whitespace,
            current_block: None,
            v_offset: 0.0,
            h_offset: 0.0,
            viewport_height: 400.0,
            metrics: Metrics {
                font: 13.0,
                line_height: 17.0,
                char_width: 7.8,
            },
            status_error: None,
            section_text: String::new(),
            dialog: None,
            scroll_to_caret: false,
            dragging: None,
            started: false,
            saved: false,
            closed: false,
            close_requested: false,
            close_cancelled: false,
            bounds: settings.window.clone(),
            settings,
            ctx: ctx.clone(),
        };
        w.recompare_now();
        Ok(w)
    }

    pub fn title(&self) -> String {
        let dirty = self.panes.iter().any(Pane::is_dirty);
        format!(
            "{}{} - Text compare",
            if dirty { "* " } else { "" },
            self.request.title
        )
    }

    pub fn is_dirty(&self) -> bool {
        self.panes.iter().any(Pane::is_dirty)
    }

    pub fn relative_path(&self) -> Option<&str> {
        self.request.relative_path.as_deref()
    }

    /// The native window: 1200×800 (or the saved bounds), at least 600×300.
    pub fn viewport_builder(&self) -> egui::ViewportBuilder {
        let mut b = egui::ViewportBuilder::default()
            .with_title(self.title())
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([600.0, 300.0]);
        if let Some(w) = &self.settings.window
            && w.width >= 300.0
            && w.height >= 200.0
        {
            b = b
                .with_inner_size([w.width as f32, w.height as f32])
                .with_position([w.left as f32, w.top as f32])
                .with_maximized(w.maximized);
        }
        if let Some(icon) = crate::app_icon() {
            b = b.with_icon(icon);
        }
        b
    }

    // ---- Comparison ------------------------------------------------------------------------------

    fn versions(&self) -> (u64, u64) {
        (self.panes[0].buffer.version, self.panes[1].buffer.version)
    }

    fn schedule_recompare(&mut self) {
        self.recompare_at = Some(Instant::now() + RECOMPARE_DELAY);
        self.ctx
            .request_repaint_after_for(RECOMPARE_DELAY, self.viewport);
    }

    /// Starts a comparison on a worker; stale results are dropped.
    fn recompare_async(&mut self) {
        self.recompare_at = None;
        self.job += 1;
        let job = self.job;
        let versions = self.versions();
        let (left, right) = (
            self.panes[0].buffer.lines_text(),
            self.panes[1].buffer.lines_text(),
        );
        let options = self.options;
        let tx = self.results_tx.clone();
        let (ctx, viewport) = (self.ctx.clone(), self.viewport);
        std::thread::spawn(move || {
            let result =
                std::panic::catch_unwind(|| TextComparison::compute_lines(&left, &right, options))
                    .map_err(|p| crate::panic_message(&p));
            let _ = tx.send(ComparisonResult {
                job,
                versions,
                result,
            });
            ctx.request_repaint_of(viewport);
        });
    }

    /// Recompares synchronously, e.g. right before or after copying a section.
    fn recompare_now(&mut self) {
        self.recompare_at = None;
        self.job += 1;
        let c = TextComparison::compute_lines(
            &self.panes[0].buffer.lines_text(),
            &self.panes[1].buffer.lines_text(),
            self.options,
        );
        let v = self.versions();
        self.apply(c, v);
    }

    fn apply(&mut self, c: TextComparison, versions: (u64, u64)) {
        self.comparison = Some(Arc::new(c));
        self.comparison_versions = versions;
        self.comparison_id += 1;
        self.status_error = None;
        self.update_current_block();
    }

    fn poll_results(&mut self) {
        while let Ok(r) = self.results.try_recv() {
            if r.job != self.job {
                continue;
            }
            match r.result {
                Ok(c) => self.apply(c, r.versions),
                Err(e) => {
                    error!("Text comparison failed: {e}");
                    self.status_error = Some(format!("Comparison failed: {e}"));
                }
            }
        }
        if self.recompare_at.is_some_and(|t| Instant::now() >= t) {
            self.recompare_async();
        }
    }

    fn comparison_is_current(&self) -> bool {
        self.comparison.is_some() && self.comparison_versions == self.versions()
    }

    fn diff_count_text(&self) -> String {
        if let Some(e) = &self.status_error {
            return e.clone();
        }
        let Some(c) = &self.comparison else {
            return String::new();
        };
        match c.difference_count {
            0 if c
                .blocks
                .iter()
                .any(|b| b.kind == DiffBlockKind::Unimportant) =>
            {
                "Only unimportant differences".into()
            }
            0 => "No differences".into(),
            1 => "1 difference section".into(),
            n => format!("{} difference sections", format_count(n as u64)),
        }
    }

    // ---- Current section and navigation --------------------------------------------------------

    /// The non-equal block at the caret, including a block without lines on this side inserted right there.
    fn block_for_caret(&self, side: usize) -> Option<usize> {
        let c = self.comparison.as_ref()?;
        let p = &self.panes[side];
        let left = p.is_left;
        let line = p.caret.line;
        let n = c.block_at(left, line);
        if let Some(n) = n
            && c.blocks[n].kind != DiffBlockKind::Equal
        {
            return Some(n);
        }
        let previous = match n {
            Some(n) => n.checked_sub(1)?,
            None => c.blocks.len().checked_sub(1)?,
        };
        let b = &c.blocks[previous];
        let (start, count) = (b.start(left), b.count(left));
        (b.kind != DiffBlockKind::Equal
            && count == 0
            && (start == line || (n.is_none() && start == line + 1)))
            .then_some(previous)
    }

    fn update_current_block(&mut self) {
        self.current_block = self.block_for_caret(self.focused);
        self.section_text = match (&self.comparison, self.current_block) {
            (Some(c), Some(b)) if c.blocks[b].is_difference() => {
                let index = c.blocks[..=b].iter().filter(|b| b.is_difference()).count();
                format!(
                    "Section {} of {}",
                    format_count(index as u64),
                    format_count(c.difference_count as u64)
                )
            }
            _ => String::new(),
        };
    }

    /// The block navigation starts from: the current section, else the block at the caret.
    fn anchor(&self) -> Option<usize> {
        if self.current_block.is_some() {
            return self.current_block;
        }
        let p = &self.panes[self.focused];
        self.comparison.as_ref()?.block_at(p.is_left, p.caret.line)
    }

    fn next_difference(&self, from: Option<usize>) -> Option<usize> {
        let c = self.comparison.as_ref()?;
        let start = from.map_or(0, |f| f + 1);
        (start..c.blocks.len()).find(|&n| c.blocks[n].is_difference())
    }

    fn previous_difference(&self, from: Option<usize>) -> Option<usize> {
        let c = self.comparison.as_ref()?;
        let end = from.unwrap_or(c.blocks.len());
        (0..end).rev().find(|&n| c.blocks[n].is_difference())
    }

    fn go_to(&mut self, block: Option<usize>) {
        let (Some(c), Some(block)) = (self.comparison.clone(), block) else {
            return;
        };
        let b = c.blocks[block];
        self.panes[0].set_caret_line(b.left_start.min(self.panes[0].buffer.line_count() - 1));
        self.panes[1].set_caret_line(b.right_start.min(self.panes[1].buffer.line_count() - 1));
        self.update_current_block();
        self.current_block = Some(block);
        self.update_current_block_text(block);
        // Put the section in the upper third, unless it is already fully visible.
        let lh = self.metrics.line_height;
        let top = c.row_of(block) as f32 * lh;
        let bottom = top + b.height() as f32 * lh;
        if top < self.v_offset || bottom > self.v_offset + self.viewport_height {
            self.v_offset = (top - self.viewport_height / 3.0).max(0.0);
            self.clamp_scroll();
        }
    }

    fn update_current_block_text(&mut self, block: usize) {
        if let Some(c) = &self.comparison
            && c.blocks[block].is_difference()
        {
            let index = c.blocks[..=block]
                .iter()
                .filter(|b| b.is_difference())
                .count();
            self.section_text = format!(
                "Section {} of {}",
                format_count(index as u64),
                format_count(c.difference_count as u64)
            );
        }
    }

    // ---- Copying sections ----------------------------------------------------------------------

    /// The sections a copy acts on: those touched by a multi-line selection, else the one at the caret.
    fn selected_blocks(&self) -> Vec<usize> {
        let Some(c) = &self.comparison else {
            return Vec::new();
        };
        let p = &self.panes[self.focused];
        if p.has_selection() {
            let (a, b) = p.selection();
            let first = a.line;
            let mut last = b.line;
            if b.col == 0 && last > first {
                last -= 1;
            }
            if last > first {
                return (0..c.blocks.len())
                    .filter(|&n| {
                        let blk = &c.blocks[n];
                        if blk.kind == DiffBlockKind::Equal {
                            return false;
                        }
                        let (start, count) = (blk.start(p.is_left), blk.count(p.is_left));
                        if count > 0 {
                            start <= last && start + count > first
                        } else {
                            start > first && start <= last
                        }
                    })
                    .collect();
            }
        }
        self.current_block.into_iter().collect()
    }

    fn copy(&mut self, to_right: bool, blocks: Option<Vec<usize>>) {
        if !self.comparison_is_current() {
            self.recompare_now();
        }
        let Some(c) = self.comparison.clone() else {
            return;
        };
        let mut blocks = blocks.unwrap_or_else(|| self.selected_blocks());
        if blocks.is_empty() {
            self.section_text = "Place the cursor in a difference section first.".into();
            return;
        }
        blocks.sort_unstable();
        let (s, t) = if to_right { (0, 1) } else { (1, 0) };
        let newline = self.panes[t].format.newline;
        // Edits are computed against the unchanged target and applied bottom-up, so offsets stay valid.
        let edits: Vec<_> = blocks
            .iter()
            .rev()
            .map(|&n| {
                compute_block_copy(
                    &self.panes[s].buffer,
                    &self.panes[t].buffer,
                    &c.blocks[n],
                    to_right,
                    newline,
                )
            })
            .collect();
        let first = c.blocks[blocks[0]];
        let target = &mut self.panes[t];
        let caret = (target.anchor, target.caret);
        target.buffer.begin_group(caret); // one undo step for the whole copy
        for e in edits {
            let a = target.buffer.pos_of(e.offset);
            let b = target.buffer.pos_of(e.offset + e.length);
            target.buffer.replace(a, b, &e.text, caret, false);
        }
        target.buffer.end_group(caret);
        self.recompare_now();
        // Stay where the copy happened (its start lines are unchanged), so Ctrl+N continues from here.
        self.panes[0].set_caret_line(first.left_start.min(self.panes[0].buffer.line_count() - 1));
        self.panes[1].set_caret_line(first.right_start.min(self.panes[1].buffer.line_count() - 1));
        self.update_current_block();
    }

    // ---- Caret sync ------------------------------------------------------------------------------

    /// Moves the other caret to the matching line, without scrolling.
    fn sync_other_caret(&mut self) {
        let Some(c) = self.comparison.clone() else {
            return;
        };
        let (p, o) = (self.focused, 1 - self.focused);
        let left = self.panes[p].is_left;
        let line = self.panes[p].caret.line;
        let partner = c
            .partner_line(left, line)
            .or_else(|| c.block_at(left, line).map(|n| c.blocks[n].start(!left)));
        if let Some(target) = partner
            && target < self.panes[o].buffer.line_count()
        {
            self.panes[o].set_caret_line(target);
        }
    }

    // ---- Saving, reloading, closing -----------------------------------------------------------

    fn save(&mut self, side: usize, check_disk: bool) -> bool {
        let p = &self.panes[side];
        if check_disk
            && p.exists
            && let Some(loaded) = p.loaded_mtime
            && std::fs::metadata(&p.path)
                .and_then(|m| m.modified())
                .is_ok_and(|m| m != loaded)
        {
            self.dialog = Some(TextDialog::DiskChanged {
                side,
                then_close: false,
            });
            return false;
        }
        let p = &mut self.panes[side];
        let result = (|| -> std::io::Result<()> {
            if let Some(dir) = p.path.parent()
                && !dir.as_os_str().is_empty()
            {
                std::fs::create_dir_all(dir)?;
            }
            codec::save(&p.path, &p.buffer.text(), &p.format)
        })();
        match result {
            Ok(()) => {
                info!("Saved {} from text compare", p.path.display());
                p.exists = true;
                p.loaded_mtime = std::fs::metadata(&p.path).and_then(|m| m.modified()).ok();
                p.buffer.mark_saved();
                self.saved = true;
                true
            }
            Err(e) => {
                error!("Could not save {}: {e}", p.path.display());
                self.dialog = Some(TextDialog::Message {
                    title: "Save".into(),
                    text: format!(
                        "Could not save {}:\n\n{}",
                        p.path.display(),
                        fc_core::compare::io_message(&e)
                    ),
                });
                false
            }
        }
    }

    fn save_all(&mut self) -> bool {
        let mut ok = true;
        for side in 0..2 {
            if self.panes[side].is_dirty() && self.dialog.is_none() {
                ok &= self.save(side, true);
            }
        }
        ok && self.dialog.is_none()
    }

    fn reload(&mut self, confirmed: bool) {
        if !confirmed && self.is_dirty() {
            self.dialog = Some(TextDialog::Reload);
            return;
        }
        let mut request = self.request.clone();
        request.left_exists = self.panes[0].path.is_file();
        request.right_exists = self.panes[1].path.is_file();
        match load(&request) {
            Ok((l, r)) => {
                self.panes[0] = Pane::new(true, l.path, l.exists, l.text, l.mtime);
                self.panes[1] = Pane::new(false, r.path, r.exists, r.text, r.mtime);
                self.recompare_now();
            }
            Err(e) => {
                self.dialog = Some(TextDialog::Message {
                    title: "Reload".into(),
                    text: e,
                })
            }
        }
    }

    /// Close (Esc, title bar): asks about unsaved changes first.
    fn request_close(&mut self) {
        if self.is_dirty() {
            self.dialog = Some(TextDialog::Close);
        } else {
            self.finish_close();
        }
    }

    pub fn finish_close(&mut self) {
        self.settings.ignore_whitespace = self.options.ignore_whitespace;
        self.settings.ignore_case = self.options.ignore_case;
        self.settings.show_whitespace = self.show_whitespace;
        self.settings.window = self.bounds.clone();
        self.closed = true;
    }

    // ---- UI ----------------------------------------------------------------------------------------

    pub fn ui(&mut self, ui: &mut Ui, class: ViewportClass) {
        if self.closed {
            return;
        }
        let ctx = ui.ctx().clone();
        self.poll_results();
        self.record_bounds(&ctx);
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.is_dirty() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.dialog.get_or_insert(TextDialog::Close);
            } else {
                self.finish_close();
                return;
            }
        }
        if self.close_requested && self.dialog.is_none() {
            self.close_requested = false;
            self.request_close();
        }
        self.update_metrics(&ctx);

        let (title, window_id) = (self.title(), egui::Id::new(self.viewport));
        let mut body = |ui: &mut Ui| {
            if self.dialog.is_none() {
                self.handle_shortcuts(ui);
            }
            egui::Panel::top("tc-menu").show(ui, |ui| self.menu(ui));
            egui::Panel::top("tc-toolbar")
                .frame(
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(6, 3))
                        .fill(ui.visuals().panel_fill),
                )
                .show(ui, |ui| self.toolbar(ui));
            egui::Panel::bottom("tc-status")
                .frame(
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(6, 3))
                        .fill(ui.visuals().panel_fill),
                )
                .show(ui, |ui| self.status(ui));
            egui::Panel::bottom("tc-details")
                .frame(
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(6, 3))
                        .fill(theme::EDITOR_BACKGROUND),
                )
                .show(ui, |ui| self.details(ui));
            egui::CentralPanel::no_frame()
                .frame(egui::Frame::new().fill(theme::EDITOR_BACKGROUND))
                .show(ui, |ui| self.panes_ui(ui));
            self.dialog_ui(ui.ctx());
        };
        if class == ViewportClass::EmbeddedWindow {
            egui::Window::new(title)
                .id(window_id)
                .default_size([1100.0, 700.0])
                .show(&ctx, |ui| body(ui));
        } else {
            body(ui);
        }

        if !self.started {
            self.started = true;
            self.go_to(self.next_difference(None));
        }
        ctx.send_viewport_cmd_to(self.viewport, ViewportCommand::Title(self.title()));
    }

    fn record_bounds(&mut self, ctx: &egui::Context) {
        let (outer, inner, maximized) = ctx.input(|i| {
            (
                i.viewport().outer_rect,
                i.viewport().inner_rect,
                i.viewport().maximized,
            )
        });
        let maximized = maximized.unwrap_or(false);
        let b = self.bounds.get_or_insert_with(WindowSettings::default);
        b.maximized = maximized;
        if !maximized && let (Some(o), Some(i)) = (outer, inner) {
            b.left = o.min.x as f64;
            b.top = o.min.y as f64;
            b.width = i.width() as f64;
            b.height = i.height() as f64;
        }
    }

    fn update_metrics(&mut self, ctx: &egui::Context) {
        let font = theme::mono(self.metrics.font);
        let (w, h) = ctx.fonts_mut(|f| (f.glyph_width(&font, 'M'), f.row_height(&font)));
        self.metrics.char_width = w;
        self.metrics.line_height = (h + 3.0).round();
    }

    fn menu(&mut self, ui: &mut Ui) {
        let ctrl = crate::platform::ctrl_name();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui
                    .add(Button::new("Save focused side").shortcut_text(format!("{ctrl}+S")))
                    .clicked()
                {
                    self.save(self.focused, true);
                }
                let (l, r) = (
                    self.panes[0].is_dirty() || !self.panes[0].exists,
                    self.panes[1].is_dirty() || !self.panes[1].exists,
                );
                if ui.add_enabled(l, Button::new("Save left")).clicked() {
                    self.save(0, true);
                }
                if ui.add_enabled(r, Button::new("Save right")).clicked() {
                    self.save(1, true);
                }
                if ui
                    .add(Button::new("Save all").shortcut_text(format!("{ctrl}+Shift+S")))
                    .clicked()
                {
                    self.save_all();
                }
                ui.separator();
                if ui
                    .add(Button::new("Reload files").shortcut_text("F5"))
                    .clicked()
                {
                    self.reload(false);
                }
                ui.separator();
                if ui.add(Button::new("Close").shortcut_text("Esc")).clicked() {
                    self.request_close();
                }
            });
            ui.menu_button("Edit", |ui| {
                let p = self.focused;
                if ui
                    .add_enabled(
                        self.panes[p].buffer.can_undo(),
                        Button::new("Undo").shortcut_text(format!("{ctrl}+Z")),
                    )
                    .clicked()
                {
                    self.panes[p].undo();
                    self.schedule_recompare();
                }
                if ui
                    .add_enabled(
                        self.panes[p].buffer.can_redo(),
                        Button::new("Redo").shortcut_text(format!("{ctrl}+Y")),
                    )
                    .clicked()
                {
                    self.panes[p].redo();
                    self.schedule_recompare();
                }
                ui.separator();
                if ui
                    .add(
                        Button::new(format!("Copy to Right {}", icons::ARROW_RIGHT))
                            .shortcut_text(format!("{ctrl}+R")),
                    )
                    .clicked()
                {
                    self.copy(true, None);
                }
                if ui
                    .add(
                        Button::new(format!("{} Copy to Left", icons::ARROW_LEFT))
                            .shortcut_text(format!("{ctrl}+L")),
                    )
                    .clicked()
                {
                    self.copy(false, None);
                }
            });
            ui.menu_button("Search", |ui| {
                if ui
                    .add(Button::new("Next difference").shortcut_text(format!("{ctrl}+N")))
                    .clicked()
                {
                    self.go_to(self.next_difference(self.anchor()));
                }
                if ui
                    .add(Button::new("Previous difference").shortcut_text(format!("{ctrl}+P")))
                    .clicked()
                {
                    self.go_to(self.previous_difference(self.anchor()));
                }
                if ui.button("First difference").clicked() {
                    self.go_to(self.next_difference(None));
                }
                if ui.button("Last difference").clicked() {
                    self.go_to(self.previous_difference(None));
                }
            });
            ui.menu_button("View", |ui| {
                let mut changed = ui
                    .checkbox(
                        &mut self.options.ignore_whitespace,
                        "Ignore whitespace differences",
                    )
                    .changed();
                changed |= ui
                    .checkbox(&mut self.options.ignore_case, "Ignore case differences")
                    .changed();
                ui.checkbox(&mut self.show_whitespace, "Visible whitespace");
                if changed {
                    self.recompare_async();
                }
            });
        });
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        let ctrl = crate::platform::ctrl_name();
        ui.horizontal(|ui| {
            let p = self.focused;
            if ui
                .button("Save")
                .on_hover_text(format!("Save the focused side ({ctrl}+S)"))
                .clicked()
            {
                self.save(p, true);
            }
            if ui
                .add_enabled(self.panes[p].buffer.can_undo(), Button::new("Undo"))
                .on_hover_text(format!("Undo ({ctrl}+Z)"))
                .clicked()
            {
                self.panes[p].undo();
                self.schedule_recompare();
            }
            if ui
                .add_enabled(self.panes[p].buffer.can_redo(), Button::new("Redo"))
                .on_hover_text(format!("Redo ({ctrl}+Y)"))
                .clicked()
            {
                self.panes[p].redo();
                self.schedule_recompare();
            }
            ui.separator();
            let anchor = self.anchor();
            if ui
                .add_enabled(
                    self.previous_difference(anchor).is_some(),
                    Button::new(format!("{} Prev", icons::CARET_UP)),
                )
                .on_hover_text(format!("Previous difference ({ctrl}+P)"))
                .clicked()
            {
                self.go_to(self.previous_difference(anchor));
            }
            if ui
                .add_enabled(
                    self.next_difference(anchor).is_some(),
                    Button::new(format!("{} Next", icons::CARET_DOWN)),
                )
                .on_hover_text(format!("Next difference ({ctrl}+N)"))
                .clicked()
            {
                self.go_to(self.next_difference(anchor));
            }
            ui.separator();
            if ui
                .button(format!("Copy {}", icons::ARROW_RIGHT))
                .on_hover_text(format!("Copy the section to the right ({ctrl}+R)"))
                .clicked()
            {
                self.copy(true, None);
            }
            if ui
                .button(format!("{} Copy", icons::ARROW_LEFT))
                .on_hover_text(format!("Copy the section to the left ({ctrl}+L)"))
                .clicked()
            {
                self.copy(false, None);
            }
            ui.separator();
            let mut changed = ui
                .toggle_value(&mut self.options.ignore_whitespace, "Ignore whitespace")
                .changed();
            changed |= ui
                .toggle_value(&mut self.options.ignore_case, "Ignore case")
                .changed();
            if changed {
                self.recompare_async();
            }
            ui.separator();
            if ui
                .button("Reload")
                .on_hover_text("Reload both files from disk (F5)")
                .clicked()
            {
                self.reload(false);
            }
        });
    }

    fn status(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(self.diff_count_text());
            ui.separator();
            ui.label(&self.section_text);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let p = &self.panes[self.focused];
                ui.label(format!(
                    "{}  Ln {}, Col {}",
                    p.name(),
                    format_count(p.caret.line as u64 + 1),
                    format_count(p.caret.col as u64 + 1)
                ));
            });
        });
    }

    /// The current line and its partner, with inline highlights.
    fn details(&mut self, ui: &mut Ui) {
        let c = self.comparison.clone();
        let p = self.focused;
        let mut lines = [
            self.panes[0].caret.line as isize,
            self.panes[1].caret.line as isize,
        ];
        if let Some(c) = &c {
            let left = self.panes[p].is_left;
            lines[1 - p] = c
                .partner_line(left, self.panes[p].caret.line)
                .map_or(-1, |l| l as isize);
        }
        for (side, label) in [(0usize, "L "), (1, "R ")] {
            let mut job = egui::text::LayoutJob::default();
            let font = self.metrics.font_id();
            let fmt = |color: egui::Color32, bg: egui::Color32| egui::TextFormat {
                font_id: font.clone(),
                color,
                background: bg,
                ..Default::default()
            };
            job.append(
                label,
                0.0,
                fmt(theme::LINE_NUMBER, egui::Color32::TRANSPARENT),
            );
            let line = lines[side];
            let pane = &self.panes[side];
            if line >= 0 && (line as usize) < pane.buffer.line_count() {
                let line = line as usize;
                let text: String = pane
                    .buffer
                    .line(line)
                    .chars()
                    .take(2000)
                    .map(|c| if c == '\t' { ' ' } else { c })
                    .collect();
                let info = c.as_ref().map(|c| c.side(pane.is_left));
                let kind = info
                    .filter(|s| line < s.line_count())
                    .map_or(DiffBlockKind::Equal, |s| s.kinds[line]);
                let (line_bg, text_bg) = match kind {
                    DiffBlockKind::Equal => {
                        (egui::Color32::TRANSPARENT, egui::Color32::TRANSPARENT)
                    }
                    DiffBlockKind::Unimportant => {
                        (theme::UNIMPORTANT_LINE, theme::UNIMPORTANT_TEXT)
                    }
                    _ => (theme::CHANGED_LINE, theme::CHANGED_TEXT),
                };
                let chars: Vec<char> = text.chars().collect();
                let ranges = info
                    .and_then(|s| s.inline.get(&line))
                    .cloned()
                    .unwrap_or_default();
                let mut pos = 0;
                for r in ranges {
                    let (a, b) = (r.start.min(chars.len()), r.end().min(chars.len()));
                    if a > pos {
                        job.append(
                            &chars[pos..a].iter().collect::<String>(),
                            0.0,
                            fmt(theme::TEXT, line_bg),
                        );
                    }
                    if b > a {
                        job.append(
                            &chars[a..b].iter().collect::<String>(),
                            0.0,
                            fmt(theme::TEXT, text_bg),
                        );
                    }
                    pos = pos.max(b);
                }
                if pos < chars.len() {
                    job.append(
                        &chars[pos..].iter().collect::<String>(),
                        0.0,
                        fmt(theme::TEXT, line_bg),
                    );
                }
            }
            ui.add(egui::Label::new(job).truncate());
        }
    }

    fn panes_ui(&mut self, ui: &mut Ui) {
        let c = self.comparison.clone();
        let cid = self.comparison_id;
        for p in &mut self.panes {
            p.update_rows(c.as_deref(), cid);
        }
        let full = ui.max_rect();
        let pane_width = ((full.width() - OVERVIEW_WIDTH - STRIP_WIDTH) / 2.0).max(50.0);
        let overview = Rect::from_min_size(full.min, vec2(OVERVIEW_WIDTH, full.height()));
        let left = Rect::from_min_size(
            pos2(overview.right(), full.top()),
            vec2(pane_width, full.height()),
        );
        let strip = Rect::from_min_size(
            pos2(left.right(), full.top()),
            vec2(STRIP_WIDTH, full.height()),
        );
        let right = Rect::from_min_max(pos2(strip.right(), full.top()), full.max);
        let rects = [left, right];

        // Headers.
        for (side, r) in rects.iter().enumerate() {
            let header = Rect::from_min_size(r.min, vec2(r.width(), HEADER_HEIGHT));
            self.header(ui, side, header);
        }
        let editor_top = full.top() + HEADER_HEIGHT;
        let editor = |r: Rect| {
            Rect::from_min_max(
                pos2(r.left(), editor_top),
                pos2(r.right() - SCROLLBAR, r.bottom() - SCROLLBAR),
            )
        };
        let editors = [editor(left), editor(right)];
        self.viewport_height = editors[0].height();
        let strip = Rect::from_min_max(
            pos2(strip.left(), editor_top),
            pos2(strip.right(), editors[0].bottom()),
        );
        let overview = Rect::from_min_max(
            pos2(overview.left(), editor_top),
            pos2(overview.right(), editors[0].bottom()),
        );

        // Scrolling (shared by both panes).
        let hovered = ui.rect_contains_pointer(full);
        if hovered && self.dialog.is_none() {
            let delta = ui.input(|i| i.smooth_scroll_delta);
            if delta != egui::Vec2::ZERO {
                self.v_offset -= delta.y;
                self.h_offset -= delta.x;
                self.clamp_scroll();
            }
        }

        for side in 0..2 {
            self.editor_ui(ui, side, editors[side]);
            self.scrollbars(ui, side, rects[side], editors[side]);
        }
        self.center_strip(ui, strip);
        self.overview(ui, overview);

        if self.scroll_to_caret {
            self.scroll_to_caret = false;
            self.ensure_caret_visible(editors[self.focused]);
        }
    }

    fn header(&self, ui: &mut Ui, side: usize, rect: Rect) {
        let p = &self.panes[side];
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::PANE_HEADER);
        painter.hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0, theme::PANE_HEADER_BORDER),
        );
        let title = format!(
            "{}{}{}",
            if p.is_dirty() { "* " } else { "" },
            p.path.display(),
            if p.exists {
                ""
            } else {
                "   (does not exist; saving creates it)"
            }
        );
        painter.text(
            pos2(rect.left() + 6.0, rect.top() + 11.0),
            Align2::LEFT_CENTER,
            title,
            FontId::proportional(13.0),
            theme::TEXT,
        );
        painter.text(
            pos2(rect.left() + 6.0, rect.top() + 29.0),
            Align2::LEFT_CENTER,
            format!("{} · {}", p.format.encoding_name(), p.format.newline_name()),
            FontId::proportional(12.0),
            theme::FORMAT_TEXT,
        );
        ui.interact(rect, ui.id().with(("tc-header", side)), Sense::hover())
            .on_hover_text(p.path.display().to_string());
    }

    fn editor_ui(&mut self, ui: &mut Ui, side: usize, rect: Rect) {
        let id = ui.id().with(("tc-editor", side));
        let response = ui.interact(rect, id, Sense::click_and_drag());
        let m = self.metrics;
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Text);
        }
        let (v, h) = (self.v_offset, self.h_offset);
        let shift = ui.input(|i| i.modifiers.shift);
        if response.drag_started() || response.clicked() {
            response.request_focus();
            self.focused = side;
            if let Some(pos) = response.interact_pointer_pos() {
                let at = self.panes[side].pos_at(rect, &m, v, h, pos);
                self.panes[side].set_caret(at, shift);
                self.dragging = response.drag_started().then_some(side);
                self.after_caret_move();
            }
        }
        if response.double_clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let at = self.panes[side].pos_at(rect, &m, v, h, pos);
            self.panes[side].select_word(at);
        }
        if response.dragged()
            && self.dragging == Some(side)
            && let Some(pos) = ui.input(|i| i.pointer.latest_pos())
        {
            let at = self.panes[side].pos_at(rect, &m, v, h, pos);
            self.panes[side].set_caret(at, true);
            // Auto-scroll while dragging outside.
            if pos.y < rect.top() {
                self.v_offset -= m.line_height;
            } else if pos.y > rect.bottom() {
                self.v_offset += m.line_height;
            }
            self.clamp_scroll();
        }
        if response.drag_stopped() {
            self.dragging = None;
            self.after_caret_move();
        }
        if response.has_focus() {
            self.focused = side;
            ui.memory_mut(|mem| {
                mem.set_focus_lock_filter(
                    id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: false,
                    },
                )
            });
            if self.dialog.is_none() {
                self.handle_editing(ui, side, rect);
            }
        } else if !ui.ctx().memory(|m| m.focused().is_some()) && !self.started {
            response.request_focus();
        }
        let c = self.comparison.clone();
        let focused = response.has_focus()
            || (self.focused == side && !ui.ctx().memory(|m| m.focused().is_some()));
        let painter = ui.painter_at(rect);
        self.panes[side].draw(
            &painter,
            rect,
            &m,
            self.v_offset,
            self.h_offset,
            c.as_deref(),
            self.current_block,
            self.show_whitespace,
            focused,
        );
    }

    /// Typing and caret keys for the focused pane.
    fn handle_editing(&mut self, ui: &mut Ui, side: usize, rect: Rect) {
        let events = ui.input(|i| i.events.clone());
        let page = ((rect.height() / self.metrics.line_height) as isize - 1).max(1);
        let mut edited = false;
        let mut moved = false;
        let pane = &mut self.panes[side];
        for event in events {
            match event {
                Event::Text(t) if !t.chars().any(char::is_control) => {
                    pane.insert(&t, true);
                    edited = true;
                }
                Event::Ime(egui::ImeEvent::Commit(t)) => {
                    pane.insert(&t, true);
                    edited = true;
                }
                Event::Paste(t) => {
                    pane.paste(&t);
                    edited = true;
                }
                Event::Copy => {
                    if pane.has_selection() {
                        ui.ctx().copy_text(pane.selected_text());
                    }
                }
                Event::Cut => {
                    if let Some(t) = pane.cut() {
                        ui.ctx().copy_text(t);
                        edited = true;
                    }
                }
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    let word = modifiers.command;
                    let extend = modifiers.shift;
                    match key {
                        Key::Enter => {
                            pane.newline();
                            edited = true;
                        }
                        Key::Tab if !modifiers.any() => {
                            pane.insert("\t", true);
                            edited = true;
                        }
                        Key::Backspace => {
                            pane.backspace(word);
                            edited = true;
                        }
                        Key::Delete => {
                            pane.delete(word);
                            edited = true;
                        }
                        Key::ArrowLeft if !modifiers.alt => {
                            pane.move_horizontal(false, word, extend);
                            moved = true;
                        }
                        Key::ArrowRight if !modifiers.alt => {
                            pane.move_horizontal(true, word, extend);
                            moved = true;
                        }
                        Key::ArrowUp if !modifiers.alt => {
                            pane.move_vertical(-1, extend);
                            moved = true;
                        }
                        Key::ArrowDown if !modifiers.alt => {
                            pane.move_vertical(1, extend);
                            moved = true;
                        }
                        Key::PageUp => {
                            pane.move_vertical(-page, extend);
                            moved = true;
                        }
                        Key::PageDown => {
                            pane.move_vertical(page, extend);
                            moved = true;
                        }
                        Key::Home => {
                            pane.home(word, extend);
                            moved = true;
                        }
                        Key::End => {
                            pane.end(word, extend);
                            moved = true;
                        }
                        Key::A if modifiers.command => {
                            pane.select_all();
                        }
                        Key::Z if modifiers.command && modifiers.shift => {
                            pane.redo();
                            edited = true;
                        }
                        Key::Z if modifiers.command => {
                            pane.undo();
                            edited = true;
                        }
                        Key::Y if modifiers.command => {
                            pane.redo();
                            edited = true;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        if edited {
            self.schedule_recompare();
        }
        if edited || moved {
            self.scroll_to_caret = true;
            self.after_caret_move();
        }
    }

    fn after_caret_move(&mut self) {
        self.sync_other_caret();
        self.update_current_block();
    }

    fn handle_shortcuts(&mut self, ui: &mut Ui) {
        let cmd = Modifiers::COMMAND;
        let shift_cmd = Modifiers::COMMAND | Modifiers::SHIFT;
        let consume = |m: Modifiers, k: Key| ui.input_mut(|i| i.consume_key(m, k));
        if consume(shift_cmd, Key::S) {
            self.save_all();
        } else if consume(cmd, Key::S) {
            self.save(self.focused, true);
        }
        if consume(Modifiers::NONE, Key::F5) {
            self.reload(false);
        }
        if consume(cmd, Key::N) || consume(Modifiers::ALT, Key::ArrowDown) {
            self.go_to(self.next_difference(self.anchor()));
        }
        if consume(cmd, Key::P) || consume(Modifiers::ALT, Key::ArrowUp) {
            self.go_to(self.previous_difference(self.anchor()));
        }
        if consume(cmd, Key::R) || consume(Modifiers::ALT, Key::ArrowRight) {
            self.copy(true, None);
        }
        if consume(cmd, Key::L) || consume(Modifiers::ALT, Key::ArrowLeft) {
            self.copy(false, None);
        }
        if consume(Modifiers::NONE, Key::Escape) {
            self.request_close();
        }
    }

    fn total_rows(&self) -> usize {
        self.panes[0]
            .rows
            .total_rows()
            .max(self.panes[1].rows.total_rows())
    }

    fn max_line_width(&self) -> f32 {
        // Cheap estimate: the longest line in chars (tabs count as one).
        let longest = self
            .panes
            .iter()
            .flat_map(|p| (0..p.buffer.line_count()).map(|l| p.buffer.line_len(l)))
            .max()
            .unwrap_or(0);
        longest as f32 * self.metrics.char_width + 40.0
    }

    fn clamp_scroll(&mut self) {
        let max_v =
            (self.total_rows() as f32 * self.metrics.line_height - self.viewport_height).max(0.0);
        self.v_offset = self.v_offset.clamp(0.0, max_v);
        self.h_offset = self.h_offset.max(0.0);
    }

    fn ensure_caret_visible(&mut self, rect: Rect) {
        let p = &self.panes[self.focused];
        let lh = self.metrics.line_height;
        let top = p.rows.row_of_line(p.caret.line) as f32 * lh;
        if top < self.v_offset {
            self.v_offset = top;
        } else if top + lh > self.v_offset + rect.height() {
            self.v_offset = top + lh - rect.height();
        }
        let x = p.caret_x(&self.metrics);
        let text_width = rect.width() - p.gutter_width(&self.metrics) - 8.0;
        if x < self.h_offset {
            self.h_offset = (x - 20.0).max(0.0);
        } else if x > self.h_offset + text_width {
            self.h_offset = x - text_width + 20.0;
        }
        self.clamp_scroll();
    }

    /// A vertical scrollbar right of each pane and a horizontal one below it (both drive the shared offsets).
    fn scrollbars(&mut self, ui: &mut Ui, side: usize, pane: Rect, editor: Rect) {
        let lh = self.metrics.line_height;
        let content = (self.total_rows() as f32 * lh).max(1.0);
        let vbar = Rect::from_min_max(
            pos2(editor.right(), editor.top()),
            pos2(pane.right(), editor.bottom()),
        );
        if let Some(offset) = scrollbar(
            ui,
            ui.id().with(("vbar", side)),
            vbar,
            true,
            self.v_offset,
            editor.height(),
            content,
        ) {
            self.v_offset = offset;
            self.clamp_scroll();
        }
        let width = self.max_line_width();
        let hbar = Rect::from_min_max(
            pos2(editor.left(), editor.bottom()),
            pos2(editor.right(), pane.bottom()),
        );
        if let Some(offset) = scrollbar(
            ui,
            ui.id().with(("hbar", side)),
            hbar,
            false,
            self.h_offset,
            editor.width(),
            width,
        ) {
            self.h_offset = offset;
        }
    }

    /// One band per difference section with copy arrows: the left half copies to the right, the right half to the left.
    fn center_strip(&mut self, ui: &mut Ui, rect: Rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::CENTER_STRIP);
        let Some(c) = self.comparison.clone() else {
            return;
        };
        let lh = self.metrics.line_height;
        let response = ui.interact(rect, ui.id().with("tc-strip"), Sense::click());
        let mut hit = None;
        for (n, b) in c.blocks.iter().enumerate() {
            if b.kind == DiffBlockKind::Equal {
                continue;
            }
            let top = rect.top() + c.row_of(n) as f32 * lh - self.v_offset;
            let height = (b.height() as f32 * lh).max(4.0);
            if top > rect.bottom() || top + height < rect.top() {
                continue;
            }
            let band = Rect::from_min_size(
                pos2(rect.left() + 3.0, top),
                vec2(rect.width() - 6.0, height),
            );
            painter.rect_filled(band, 0.0, band_color(b.kind));
            if self.current_block == Some(n) {
                painter.rect_stroke(
                    band,
                    0.0,
                    Stroke::new(1.5, theme::CURRENT_SECTION),
                    egui::StrokeKind::Inside,
                );
            }
            let cy = top + lh.min(height) / 2.0;
            arrow(&painter, rect.left() + rect.width() * 0.30, cy, true);
            arrow(&painter, rect.left() + rect.width() * 0.70, cy, false);
            if let Some(p) = response.hover_pos()
                && p.y >= top
                && p.y <= top + height.max(lh)
            {
                hit = Some((n, p.x < rect.center().x));
            }
        }
        if let Some((n, to_right)) = hit {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            let response = response.on_hover_text(if to_right {
                "Copy this section to the right (Ctrl+R)"
            } else {
                "Copy this section to the left (Ctrl+L)"
            });
            if response.clicked() {
                self.copy(to_right, Some(vec![n]));
            }
        }
    }

    /// A thumbnail of the whole comparison: one mark per difference and the visible area. Click or drag to jump.
    fn overview(&mut self, ui: &mut Ui, rect: Rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::OVERVIEW);
        let Some(c) = self.comparison.clone() else {
            return;
        };
        let lh = self.metrics.line_height;
        let scale = rect.height() / (c.total_rows.max(1)) as f32;
        for (n, b) in c.blocks.iter().enumerate() {
            if b.kind == DiffBlockKind::Equal {
                continue;
            }
            painter.rect_filled(
                Rect::from_min_size(
                    pos2(rect.left() + 2.0, rect.top() + c.row_of(n) as f32 * scale),
                    vec2(rect.width() - 4.0, (b.height() as f32 * scale).max(2.0)),
                ),
                0.0,
                band_color(b.kind),
            );
        }
        let top = self.v_offset / lh * scale;
        let height = (self.viewport_height / lh * scale).max(4.0);
        let view = Rect::from_min_size(
            pos2(rect.left() + 0.5, rect.top() + top + 0.5),
            vec2(rect.width() - 1.0, height.min(rect.height() - top) - 1.0),
        );
        painter.rect(
            view,
            0.0,
            theme::OVERVIEW_VIEWPORT_FILL,
            Stroke::new(1.0, theme::OVERVIEW_VIEWPORT_BORDER),
            egui::StrokeKind::Inside,
        );
        let response = ui
            .interact(rect, ui.id().with("tc-overview"), Sense::click_and_drag())
            .on_hover_text("Overview: click to jump");
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if (response.clicked() || response.dragged())
            && let Some(p) = response.interact_pointer_pos()
        {
            let row = (p.y - rect.top()) / scale;
            self.v_offset = row * lh - self.viewport_height / 2.0;
            self.clamp_scroll();
        }
    }

    // ---- Dialogs -----------------------------------------------------------------------------------

    fn dialog_ui(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &self.dialog else { return };
        let mut answer: Option<Answer> = None;
        let modal = egui::Modal::new(egui::Id::new(("tc-dialog", self.viewport))).show(ctx, |ui| {
            ui.set_max_width(520.0);
            let (title, text, kind) = match dialog {
                TextDialog::Message { title, text } => (title.clone(), text.clone(), 0),
                TextDialog::DiskChanged { side, .. } => (
                    "Save".to_string(),
                    format!(
                        "{}\n\nhas been changed on disk since it was loaded. Overwrite it?",
                        self.panes[*side].path.display()
                    ),
                    1,
                ),
                TextDialog::Reload => (
                    "Reload".into(),
                    "Discard your unsaved changes and reload both files from disk?".into(),
                    1,
                ),
                TextDialog::Close => {
                    let sides: Vec<String> = self
                        .panes
                        .iter()
                        .filter(|p| p.is_dirty())
                        .map(|p| p.path.display().to_string())
                        .collect();
                    (
                        self.request.title.clone(),
                        format!("Save changes to\n{}?", sides.join(" and ")),
                        2,
                    )
                }
            };
            crate::dialogs::title_line(ui, &title);
            ui.label(text);
            ui.add_space(8.0);
            let size = vec2(ui.available_width(), 26.0);
            ui.allocate_ui_with_layout(size, Layout::right_to_left(Align::Center), |ui| {
                let b = |t: &str| Button::new(t).min_size(vec2(76.0, 24.0));
                match kind {
                    0 => {
                        if ui.add(b("OK")).clicked() {
                            answer = Some(Answer::Ok);
                        }
                    }
                    1 => {
                        if ui.add(b("No")).clicked() {
                            answer = Some(Answer::No);
                        }
                        if ui.add(b("Yes")).clicked() {
                            answer = Some(Answer::Yes);
                        }
                    }
                    _ => {
                        if ui.add(b("Cancel")).clicked() {
                            answer = Some(Answer::Cancel);
                        }
                        if ui.add(b("No")).clicked() {
                            answer = Some(Answer::No);
                        }
                        if ui.add(b("Yes")).clicked() {
                            answer = Some(Answer::Yes);
                        }
                    }
                }
            });
        });
        if answer.is_none() && modal.should_close() {
            answer = Some(Answer::Cancel);
        }
        let Some(answer) = answer else { return };
        let dialog = self.dialog.take().expect("checked");
        match (dialog, answer) {
            (TextDialog::DiskChanged { side, then_close }, Answer::Yes) => {
                if self.save(side, false) && then_close {
                    self.continue_close_after_save();
                }
            }
            (
                TextDialog::DiskChanged {
                    then_close: true, ..
                },
                _,
            ) => self.close_cancelled = true,
            (TextDialog::Reload, Answer::Yes) => self.reload(true),
            (TextDialog::Close, Answer::Yes) => self.continue_close_after_save(),
            (TextDialog::Close, Answer::No) => self.finish_close(),
            (TextDialog::Close, _) => self.close_cancelled = true,
            _ => {}
        }
    }

    /// Saves the dirty sides one by one (asking about files changed on disk), then closes.
    fn continue_close_after_save(&mut self) {
        for side in 0..2 {
            if !self.panes[side].is_dirty() {
                continue;
            }
            let p = &self.panes[side];
            let changed = p.exists
                && p.loaded_mtime.is_some_and(|loaded| {
                    std::fs::metadata(&p.path)
                        .and_then(|m| m.modified())
                        .is_ok_and(|m| m != loaded)
                });
            if changed {
                self.dialog = Some(TextDialog::DiskChanged {
                    side,
                    then_close: true,
                });
                return;
            }
            if !self.save(side, false) {
                self.close_cancelled = true;
                return;
            }
        }
        self.finish_close();
    }
}

#[derive(Debug, Clone, Copy)]
enum Answer {
    Ok,
    Yes,
    No,
    Cancel,
}

fn arrow(painter: &egui::Painter, cx: f32, cy: f32, right: bool) {
    let (s, d) = (4.5, if right { 1.0 } else { -1.0 });
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(cx - d * s, cy - s),
            pos2(cx + d * s, cy),
            pos2(cx - d * s, cy + s),
        ],
        theme::GLYPH,
        Stroke::NONE,
    ));
}

/// A minimal scrollbar; returns the new offset when dragged or clicked.
fn scrollbar(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    vertical: bool,
    offset: f32,
    visible: f32,
    content: f32,
) -> Option<f32> {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::CENTER_STRIP);
    if content <= visible {
        return None;
    }
    let len = if vertical {
        rect.height()
    } else {
        rect.width()
    };
    let thumb_len = (visible / content * len).max(20.0).min(len);
    let travel = (len - thumb_len).max(1.0);
    let max_offset = content - visible;
    let pos = offset / max_offset * travel;
    let thumb = if vertical {
        Rect::from_min_size(
            pos2(rect.left() + 2.0, rect.top() + pos),
            vec2(rect.width() - 4.0, thumb_len),
        )
    } else {
        Rect::from_min_size(
            pos2(rect.left() + pos, rect.top() + 2.0),
            vec2(thumb_len, rect.height() - 4.0),
        )
    };
    let response = ui.interact(rect, id, Sense::click_and_drag());
    let color = if response.dragged() || response.hovered() {
        theme::LINE_NUMBER
    } else {
        theme::PANE_HEADER_BORDER
    };
    painter.rect_filled(thumb, 3.0, color);
    if response.dragged() || response.clicked() {
        let p = response.interact_pointer_pos()?;
        let at = if vertical {
            p.y - rect.top()
        } else {
            p.x - rect.left()
        };
        return Some(((at - thumb_len / 2.0) / travel * max_offset).clamp(0.0, max_offset));
    }
    None
}

/// The window state shared with its viewport callback.
pub type SharedWindow = Arc<Mutex<TextCompareWindow>>;

/// Shows a text compare window as its own native window (call every frame while it is open).
pub fn show_deferred(ctx: &egui::Context, window: &SharedWindow) {
    let (id, builder) = {
        let w = window.lock().unwrap();
        (w.viewport, w.viewport_builder())
    };
    let w = window.clone();
    ctx.show_viewport_deferred(id, builder, move |ui, class| {
        if let Ok(mut w) = w.lock() {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.ui(ui, class)));
            if let Err(p) = result {
                error!("Text compare window failed: {}", crate::panic_message(&p));
                w.closed = true;
            }
        }
    });
}

/// The app when two files are given on the command line: only the text compare window, which is the root.
pub struct TextOnlyApp {
    window: TextCompareWindow,
    settings: crate::settings::AppSettings,
}

impl TextOnlyApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        request: TextCompareRequest,
        settings: crate::settings::AppSettings,
    ) -> Result<Self, String> {
        theme::install(&cc.egui_ctx);
        let mut window =
            TextCompareWindow::open(&cc.egui_ctx, request, settings.text_compare.clone())?;
        window.viewport = ViewportId::ROOT;
        Ok(Self { window, settings })
    }
}

impl eframe::App for TextOnlyApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.window.ui(ui, ViewportClass::Root);
        if self.window.closed {
            self.settings.text_compare = self.window.settings.clone();
            crate::settings::save(&crate::settings::settings_path(), &self.settings);
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(dir: &Path, left: &[u8], right: Option<&[u8]>) -> TextCompareRequest {
        let l = dir.join("l.txt");
        let r = dir.join("sub").join("r.txt");
        std::fs::write(&l, left).unwrap();
        if let Some(r_bytes) = right {
            std::fs::create_dir_all(r.parent().unwrap()).unwrap();
            std::fs::write(&r, r_bytes).unwrap();
        }
        TextCompareRequest {
            left: l,
            left_exists: true,
            right: r,
            right_exists: right.is_some(),
            title: "t".into(),
            relative_path: None,
        }
    }

    #[test]
    fn missing_side_inherits_format_and_binary_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let (l, r) = load(&request(dir.path(), b"\xEF\xBB\xBFa\nb\n", None)).unwrap();
        assert!(l.exists && !r.exists);
        assert_eq!("UTF-8 BOM", r.text.encoding_name());
        assert_eq!("\n", r.text.newline);
        assert_eq!("", r.text.text);
        let e = load(&request(dir.path(), b"a\0b", Some(b"x")))
            .err()
            .unwrap();
        assert!(e.ends_with("is not a text file and cannot be compared as text."));
    }

    #[test]
    fn copy_and_save_round_trip_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let req = request(dir.path(), b"a\r\nX\r\nb\r\n", Some(b"a\nY\nb\n"));
        let ctx = egui::Context::default();
        let mut w = TextCompareWindow::open(&ctx, req, TextCompareSettings::default()).unwrap();
        assert_eq!(1, w.comparison.as_ref().unwrap().difference_count);
        w.go_to(w.next_difference(None));
        w.copy(true, None);
        assert_eq!(0, w.comparison.as_ref().unwrap().difference_count);
        assert!(w.panes[1].is_dirty());
        assert!(w.save(1, true));
        // The right side keeps its LF line endings; the copied line gets LF too.
        assert_eq!(
            b"a\nX\nb\n".to_vec(),
            std::fs::read(dir.path().join("sub/r.txt")).unwrap()
        );
        w.panes[1].undo();
        assert!(w.panes[1].is_dirty());
    }

    #[test]
    fn navigation_and_sections() {
        let dir = tempfile::tempdir().unwrap();
        let req = request(dir.path(), b"1\n2\n3\n4\n5\n6", Some(b"1\nx\n3\n4\ny\n6"));
        let ctx = egui::Context::default();
        let mut w = TextCompareWindow::open(&ctx, req, TextCompareSettings::default()).unwrap();
        let first = w.next_difference(None);
        w.go_to(first);
        assert_eq!("Section 1 of 2", w.section_text);
        w.go_to(w.next_difference(w.anchor()));
        assert_eq!("Section 2 of 2", w.section_text);
        assert_eq!(4, w.panes[0].caret.line);
        assert_eq!(None, w.next_difference(w.anchor()));
        w.go_to(w.previous_difference(w.anchor()));
        assert_eq!("Section 1 of 2", w.section_text);
    }
}
