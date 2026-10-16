//! App-modal dialogs drawn with `egui::Modal`. The app keeps a stack of [`Dialog`]s; the top one is shown and
//! returns an [`Outcome`] once the user answered.

use std::sync::mpsc::Sender;

use egui::{Align, Color32, Key, Layout, Modifiers, RichText, TextEdit, Ui, vec2};
use fc_core::model::NodeId;
use fc_core::ops::{
    ActionKind, OperationPlan, OperationSummary, OverwriteChoice, OverwriteConflict,
    OverwriteDecision, PlannedAction, Sides, is_valid_name,
};

use crate::settings::{AppSettings, ContentCompareSetting, OverwriteSetting, SymlinkSetting};
use crate::state::{format_count, format_date, format_size};
use crate::theme;

/// A planned copy or delete waiting for the user's go.
#[derive(Debug, Clone)]
pub struct PendingOp {
    pub title: String,
    pub plan: OperationPlan,
    /// The selected nodes the plan was made from (their rows are refreshed afterwards).
    pub selection: Vec<NodeId>,
    pub is_delete: bool,
    /// Delete permanently instead of moving to the trash.
    pub permanent: bool,
}

/// What happens after a confirmation was answered with Yes.
#[derive(Debug, Clone)]
pub enum ConfirmAction {
    ReplaceProfile(usize),
    DeleteProfile(usize),
    /// Run the operation (e.g. after the second confirmation of a permanent delete).
    Execute(PendingOp),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputPurpose {
    SaveProfile,
    Rename(NodeId),
}

#[derive(Debug)]
pub struct RenameSides {
    pub left_enabled: bool,
    pub left: bool,
    pub right_enabled: bool,
    pub right: bool,
}

#[derive(Debug)]
pub enum Dialog {
    Message {
        title: String,
        text: String,
        error: bool,
    },
    Confirm {
        title: String,
        text: String,
        warning: bool,
        action: ConfirmAction,
    },
    Preview {
        op: PendingOp,
        offer_dont_ask: bool,
        dont_ask: bool,
    },
    Overwrite {
        conflict: OverwriteConflict,
        apply_to_all: bool,
        reply: Sender<OverwriteDecision>,
    },
    Summary {
        title: String,
        header: String,
        report: String,
    },
    Input {
        title: String,
        prompt: String,
        value: String,
        purpose: InputPurpose,
        sides: Option<RenameSides>,
        error: Option<String>,
        focused: bool,
    },
    DeleteSide {
        nodes: Vec<NodeId>,
    },
    Settings {
        edit: Box<AppSettings>,
        tolerance: String,
        threshold: String,
        error: Option<String>,
    },
}

#[derive(Debug)]
pub enum Outcome {
    Closed,
    Confirmed(ConfirmAction),
    Preview {
        op: PendingOp,
        dont_ask_again: bool,
    },
    DeleteSide {
        sides: Sides,
        nodes: Vec<NodeId>,
    },
    Input {
        purpose: InputPurpose,
        value: String,
        left: bool,
        right: bool,
    },
    Settings(Box<AppSettings>),
}

impl Dialog {
    pub fn info(title: impl Into<String>, text: impl Into<String>) -> Self {
        Dialog::Message {
            title: title.into(),
            text: text.into(),
            error: false,
        }
    }

    pub fn error(title: impl Into<String>, text: impl Into<String>) -> Self {
        Dialog::Message {
            title: title.into(),
            text: text.into(),
            error: true,
        }
    }

    pub fn confirm(
        title: impl Into<String>,
        text: impl Into<String>,
        action: ConfirmAction,
    ) -> Self {
        Dialog::Confirm {
            title: title.into(),
            text: text.into(),
            warning: false,
            action,
        }
    }

    pub fn warning(
        title: impl Into<String>,
        text: impl Into<String>,
        action: ConfirmAction,
    ) -> Self {
        Dialog::Confirm {
            title: title.into(),
            text: text.into(),
            warning: true,
            action,
        }
    }

    pub fn summary(title: &str, summary: &OperationSummary, notes: &[String]) -> Self {
        let nl = fc_core::NEWLINE;
        let mut report = summary.to_report();
        if !notes.is_empty() {
            report.push_str(nl);
            report.push_str("Not included:");
            for n in notes {
                report.push_str(nl);
                report.push_str("  ");
                report.push_str(n);
            }
        }
        Dialog::Summary {
            title: format!("{title} - summary"),
            header: format!(
                "{} succeeded, {} skipped, {} failed{}",
                format_count(summary.succeeded() as u64),
                format_count(summary.skipped() as u64),
                format_count(summary.failed() as u64),
                if summary.cancelled {
                    " - cancelled"
                } else {
                    ""
                }
            ),
            report,
        }
    }

    pub fn input(title: &str, prompt: &str, initial: &str, purpose: InputPurpose) -> Self {
        Dialog::Input {
            title: title.into(),
            prompt: prompt.into(),
            value: initial.into(),
            purpose,
            sides: None,
            error: None,
            focused: false,
        }
    }

    pub fn rename(node: NodeId, name: &str, exists_left: bool, exists_right: bool) -> Self {
        Dialog::Input {
            title: "Rename".into(),
            prompt: "New name:".into(),
            value: name.into(),
            purpose: InputPurpose::Rename(node),
            sides: Some(RenameSides {
                left_enabled: exists_left,
                left: exists_left,
                right_enabled: exists_right,
                right: exists_right,
            }),
            error: None,
            focused: false,
        }
    }

    pub fn settings(current: &AppSettings) -> Self {
        Dialog::Settings {
            tolerance: current.tolerance_seconds.to_string(),
            threshold: current.preview_threshold.to_string(),
            edit: Box::new(current.clone()),
            error: None,
        }
    }

    /// Shows the dialog; returns the outcome when it was answered this frame.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Outcome> {
        let mut outcome = None;
        let wide = matches!(self, Dialog::Preview { .. } | Dialog::Summary { .. });
        let response = egui::Modal::new(egui::Id::new("fc-dialog")).show(ctx, |ui| {
            let max = ctx.content_rect().size() - vec2(80.0, 80.0);
            if wide {
                ui.set_width(900.0f32.min(max.x));
            } else {
                ui.set_max_width(560.0f32.min(max.x));
            }
            outcome = match self {
                Dialog::Message { title, text, error } => message(ui, title, text, *error),
                Dialog::Confirm {
                    title,
                    text,
                    warning,
                    action,
                } => confirm(ui, title, text, *warning, action),
                Dialog::Preview {
                    op,
                    offer_dont_ask,
                    dont_ask,
                } => preview(ui, op, *offer_dont_ask, dont_ask, max.y),
                Dialog::Overwrite {
                    conflict,
                    apply_to_all,
                    reply,
                } => overwrite(ui, conflict, apply_to_all, reply),
                Dialog::Summary {
                    title,
                    header,
                    report,
                } => summary(ui, title, header, report, max.y),
                Dialog::Input {
                    title,
                    prompt,
                    value,
                    purpose,
                    sides,
                    error,
                    focused,
                } => input(ui, title, prompt, value, *purpose, sides, error, focused),
                Dialog::DeleteSide { nodes } => delete_side(ui, nodes),
                Dialog::Settings {
                    edit,
                    tolerance,
                    threshold,
                    error,
                } => settings(ui, edit, tolerance, threshold, error),
            };
        });
        if outcome.is_none() && response.should_close() {
            // Closing the overwrite dialog counts as "Cancel all".
            if let Dialog::Overwrite { reply, .. } = self {
                let _ = reply.send(OverwriteDecision {
                    choice: OverwriteChoice::Cancel,
                    apply_to_all: false,
                });
            }
            outcome = Some(Outcome::Closed);
        }
        outcome
    }
}

fn message(ui: &mut Ui, title: &str, text: &str, error: bool) -> Option<Outcome> {
    title_line(ui, title);
    let color = if error { theme::ERROR } else { theme::TEXT };
    ui.label(RichText::new(text).color(color));
    ui.add_space(8.0);
    let ok = buttons_right(ui, |ui| ui.add(button("OK")).clicked());
    (ok || enter(ui)).then_some(Outcome::Closed)
}

fn confirm(
    ui: &mut Ui,
    title: &str,
    text: &str,
    warning: bool,
    action: &ConfirmAction,
) -> Option<Outcome> {
    title_line(ui, title);
    let color = if warning { theme::ERROR } else { theme::TEXT };
    ui.label(RichText::new(text).color(color));
    ui.add_space(8.0);
    // Default is No: Enter answers No.
    let (yes, no) = buttons_right(ui, |ui| {
        let no = ui.add(button("No")).clicked();
        let yes = ui.add(button("Yes")).clicked();
        (yes, no)
    });
    if yes {
        Some(Outcome::Confirmed(action.clone()))
    } else if no || enter(ui) {
        Some(Outcome::Closed)
    } else {
        None
    }
}

fn action_name(a: &PlannedAction) -> &'static str {
    match a.kind {
        ActionKind::CopyFile => "Copy",
        ActionKind::CreateDirectory => "Create folder",
        ActionKind::Delete if a.is_dir => "Delete folder",
        ActionKind::Delete => "Delete file",
    }
}

fn overwrite_text(a: &PlannedAction) -> (&'static str, bool) {
    if a.kind != ActionKind::CopyFile || !a.target_exists {
        ("", false)
    } else if a.target_is_newer {
        ("yes - destination is newer!", true)
    } else if a.direction_unknown {
        ("ask - same time, different content!", true)
    } else {
        ("yes", false)
    }
}

/// The header line of the preview.
pub fn preview_header(op: &PendingOp) -> String {
    let plan = &op.plan;
    if op.is_delete {
        format!(
            "{}: {} item(s), {}",
            if op.permanent {
                "Permanently delete"
            } else {
                "Move to the Recycle Bin"
            },
            format_count(plan.actions.len() as u64),
            format_size(plan.total_bytes())
        )
    } else {
        let files = plan
            .actions
            .iter()
            .filter(|a| a.kind != ActionKind::CreateDirectory)
            .count();
        let folders = plan.actions.len() - files;
        let mut s = format!(
            "{} file(s) to copy, {}",
            format_count(files as u64),
            format_size(plan.total_bytes())
        );
        if folders > 0 {
            s.push_str(&format!(
                ", {} folder(s) to create",
                format_count(folders as u64)
            ));
        }
        s
    }
}

/// Only the first warning that applies.
pub fn preview_warning(op: &PendingOp) -> Option<String> {
    let a = &op.plan.actions;
    let newer = a.iter().filter(|a| a.target_is_newer).count();
    let unknown = a.iter().filter(|a| a.direction_unknown).count();
    let overwrites = a
        .iter()
        .filter(|a| a.target_exists && a.kind == ActionKind::CopyFile)
        .count();
    if op.permanent {
        Some(
            "Items will be deleted permanently and cannot be restored from the Recycle Bin.".into(),
        )
    } else if newer > 0 {
        Some(format!(
            "{} file(s) at the destination are NEWER than the source. Check the copy direction.",
            format_count(newer as u64)
        ))
    } else if unknown > 0 {
        Some(format!(
            "{} file(s) have the same timestamp but different content. You will be asked before each is overwritten.",
            format_count(unknown as u64)
        ))
    } else if overwrites > 0 {
        Some(format!(
            "{} existing file(s) at the destination will be overwritten (subject to the overwrite policy).",
            format_count(overwrites as u64)
        ))
    } else {
        None
    }
}

pub fn notes_text(notes: &[String]) -> Option<String> {
    if notes.is_empty() {
        return None;
    }
    let shown: Vec<&str> = notes.iter().take(20).map(String::as_str).collect();
    Some(format!(
        "Not included: {}{}",
        shown.join("  |  "),
        if notes.len() > 20 { " ..." } else { "" }
    ))
}

fn preview(
    ui: &mut Ui,
    op: &PendingOp,
    offer_dont_ask: bool,
    dont_ask: &mut bool,
    max_height: f32,
) -> Option<Outcome> {
    title_line(ui, &op.title);
    ui.label(RichText::new(preview_header(op)).strong());
    if let Some(w) = preview_warning(op) {
        ui.add_space(4.0);
        message_bar(ui, &w, theme::WARNING_BAR, theme::TEXT);
    }
    ui.add_space(4.0);
    let table_height = (max_height - 200.0).clamp(120.0, 520.0 - 150.0);
    let widths = [100.0, 0.0, 0.0, 80.0, 190.0];
    let width = ui.available_width();
    let path_width = ((width - widths[0] - widths[3] - widths[4] - 20.0) / 2.0).max(120.0);
    let cols = [widths[0], path_width, path_width, widths[3], widths[4]];
    let header = ["Action", "Source", "Destination", "Size", "Overwrite"];
    ui.horizontal(|ui| {
        for (h, w) in header.iter().zip(cols) {
            ui.add_sized(
                [w, 18.0],
                egui::Label::new(RichText::new(*h).strong()).truncate(),
            );
        }
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt("preview")
        .max_height(table_height)
        .auto_shrink([false, true])
        .show_rows(ui, 18.0, op.plan.actions.len(), |ui, range| {
            for a in &op.plan.actions[range] {
                ui.horizontal(|ui| {
                    let (over, warn) = overwrite_text(a);
                    let size = if a.kind == ActionKind::CreateDirectory {
                        String::new()
                    } else {
                        format_size(a.size)
                    };
                    let cells = [
                        action_name(a).to_string(),
                        a.source_path
                            .as_deref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                        a.target_path.display().to_string(),
                        size,
                        over.to_string(),
                    ];
                    for (i, (text, w)) in cells.iter().zip(cols).enumerate() {
                        let mut t = RichText::new(text.as_str());
                        if i == 4 && warn {
                            t = t.color(theme::ERROR);
                        }
                        ui.add_sized([w, 18.0], egui::Label::new(t).truncate())
                            .on_hover_text(text.as_str());
                    }
                });
            }
        });
    if let Some(n) = notes_text(&op.plan.notes) {
        ui.add_space(4.0);
        ui.label(RichText::new(n).color(theme::OLDER));
    }
    ui.add_space(8.0);
    let mut result = None;
    ui.horizontal(|ui| {
        if offer_dont_ask {
            ui.checkbox(dont_ask, "Don't ask again");
        }
        buttons_right(ui, |ui| {
            if ui.add(button("Cancel")).clicked() {
                result = Some(Outcome::Closed);
            }
            let ok = ui.add(button(if op.is_delete { "Delete" } else { "Copy" }));
            if ok.clicked() {
                result = Some(Outcome::Preview {
                    op: op.clone(),
                    dont_ask_again: *dont_ask,
                });
            }
        });
    });
    if result.is_none() && enter(ui) {
        result = Some(Outcome::Preview {
            op: op.clone(),
            dont_ask_again: *dont_ask,
        });
    }
    result
}

fn overwrite(
    ui: &mut Ui,
    c: &OverwriteConflict,
    apply_to_all: &mut bool,
    reply: &Sender<OverwriteDecision>,
) -> Option<Outcome> {
    title_line(ui, "File already exists");
    if c.target_is_newer {
        ui.label(RichText::new("Warning: the destination file is NEWER than the source. Overwriting it replaces the newer version - is the copy direction correct?").color(theme::ERROR).strong());
    }
    if c.direction_unknown {
        ui.label(RichText::new("Both files have the same timestamp but different content, so it is unknown which one is newer. Compare the contents before overwriting.").color(theme::CONFLICT).strong());
    }
    ui.add_space(4.0);
    egui::Grid::new("overwrite")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            let describe =
                |size, time| format!("{}, modified {}", format_size(size), format_date(time));
            ui.label(RichText::new("Source:").strong());
            ui.label(c.source_path.display().to_string());
            ui.end_row();
            ui.label("");
            ui.label(describe(c.source_size, c.source_time));
            ui.end_row();
            ui.label(RichText::new("Destination:").strong());
            ui.label(c.target_path.display().to_string());
            ui.end_row();
            ui.label("");
            ui.label(describe(c.target_size, c.target_time));
            ui.end_row();
        });
    ui.add_space(4.0);
    ui.checkbox(apply_to_all, "Apply to all remaining conflicts");
    ui.add_space(8.0);
    let mut choice = None;
    // A newer destination or an unknown direction defaults to the safe choice.
    let default_skip = c.target_is_newer || c.direction_unknown;
    buttons_right(ui, |ui| {
        if ui.add(button("Cancel all")).clicked() {
            choice = Some(OverwriteChoice::Cancel);
        }
        if ui.add(default_button("Skip", default_skip)).clicked() {
            choice = Some(OverwriteChoice::Skip);
        }
        if ui.add(button("Keep both")).clicked() {
            choice = Some(OverwriteChoice::KeepBoth);
        }
        if ui.add(button("Overwrite if newer")).clicked() {
            choice = Some(OverwriteChoice::OverwriteIfNewer);
        }
        if ui.add(default_button("Overwrite", !default_skip)).clicked() {
            choice = Some(OverwriteChoice::Overwrite);
        }
    });
    if choice.is_none() && enter(ui) {
        choice = Some(if default_skip {
            OverwriteChoice::Skip
        } else {
            OverwriteChoice::Overwrite
        });
    }
    let choice = choice?;
    let _ = reply.send(OverwriteDecision {
        choice,
        apply_to_all: *apply_to_all,
    });
    Some(Outcome::Closed)
}

fn summary(
    ui: &mut Ui,
    title: &str,
    header: &str,
    report: &str,
    max_height: f32,
) -> Option<Outcome> {
    title_line(ui, title);
    ui.label(RichText::new(header).strong());
    ui.add_space(4.0);
    egui::ScrollArea::vertical()
        .id_salt("summary")
        .max_height((max_height - 160.0).clamp(100.0, 400.0))
        .show(ui, |ui| {
            ui.add(
                TextEdit::multiline(&mut &*report)
                    .font(theme::mono(12.0))
                    .desired_width(f32::INFINITY),
            );
        });
    ui.add_space(8.0);
    let mut result = None;
    buttons_right(ui, |ui| {
        if ui.add(button("Close")).clicked() {
            result = Some(Outcome::Closed);
        }
        if ui.add(button("Copy report to clipboard")).clicked() {
            ui.ctx().copy_text(report.to_string());
        }
    });
    if result.is_none() && enter(ui) {
        result = Some(Outcome::Closed);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn input(
    ui: &mut Ui,
    title: &str,
    prompt: &str,
    value: &mut String,
    purpose: InputPurpose,
    sides: &mut Option<RenameSides>,
    error: &mut Option<String>,
    focused: &mut bool,
) -> Option<Outcome> {
    title_line(ui, title);
    ui.label(prompt);
    let id = egui::Id::new("fc-input");
    let edit = ui.add(TextEdit::singleline(value).id(id).desired_width(420.0));
    if !*focused {
        *focused = true;
        edit.request_focus();
        // Rename preselects the name without its extension.
        let end = match (purpose, value.rfind('.')) {
            (InputPurpose::Rename(_), Some(dot)) if dot > 0 => value[..dot].chars().count(),
            _ => value.chars().count(),
        };
        if let Some(mut s) = TextEdit::load_state(ui.ctx(), id) {
            s.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(end),
            )));
            s.store(ui.ctx(), id);
        }
    }
    if let Some(s) = sides {
        ui.horizontal(|ui| {
            ui.label("Rename on:");
            ui.add_enabled(s.left_enabled, egui::Checkbox::new(&mut s.left, "Left"));
            ui.add_enabled(s.right_enabled, egui::Checkbox::new(&mut s.right, "Right"));
        });
    }
    if let Some(e) = error {
        ui.label(RichText::new(e.as_str()).color(theme::ERROR));
    }
    ui.add_space(8.0);
    let mut ok = false;
    let mut cancel = false;
    buttons_right(ui, |ui| {
        cancel = ui.add(button("Cancel")).clicked();
        ok = ui.add(button("OK")).clicked();
    });
    if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
        ok = true;
    }
    if cancel {
        return Some(Outcome::Closed);
    }
    if !ok {
        return None;
    }
    let v = value.trim().to_string();
    if v.is_empty() {
        return None;
    }
    let (left, right) = match sides {
        Some(s) => {
            if !is_valid_name(&v) {
                *error = Some("The name contains invalid characters.".into());
                return None;
            }
            if !s.left && !s.right {
                return None;
            }
            (s.left, s.right)
        }
        None => (false, false),
    };
    Some(Outcome::Input {
        purpose,
        value: v,
        left,
        right,
    })
}

fn delete_side(ui: &mut Ui, nodes: &[NodeId]) -> Option<Outcome> {
    title_line(ui, "Delete");
    ui.label("Some selected items exist on both sides. Which side should be deleted?");
    ui.add_space(8.0);
    let mut sides = None;
    let mut cancel = false;
    buttons_right(ui, |ui| {
        cancel = ui.add(default_button("Cancel", true)).clicked();
        if ui.add(button("Both")).clicked() {
            sides = Some(Sides::Both);
        }
        if ui.add(button("Right")).clicked() {
            sides = Some(Sides::Right);
        }
        if ui.add(button("Left")).clicked() {
            sides = Some(Sides::Left);
        }
    });
    if let Some(sides) = sides {
        return Some(Outcome::DeleteSide {
            sides,
            nodes: nodes.to_vec(),
        });
    }
    (cancel || enter(ui)).then_some(Outcome::Closed)
}

fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(theme::FORMAT_TEXT));
}

fn settings(
    ui: &mut Ui,
    s: &mut AppSettings,
    tolerance: &mut String,
    threshold: &mut String,
    error: &mut Option<String>,
) -> Option<Outcome> {
    title_line(ui, "Settings");
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("Comparison").strong());
        egui::Grid::new("settings-cmp").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
            ui.label("Timestamp tolerance (seconds):");
            ui.add(TextEdit::singleline(tolerance).desired_width(80.0));
            ui.end_row();
        });
        hint(ui, "Files saved within this window on both sides are treated as same-time; real changes in that window can be missed unless content verification is on.");
        ui.checkbox(&mut s.ignore_dst_offset, "Ignore exactly ±1 hour differences (DST / time zone)");
        hint(ui, "A real change exactly 1 hour apart will be treated as same-time.");
        egui::Grid::new("settings-cmp2").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
            ui.label("Verify file contents:");
            egui::ComboBox::from_id_salt("content")
                .width(240.0)
                .selected_text(content_name(s.content_compare))
                .show_ui(ui, |ui| {
                    for v in [
                        ContentCompareSetting::Off,
                        ContentCompareSetting::WhenTimestampsEqual,
                        ContentCompareSetting::WhenSizesEqual,
                    ] {
                        ui.selectable_value(&mut s.content_compare, v, content_name(v));
                    }
                });
            ui.end_row();
        });
        hint(ui, "File size is always compared. Verification reads both files byte by byte after the scan, which takes longer for large folders.");
        egui::Grid::new("settings-cmp3").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
            ui.label("Symbolic links / junctions:");
            egui::ComboBox::from_id_salt("symlinks")
                .selected_text(format!("{:?}", s.symlink_mode))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.symlink_mode, SymlinkSetting::Skip, "Skip");
                    ui.selectable_value(&mut s.symlink_mode, SymlinkSetting::Follow, "Follow");
                });
            ui.end_row();
        });
        ui.checkbox(&mut s.ignore_hidden_and_system, "Ignore hidden and system files");
        ui.checkbox(&mut s.recursive, "Compare subfolders (recursive)");
        egui::Grid::new("settings-cmp4").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
            ui.label("Exclude patterns:");
            ui.add(TextEdit::singleline(&mut s.exclude_patterns).desired_width(320.0))
                .on_hover_text("e.g. *.tmp;.git;node_modules;bin;obj");
            ui.end_row();
            ui.label("Include patterns:");
            ui.add(TextEdit::singleline(&mut s.include_patterns).desired_width(320.0))
                .on_hover_text("Empty = all files");
            ui.end_row();
        });
    });
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("File operations").strong());
        egui::Grid::new("settings-ops")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("Overwrite policy:");
                egui::ComboBox::from_id_salt("overwrite")
                    .selected_text(format!("{:?}", s.overwrite_policy))
                    .show_ui(ui, |ui| {
                        for v in [
                            OverwriteSetting::Ask,
                            OverwriteSetting::Skip,
                            OverwriteSetting::Overwrite,
                            OverwriteSetting::OverwriteIfNewer,
                            OverwriteSetting::KeepBoth,
                        ] {
                            ui.selectable_value(&mut s.overwrite_policy, v, format!("{v:?}"));
                        }
                    });
                ui.end_row();
            });
        ui.checkbox(
            &mut s.delete_to_recycle_bin,
            format!(
                "Delete to the {} (unchecked = delete permanently)",
                crate::platform::trash_name()
            ),
        );
        ui.checkbox(
            &mut s.confirm_recycle_delete,
            format!("Confirm {} deletes", crate::platform::trash_name()),
        );
        egui::Grid::new("settings-ops2")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("Skip copy preview for fewer than (items):");
                ui.add(TextEdit::singleline(threshold).desired_width(80.0))
                    .on_hover_text("0 = always show the preview");
                ui.end_row();
            });
    });
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("View").strong());
        ui.checkbox(&mut s.folders_first, "Sort folders before files");
    });
    if let Some(e) = error {
        ui.label(RichText::new(e.as_str()).color(theme::ERROR));
    }
    ui.add_space(8.0);
    let mut ok = false;
    let mut cancel = false;
    buttons_right(ui, |ui| {
        cancel = ui.add(button("Cancel")).clicked();
        ok = ui.add(button("OK")).clicked();
    });
    if cancel {
        return Some(Outcome::Closed);
    }
    if !ok && !(ui.input(|i| i.key_pressed(Key::Enter)) && !ui.ctx().text_edit_focused()) {
        return None;
    }
    let (Ok(t), Ok(p)) = (
        tolerance.trim().parse::<f64>(),
        threshold.trim().parse::<i64>(),
    ) else {
        *error = Some("Please correct the highlighted values.".into());
        return None;
    };
    if t < 0.0 || p < 0 || !t.is_finite() {
        *error = Some("Values must not be negative.".into());
        return None;
    }
    s.tolerance_seconds = t;
    s.preview_threshold = p;
    Some(Outcome::Settings(Box::new(s.clone())))
}

fn content_name(v: ContentCompareSetting) -> &'static str {
    match v {
        ContentCompareSetting::Off => "Off (fast)",
        ContentCompareSetting::WhenTimestampsEqual => "Verify files that look identical",
        ContentCompareSetting::WhenSizesEqual => "Verify all same-size files",
    }
}

pub fn title_line(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).strong().size(15.0));
    ui.add_space(6.0);
}

fn button(text: &str) -> egui::Button<'_> {
    egui::Button::new(text).min_size(vec2(76.0, 24.0))
}

/// The default button (Enter) is drawn with a highlighted border.
fn default_button(text: &str, is_default: bool) -> egui::Button<'_> {
    let b = button(text);
    if is_default {
        b.stroke((1.5, theme::SELECTION_FOCUS))
    } else {
        b
    }
}

/// Lays buttons out right-aligned (the closure adds them right to left).
pub fn buttons_right<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let size = vec2(ui.available_width(), 26.0);
    ui.allocate_ui_with_layout(size, Layout::right_to_left(Align::Center), add)
        .inner
}

fn enter(ui: &Ui) -> bool {
    !ui.ctx().text_edit_focused() && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter))
}

/// A colored bar with a message (warnings in dialogs, the root error in the main window).
pub fn message_bar(ui: &mut Ui, text: &str, fill: Color32, color: Color32) {
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).color(color));
        });
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn action(kind: ActionKind, exists: bool, newer: bool, unknown: bool) -> PlannedAction {
        PlannedAction {
            kind,
            node: NodeId(1),
            relative_path: "a".into(),
            target_side: fc_core::model::Side::Right,
            source_path: Some(PathBuf::from("s")),
            target_path: PathBuf::from("t"),
            is_dir: false,
            size: 1024,
            source_time: None,
            target_time: None,
            target_exists: exists,
            target_is_newer: newer,
            direction_unknown: unknown,
        }
    }

    fn op(actions: Vec<PlannedAction>, is_delete: bool, permanent: bool) -> PendingOp {
        PendingOp {
            title: "t".into(),
            plan: OperationPlan {
                actions,
                notes: Vec::new(),
            },
            selection: Vec::new(),
            is_delete,
            permanent,
        }
    }

    #[test]
    fn preview_texts() {
        let copy = op(
            vec![
                action(ActionKind::CopyFile, true, false, true),
                action(ActionKind::CopyFile, true, true, false),
                action(ActionKind::CreateDirectory, false, false, false),
            ],
            false,
            false,
        );
        assert_eq!(
            "2 file(s) to copy, 3.0 KB, 1 folder(s) to create",
            preview_header(&copy)
        );
        assert_eq!(
            Some(
                "1 file(s) at the destination are NEWER than the source. Check the copy direction."
                    .into()
            ),
            preview_warning(&copy)
        );
        let del = op(
            vec![action(ActionKind::Delete, true, false, false)],
            true,
            true,
        );
        assert_eq!(
            "Permanently delete: 1 item(s), 1.0 KB",
            preview_header(&del)
        );
        assert!(
            preview_warning(&del)
                .unwrap()
                .starts_with("Items will be deleted permanently")
        );
        let notes: Vec<String> = (0..25).map(|i| format!("n{i}")).collect();
        let text = notes_text(&notes).unwrap();
        assert!(text.starts_with("Not included: n0  |  n1"));
        assert!(text.ends_with("n19 ..."));
    }
}
