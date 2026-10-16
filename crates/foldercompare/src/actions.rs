//! Commands of the main window: copy, delete, rename, rescan, export, shell actions, settings and profiles.
//! Same order and messages as the WPF `MainViewModel`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::SystemTime;

use fc_core::compare::{DiffEngine, PathComparison, RefreshData, RefreshRequest};
use fc_core::model::{DiffStatus, NodeId, Side};
use fc_core::ops::{
    self, ActionKind, CopyDirection, FileOperationExecutor, ItemOutcome, OperationOptions,
    OperationProgress, OperationSummary, OverwriteChoice, OverwriteDecision, Sides, plan_copy,
    plan_delete, top_level_only,
};
use tracing::warn;

use crate::app::{Busy, FolderCompareApp, Msg, spawn_worker};
use crate::dialogs::{ConfirmAction, Dialog, InputPurpose, Outcome, PendingOp};
use crate::platform::{self, AppTrash, TRASH_FAILED};
use crate::state::{format_count, format_size};

/// Result of an operation, sent back by the worker.
pub struct OpResult {
    pub op: PendingOp,
    pub summary: OperationSummary,
    pub refresh: Vec<RefreshData>,
}

impl FolderCompareApp {
    // ---- Copy / delete -----------------------------------------------------------------------

    pub fn copy(&mut self, direction: CopyDirection) {
        let Some(result) = &self.state.result else {
            return;
        };
        let busy = self.is_busy();
        let allowed = match direction {
            CopyDirection::LeftToRight => self.state.can_copy_to_right(busy),
            CopyDirection::RightToLeft => self.state.can_copy_to_left(busy),
        };
        if !allowed {
            return;
        }
        let nodes = self.state.selected_nodes();
        let plan = plan_copy(result, &nodes, direction);
        let title = match direction {
            CopyDirection::LeftToRight => "Copy to right",
            CopyDirection::RightToLeft => "Copy to left",
        };
        if plan.actions.is_empty() {
            let mut text = "Nothing to copy.".to_string();
            if !plan.notes.is_empty() {
                text.push_str("\n\n");
                text.push_str(&plan.notes.join("\n"));
            }
            self.dialogs.push(Dialog::info(title, text));
            return;
        }
        let op = PendingOp {
            title: title.into(),
            plan,
            selection: nodes,
            is_delete: false,
            permanent: false,
        };
        if op.plan.actions.len() as i64 >= self.settings.preview_threshold {
            self.dialogs.push(Dialog::Preview {
                op,
                offer_dont_ask: false,
                dont_ask: false,
            });
        } else {
            self.execute(op);
        }
    }

    /// Delete key / toolbar: asks for the side when some items exist on both sides.
    pub fn delete(&mut self) {
        if !self.state.can_delete_any(self.is_busy()) {
            return;
        }
        let nodes = self.state.selected_nodes();
        let both = nodes
            .iter()
            .any(|&n| self.state.node(n).left.is_some() && self.state.node(n).right.is_some());
        if both {
            self.dialogs.push(Dialog::DeleteSide { nodes });
        } else {
            self.delete_sides(nodes, Sides::Both);
        }
    }

    pub fn delete_side(&mut self, sides: Sides) {
        let busy = self.is_busy();
        let allowed = match sides {
            Sides::Left => self.state.can_delete_left(busy),
            Sides::Right => self.state.can_delete_right(busy),
            Sides::Both => self.state.can_delete_any(busy),
        };
        if allowed {
            let nodes = self.state.selected_nodes();
            self.delete_sides(nodes, sides);
        }
    }

    fn delete_sides(&mut self, nodes: Vec<NodeId>, sides: Sides) {
        let Some(result) = &self.state.result else {
            return;
        };
        let plan = plan_delete(result, &nodes, sides);
        if plan.actions.is_empty() {
            self.dialogs.push(Dialog::info(
                "Delete",
                "Nothing to delete on the chosen side.",
            ));
            return;
        }
        let recycle = self.settings.delete_to_recycle_bin;
        let op = PendingOp {
            title: if recycle {
                format!("Move to {}", platform::trash_name())
            } else {
                "Delete permanently".into()
            },
            plan,
            selection: nodes,
            is_delete: true,
            permanent: !recycle,
        };
        if !recycle || self.settings.confirm_recycle_delete {
            self.dialogs.push(Dialog::Preview {
                op,
                offer_dont_ask: recycle,
                dont_ask: false,
            });
        } else {
            self.confirmed_delete(op);
        }
    }

    /// After the preview: a permanent delete is confirmed a second time.
    fn confirmed_delete(&mut self, op: PendingOp) {
        if op.permanent {
            let text = format!(
                "Permanently delete {} item(s) ({})?\n\nThis cannot be undone.",
                format_count(op.plan.actions.len() as u64),
                format_size(op.plan.total_bytes())
            );
            self.dialogs.push(Dialog::warning(
                "Delete permanently",
                text,
                ConfirmAction::Execute(op),
            ));
        } else {
            self.execute(op);
        }
    }

    /// Runs the plan on a worker thread; overwrite questions come back as dialogs.
    fn execute(&mut self, mut op: PendingOp) {
        let Some(result) = &self.state.result else {
            return;
        };
        if op.is_delete {
            op.title = "Delete".into();
        }
        let options = OperationOptions {
            tolerance: result.options.tolerance,
            overwrite_policy: self.settings.overwrite_policy.into(),
            delete_to_recycle_bin: !op.permanent,
            ..OperationOptions::new(&result.left_root, &result.right_root)
        };
        // Refresh only the affected rows afterwards instead of rescanning everything.
        let top = top_level_only(&result.tree, &op.selection);
        let requests: Vec<(NodeId, RefreshRequest)> = top
            .iter()
            .filter_map(|&n| {
                let node = result.tree.node(n);
                node.parent
                    .map(|p| (n, result.refresh_request(p, &node.name)))
            })
            .collect();
        let paths = result.paths();

        self.job += 1;
        let job = self.job;
        self.cancel = Arc::new(AtomicBool::new(false));
        self.busy = Busy::Operating;
        self.progress = 0.0;
        self.progress_indeterminate = false;
        self.file_progress = 0.0;
        let (tx, ctx, cancel) = (self.tx.clone(), self.ctx.clone(), self.cancel.clone());
        let generation = self.state.generation;
        spawn_worker(
            self.tx.clone(),
            self.ctx.clone(),
            job,
            "Operation failed",
            move || {
                let trash = AppTrash::default();
                let executor = FileOperationExecutor::new(&trash);
                let title = op.title.clone();
                let mut resolver = |c: &ops::OverwriteConflict| {
                    let (reply, answer) = mpsc::channel();
                    let _ = tx.send(Msg::Overwrite {
                        conflict: c.clone(),
                        reply,
                    });
                    ctx.request_repaint();
                    answer.recv().unwrap_or(OverwriteDecision {
                        choice: OverwriteChoice::Cancel,
                        apply_to_all: false,
                    })
                };
                let mut progress = |p: &OperationProgress| {
                    let _ = tx.send(Msg::OpProgress {
                        job,
                        title: title.clone(),
                        progress: p.clone(),
                    });
                    ctx.request_repaint();
                };
                let summary = executor.execute(
                    &op.plan.actions,
                    &options,
                    Some(&mut resolver),
                    Some(&mut progress),
                    &cancel,
                );

                let mut targets: Vec<RefreshRequest> =
                    requests.iter().map(|(_, r)| r.clone()).collect();
                for item in &summary.items {
                    let Some(final_target) = &item.final_target else {
                        continue;
                    };
                    let Some((_, req)) = requests.iter().find(|(n, _)| *n == item.action.node)
                    else {
                        continue;
                    };
                    let name = final_target
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if !paths.eq(&name, req.name()) {
                        targets.push(req.with_name(&name));
                    }
                }
                let refresh = prepare_all(&dedupe(targets, paths));
                let _ = tx.send(Msg::OpDone {
                    job,
                    generation,
                    result: Box::new(OpResult {
                        op,
                        summary,
                        refresh,
                    }),
                });
                ctx.request_repaint();
            },
        );
    }

    pub fn on_op_progress(&mut self, title: &str, p: &OperationProgress) {
        self.progress = if p.total_bytes > 0 {
            p.total_bytes_done as f32 / p.total_bytes as f32
        } else if p.items_total > 0 {
            p.items_done as f32 / p.items_total as f32
        } else {
            0.0
        };
        self.file_progress = if p.file_bytes_total > 0 {
            p.file_bytes_done as f32 / p.file_bytes_total as f32
        } else {
            0.0
        };
        self.status_text = format!(
            "{title}: {}/{}  {}",
            format_count((p.items_done + 1).min(p.items_total) as u64),
            format_count(p.items_total as u64),
            p.current_item
        );
    }

    pub fn on_op_done(&mut self, generation: u64, r: OpResult) {
        self.busy = Busy::Idle;
        self.progress = 0.0;
        self.file_progress = 0.0;
        self.apply_refresh(generation, r.refresh);
        let s = &r.summary;
        self.status_text = format!(
            "{}: {} succeeded, {} skipped, {} failed{}.",
            r.op.title,
            format_count(s.succeeded() as u64),
            format_count(s.skipped() as u64),
            format_count(s.failed() as u64),
            if s.cancelled { " (cancelled)" } else { "" }
        );
        self.dialogs
            .push(Dialog::summary(&r.op.title, s, &r.op.plan.notes));

        // Items the trash refused: never fall back silently, ask before deleting them permanently.
        let refused: Vec<_> = s
            .items
            .iter()
            .filter(|i| {
                i.outcome == ItemOutcome::Failed
                    && i.action.kind == ActionKind::Delete
                    && i.message
                        .as_deref()
                        .is_some_and(|m| m.starts_with(TRASH_FAILED))
            })
            .collect();
        if !refused.is_empty() && !r.op.permanent {
            let mut plan = r.op.plan.clone();
            plan.actions = refused.iter().map(|i| i.action.clone()).collect();
            plan.notes.clear();
            let text = format!(
                "{} item(s) could not be moved to the {}:\n\n{}\n\nDelete them permanently? This cannot be undone.",
                format_count(refused.len() as u64),
                platform::trash_name(),
                refused[0].message.as_deref().unwrap_or_default()
            );
            let op = PendingOp {
                title: "Delete permanently".into(),
                selection: r.op.selection.clone(),
                plan,
                is_delete: true,
                permanent: true,
            };
            self.dialogs.push(Dialog::warning(
                "Delete permanently",
                text,
                ConfirmAction::Execute(op),
            ));
        }
    }

    /// Applies prepared refreshes unless a newer comparison replaced the tree meanwhile.
    pub fn apply_refresh(&mut self, generation: u64, data: Vec<RefreshData>) {
        if generation != self.state.generation {
            return;
        }
        let Some(result) = self.state.result.as_mut() else {
            return;
        };
        let mut parents = Vec::new();
        for d in data {
            parents.push(d.parent);
            result.apply_refresh(d);
        }
        for p in parents {
            self.state.sort_subtree(p);
        }
        self.state.rebuild_rows(true);
        self.update_counts();
    }

    // ---- Rescan, rename -----------------------------------------------------------------------

    pub fn rescan_selected(&mut self) {
        let Some(result) = &self.state.result else {
            return;
        };
        if !self.state.can_refresh_selected(self.is_busy()) {
            return;
        }
        let top = top_level_only(&result.tree, &self.state.selected_nodes());
        let requests: Vec<RefreshRequest> = top
            .iter()
            .filter_map(|&n| {
                let node = result.tree.node(n);
                node.parent.map(|p| result.refresh_request(p, &node.name))
            })
            .collect();
        let status = format!(
            "Rescanned {} selected item(s).",
            format_count(requests.len() as u64)
        );
        self.refresh_in_background(requests, Some(status), Busy::Scanning);
    }

    fn refresh_in_background(
        &mut self,
        requests: Vec<RefreshRequest>,
        status: Option<String>,
        busy: Busy,
    ) {
        self.job += 1;
        let job = self.job;
        self.busy = busy;
        self.progress_indeterminate = true;
        let generation = self.state.generation;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let paths = self
            .state
            .result
            .as_ref()
            .map(|r| r.paths())
            .unwrap_or(PathComparison::CaseSensitive);
        spawn_worker(
            self.tx.clone(),
            self.ctx.clone(),
            job,
            "Refresh failed",
            move || {
                let data = prepare_all(&dedupe(requests, paths));
                let _ = tx.send(Msg::RefreshDone {
                    job,
                    generation,
                    data,
                    status,
                });
                ctx.request_repaint();
            },
        );
    }

    pub fn ask_rename(&mut self) {
        if !self.state.can_rename(self.is_busy()) {
            return;
        }
        let Some(id) = self.state.first_selected() else {
            return;
        };
        let node = self.state.node(id);
        self.dialogs.push(Dialog::rename(
            id,
            &node.name.clone(),
            node.left.is_some(),
            node.right.is_some(),
        ));
    }

    fn rename(&mut self, id: NodeId, new_name: &str, left: bool, right: bool) {
        let Some(result) = &self.state.result else {
            return;
        };
        let node = result.tree.node(id);
        if new_name == node.name || !node.alive {
            return;
        }
        let Some(parent) = node.parent else { return };
        let options = OperationOptions::new(&result.left_root, &result.right_root);
        let trash = AppTrash::default();
        let executor = FileOperationExecutor::new(&trash);
        let mut errors = Vec::new();
        for (side, wanted) in [(Side::Left, left), (Side::Right, right)] {
            if !wanted || node.get(side).is_none() {
                continue;
            }
            if let Err(e) = executor.rename(&options, side, &node.relative_path, new_name) {
                errors.push(format!("{side:?}: {e}"));
            }
        }
        let requests = vec![
            result.refresh_request(parent, &node.name),
            result.refresh_request(parent, new_name),
        ];
        self.refresh_in_background(requests, None, Busy::Scanning);
        if !errors.is_empty() {
            self.dialogs
                .push(Dialog::error("Rename", errors.join("\n")));
        }
    }

    // ---- Export ------------------------------------------------------------------------------

    pub fn export_report(&mut self) {
        if !self.state.can_export(self.is_busy()) {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .set_title("Export report")
            .set_file_name("FolderCompare report.csv")
            .add_filter("CSV file", &["csv"])
            .add_filter("Text file", &["txt"])
            .save_file()
        else {
            return;
        };
        let Some(result) = &self.state.result else {
            return;
        };
        // The current filter, independent of which folders are expanded.
        let nodes = self.state.visible_nodes();
        let is_txt = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("txt"));
        let body = if is_txt {
            fc_core::report::write_text(result, &nodes, SystemTime::now())
        } else {
            fc_core::report::write_csv(result, &nodes)
        };
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(body.as_bytes());
        match std::fs::write(&path, bytes) {
            Ok(()) => {
                self.status_text = format!(
                    "Report with {} rows written to {}.",
                    format_count(nodes.len() as u64),
                    path.display()
                )
            }
            Err(e) => self.dialogs.push(Dialog::error(
                "Export report",
                fc_core::compare::io_message(&e),
            )),
        }
    }

    // ---- Shell -------------------------------------------------------------------------------

    fn path_of(&self, side: Side) -> Option<PathBuf> {
        let id = self.state.first_selected()?;
        self.state.node(id).get(side).map(|s| s.full_path.clone())
    }

    pub fn open_side(&mut self, side: Side, what: ShellAction) {
        let Some(path) = self.path_of(side) else {
            return;
        };
        let result = match what {
            ShellAction::Open => platform::open(&path),
            ShellAction::OpenFolder => match path.parent() {
                Some(p) => platform::open(p),
                None => Ok(()),
            },
            ShellAction::Reveal => platform::reveal(&path),
        };
        if let Err(e) = result {
            self.dialogs.push(Dialog::error("Open", e));
        }
    }

    pub fn copy_paths(&mut self, left: bool, right: bool) {
        let mut lines = Vec::new();
        for id in self.state.selected_nodes() {
            let node = self.state.node(id);
            if left && let Some(s) = &node.left {
                lines.push(s.full_path.display().to_string());
            }
            if right && let Some(s) = &node.right {
                lines.push(s.full_path.display().to_string());
            }
        }
        if !lines.is_empty() {
            self.ctx.copy_text(lines.join(fc_core::NEWLINE));
        }
    }

    // ---- Settings and profiles -----------------------------------------------------------------

    pub fn open_settings(&mut self) {
        self.save_settings();
        self.dialogs.push(Dialog::settings(&self.settings));
    }

    fn settings_edited(&mut self, edited: crate::settings::AppSettings) {
        let folders_changed = edited.folders_first != self.state.folders_first;
        self.settings = edited;
        self.exclude = self.settings.exclude_patterns.clone();
        self.include = self.settings.include_patterns.clone();
        if folders_changed {
            self.state.set_folders_first(self.settings.folders_first);
        }
        self.save_settings();
        if self.state.result.is_some() {
            self.status_text = "Settings saved. Press F5 to rescan with the new options.".into();
        }
    }

    pub fn ask_save_profile(&mut self) {
        let current = self
            .selected_profile
            .and_then(|i| self.settings.profiles.get(i))
            .map(|p| p.name.clone())
            .unwrap_or_default();
        self.dialogs.push(Dialog::input(
            "Save profile",
            "Profile name:",
            &current,
            InputPurpose::SaveProfile,
        ));
    }

    /// Handles answers of dialogs that start or continue operations.
    pub fn handle_operation_outcome(&mut self, outcome: Outcome) -> Option<Outcome> {
        match outcome {
            Outcome::Preview { op, dont_ask_again } => {
                if op.is_delete {
                    if dont_ask_again {
                        self.settings.confirm_recycle_delete = false;
                        self.save_settings();
                    }
                    self.confirmed_delete(op);
                } else {
                    self.execute(op);
                }
                None
            }
            Outcome::Confirmed(ConfirmAction::Execute(op)) => {
                self.execute(op);
                None
            }
            Outcome::DeleteSide { sides, nodes } => {
                self.delete_sides(nodes, sides);
                None
            }
            Outcome::Input {
                purpose: InputPurpose::Rename(id),
                value,
                left,
                right,
            } => {
                self.rename(id, &value, left, right);
                None
            }
            Outcome::Input {
                purpose: InputPurpose::SaveProfile,
                value,
                ..
            } => {
                self.save_profile(&value);
                None
            }
            Outcome::Settings(s) => {
                self.settings_edited(*s);
                None
            }
            other => Some(other),
        }
    }

    /// Opens the text compare window for the selected file (filled in with the text compare window).
    pub fn compare_contents(&mut self) {}

    /// Selects every visible row with `status` (context menu).
    pub fn select_status(&mut self, status: DiffStatus) {
        self.state.select_by_status(status);
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ShellAction {
    Open,
    OpenFolder,
    Reveal,
}

/// Removes duplicate (parent, name) requests.
fn dedupe(requests: Vec<RefreshRequest>, paths: PathComparison) -> Vec<RefreshRequest> {
    let mut out: Vec<RefreshRequest> = Vec::new();
    for r in requests {
        if !out
            .iter()
            .any(|o| o.parent() == r.parent() && paths.eq(o.name(), r.name()))
        {
            out.push(r);
        }
    }
    out
}

fn prepare_all(requests: &[RefreshRequest]) -> Vec<RefreshData> {
    let engine = DiffEngine::new();
    let never = AtomicBool::new(false);
    requests
        .iter()
        .filter_map(|r| match engine.prepare_refresh(r, &never) {
            Ok(d) => Some(d),
            Err(e) => {
                warn!("Refresh of {} failed: {e}", r.name());
                None
            }
        })
        .collect()
}
