//! Rendering of the main window.

use egui::{Align, Button, Key, Layout, Modifiers, RichText, TextEdit, Ui, vec2};
use fc_core::model::{DiffStatus, NodeId, Side};
use fc_core::ops::{CopyDirection, Sides};

use crate::actions::ShellAction;
use crate::app::{Busy, FolderCompareApp};
use crate::dialogs::message_bar;
use crate::platform;
use crate::result_list;
use crate::theme::{self, icons};

const COMBO_BUTTON: f32 = 22.0;
const BROWSE: f32 = 30.0;
const SWAP: f32 = 34.0;

impl FolderCompareApp {
    pub fn main_ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.handle_keys(&ctx);
        self.handle_dropped_files(&ctx);

        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        let mut swap_center = None;
        egui::Panel::top("folders")
            .frame(
                egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 6,
                        right: 6,
                        top: 6,
                        bottom: 2,
                    })
                    .fill(ui.visuals().panel_fill),
            )
            .show(ui, |ui| swap_center = self.folder_bar(ui));
        egui::Panel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .fill(ui.visuals().panel_fill),
            )
            .show(ui, |ui| self.toolbars(ui));
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .fill(ui.visuals().panel_fill),
            )
            .show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::no_frame()
            .frame(egui::Frame::new().fill(theme::EDITOR_BACKGROUND))
            .show(ui, |ui| {
                let rect = ui.max_rect();
                result_list::fit_columns(&mut self.columns, rect.left(), rect.width(), swap_center);
                let mut list = std::mem::take(&mut self.list);
                let mut columns = self.columns;
                let enables = Enables::of(self);
                let mut chosen = None;
                let events = list.show(ui, &mut self.state, &mut columns, &mut |ui| {
                    context_menu(ui, &enables, &mut chosen);
                });
                self.columns = columns;
                if let Some(c) = chosen {
                    self.run_menu_command(c);
                }
                self.list = list;
                if let Some(key) = events.sort {
                    self.state.sort_by(key);
                }
                if let Some(id) = events.double_clicked {
                    self.activate(id);
                }
            });
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        let busy = self.is_busy();
        let ctrl = platform::ctrl_name();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui
                    .add_enabled(
                        self.can_compare(),
                        Button::new("Compare / Refresh").shortcut_text("F5"),
                    )
                    .clicked()
                {
                    self.start_compare();
                }
                if ui
                    .add_enabled(
                        self.state.can_refresh_selected(busy),
                        Button::new("Rescan selected"),
                    )
                    .clicked()
                {
                    self.rescan_selected();
                }
                if ui
                    .add_enabled(
                        self.state.can_export(busy),
                        Button::new("Export report...").shortcut_text(format!("{ctrl}+E")),
                    )
                    .clicked()
                {
                    self.export_report();
                }
                ui.separator();
                if ui.button("Open log folder").clicked() {
                    self.open_log_folder();
                }
                ui.separator();
                if ui.button("Exit").clicked() {
                    self.exit();
                }
            });
            ui.menu_button("View", |ui| {
                if ui.button("Expand all").clicked() {
                    self.state.expand_all();
                }
                if ui.button("Collapse all").clicked() {
                    self.state.collapse_all();
                }
                ui.separator();
                let mut folders_first = self.state.folders_first;
                if ui
                    .checkbox(&mut folders_first, "Folders before files")
                    .changed()
                {
                    self.settings.folders_first = folders_first;
                    self.state.set_folders_first(folders_first);
                }
            });
            ui.menu_button("Profiles", |ui| {
                if ui.button("Save current as profile...").clicked() {
                    self.ask_save_profile();
                }
                if ui
                    .add_enabled(
                        self.selected_profile.is_some(),
                        Button::new("Delete selected profile"),
                    )
                    .clicked()
                {
                    self.delete_profile();
                }
            });
            ui.menu_button("Tools", |ui| {
                if ui
                    .add_enabled(
                        self.state.can_compare_contents(busy),
                        Button::new("Compare contents...").shortcut_text("Enter"),
                    )
                    .clicked()
                {
                    self.compare_contents();
                }
                ui.separator();
                if crate::explorer::is_supported() {
                    let mut on = self.explorer_integration;
                    let changed = ui
                        .checkbox(&mut on, "Explorer context menu integration")
                        .on_hover_text("Adds \"Select as left\", \"Compare to ...\" and \"Compare Folders/Files\" to the right-click menu of files and folders in Explorer")
                        .changed();
                    if changed {
                        let result = if on { crate::explorer::register() } else { crate::explorer::unregister() };
                        if let Err(e) = result {
                            tracing::error!("Could not change the Explorer context menu integration: {e}");
                            self.dialogs.push(crate::dialogs::Dialog::error("Explorer context menu", e));
                        }
                        self.explorer_integration = crate::explorer::is_registered();
                    }
                    ui.separator();
                }
                if ui.button("Settings...").clicked() {
                    self.open_settings();
                }
            });
        });
    }

    /// Returns the screen x of the swap button's center (the status column is aligned to it).
    fn folder_bar(&mut self, ui: &mut Ui) -> Option<f32> {
        let mut swap_center = None;
        let spacing = 3.0;
        let compare_width = 82.0;
        let fixed = 2.0 * (COMBO_BUTTON + spacing + BROWSE)
            + 8.0
            + SWAP
            + 8.0
            + 8.0
            + compare_width
            + 4.0 * spacing;
        let combo = ((ui.available_width() - fixed) / 2.0).max(100.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = spacing;
            self.path_box(ui, true, combo);
            if ui
                .add_sized([BROWSE, 22.0], Button::new("..."))
                .on_hover_text("Browse for the left folder")
                .clicked()
            {
                self.browse(true);
            }
            ui.add_space(8.0);
            let swap = ui
                .add_enabled_ui(!self.is_busy(), |ui| {
                    ui.add_sized(
                        [SWAP, 22.0],
                        Button::new(RichText::new(icons::ARROWS_LEFT_RIGHT).size(15.0)),
                    )
                })
                .inner
                .on_hover_text("Swap left and right");
            swap_center = Some(swap.rect.center().x);
            if swap.clicked() {
                self.swap();
            }
            ui.add_space(8.0);
            self.path_box(ui, false, combo);
            if ui
                .add_sized([BROWSE, 22.0], Button::new("..."))
                .on_hover_text("Browse for the right folder")
                .clicked()
            {
                self.browse(false);
            }
            ui.add_space(8.0);
            let compare = ui
                .add_enabled(
                    self.can_compare(),
                    Button::new(RichText::new("Compare").strong())
                        .min_size(vec2(compare_width, 22.0)),
                )
                .on_hover_text("Compare / refresh (F5)");
            if compare.clicked() {
                self.start_compare();
            }
        });
        if self.left_error.is_some() || self.right_error.is_some() {
            ui.horizontal(|ui| {
                let width = combo + COMBO_BUTTON + 3.0 + BROWSE + 8.0 + SWAP + 8.0;
                ui.allocate_ui_with_layout(
                    vec2(width, 16.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        if let Some(e) = &self.left_error {
                            ui.label(RichText::new(e).color(theme::ERROR));
                        }
                    },
                );
                if let Some(e) = &self.right_error {
                    ui.label(RichText::new(e).color(theme::ERROR));
                }
            });
        }
        if let Some(e) = self.root_error.clone() {
            ui.add_space(2.0);
            message_bar(ui, &e, theme::MESSAGE_BAR, theme::ERROR);
        }
        swap_center
    }

    /// An editable path with a drop-down of recent paths (and a drop target for files and folders).
    fn path_box(&mut self, ui: &mut Ui, left: bool, width: f32) {
        let (text, hint, tip) = if left {
            (
                &mut self.left_path,
                "Left folder",
                "Left folder (type, paste or drop a folder)",
            )
        } else {
            (
                &mut self.right_path,
                "Right folder",
                "Right folder (type, paste or drop a folder)",
            )
        };
        let edit = ui
            .add(
                TextEdit::singleline(text)
                    .desired_width(width)
                    .hint_text(hint),
            )
            .on_hover_text(tip);
        if left {
            self.left_box = Some(edit.rect);
        } else {
            self.right_box = Some(edit.rect);
        }
        let mut changed = edit.changed();
        let mut picked = None;
        ui.menu_button(icons::CARET_DOWN, |ui| {
            ui.set_min_width(width);
            if self.recent.is_empty() {
                ui.label(RichText::new("No recent folders").weak());
            }
            for p in &self.recent {
                if ui.button(p.as_str()).clicked() {
                    picked = Some(p.clone());
                }
            }
        });
        if let Some(p) = picked {
            if left {
                self.left_path = p;
            } else {
                self.right_path = p;
            }
            changed = true;
        }
        if changed {
            self.validate_paths();
        }
        if self.drop_hover == Some(left) {
            ui.painter().rect_stroke(
                edit.rect,
                2.0,
                (2.0, theme::ONLY),
                egui::StrokeKind::Outside,
            );
        }
    }

    fn toolbars(&mut self, ui: &mut Ui) {
        let busy = self.is_busy();
        let ctrl = platform::ctrl_name();
        ui.horizontal_wrapped(|ui| {
            ui.label("Show:");
            let f = &mut self.state.filters;
            let mut changed = false;
            changed |= ui.toggle_value(&mut f.show_only_left, RichText::new("Only left").color(theme::ONLY)).on_hover_text("Show items that exist only on the left").changed();
            changed |= ui.toggle_value(&mut f.show_newer_left, RichText::new("Newer left").color(theme::NEWER).family(theme::bold_family())).on_hover_text("Show items that are newer on the left").changed();
            changed |= ui.toggle_value(&mut f.show_identical, "Identical").on_hover_text("Show identical items").changed();
            changed |= ui.toggle_value(&mut f.show_time_only, "Time only").on_hover_text("Show files whose content was verified identical but whose timestamps differ").changed();
            changed |= ui.toggle_value(&mut f.show_newer_right, RichText::new("Newer right").color(theme::NEWER).family(theme::bold_family())).on_hover_text("Show items that are newer on the right").changed();
            changed |= ui.toggle_value(&mut f.show_only_right, RichText::new("Only right").color(theme::ONLY)).on_hover_text("Show items that exist only on the right").changed();
            changed |= ui.toggle_value(&mut f.show_same_time_different, RichText::new("Same time, different").color(theme::CONFLICT).family(theme::bold_family())).on_hover_text("Show files with the same timestamp but different size or content (direction unknown)").changed();
            changed |= ui.toggle_value(&mut f.show_errors, RichText::new("Errors").color(theme::ERROR)).on_hover_text("Show errors and file/folder conflicts").changed();
            ui.separator();
            changed |= ui.toggle_value(&mut f.differences_only, "Differences only").on_hover_text("Hide identical items and folders without differences").changed();
            if changed {
                self.filters_changed();
            }
            ui.separator();
            ui.label("Search:");
            let search = ui
                .add(TextEdit::singleline(&mut self.search_text).desired_width(160.0).id(egui::Id::new("search")))
                .on_hover_text(format!("Filter by file or folder name ({ctrl}+F)"));
            if search.changed() {
                self.search_edited();
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(self.state.can_copy_to_right(busy), Button::new(format!("Copy {}", icons::ARROW_RIGHT)))
                .on_hover_text(format!("Copy selection to the right ({ctrl}+Right)"))
                .clicked()
            {
                self.copy(CopyDirection::LeftToRight);
            }
            if ui.add_enabled(self.state.can_copy_to_left(busy), Button::new(format!("{} Copy", icons::ARROW_LEFT)))
                .on_hover_text(format!("Copy selection to the left ({ctrl}+Left)"))
                .clicked()
            {
                self.copy(CopyDirection::RightToLeft);
            }
            if ui.add_enabled(self.state.can_delete_any(busy), Button::new("Delete")).on_hover_text("Delete selection (Del)").clicked() {
                self.delete();
            }
            ui.separator();
            if ui.add_enabled(self.can_compare(), Button::new("Refresh")).on_hover_text("Rescan (F5)").clicked() {
                self.start_compare();
            }
            if ui.button("Expand all").on_hover_text("Expand all folders").clicked() {
                self.state.expand_all();
            }
            if ui.button("Collapse all").on_hover_text("Collapse all folders").clicked() {
                self.state.collapse_all();
            }
            if ui.button("Options").on_hover_text("Settings").clicked() {
                self.open_settings();
            }
            ui.separator();
            ui.label("Exclude:");
            ui.add(TextEdit::singleline(&mut self.exclude).desired_width(200.0))
                .on_hover_text("Glob patterns to skip, separated by ';' (e.g. *.tmp;.git;node_modules;bin;obj). Applied on the next scan.");
            ui.label("Include:");
            ui.add(TextEdit::singleline(&mut self.include).desired_width(120.0))
                .on_hover_text("Only compare files matching these patterns, separated by ';' (empty = all files). Applied on the next scan.");
            ui.separator();
            ui.label("Profile:");
            let current = self
                .selected_profile
                .and_then(|i| self.settings.profiles.get(i))
                .map(|p| p.name.clone())
                .unwrap_or_default();
            let mut chosen = None;
            egui::ComboBox::from_id_salt("profile")
                .width(140.0)
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (i, p) in self.settings.profiles.iter().enumerate() {
                        if ui.selectable_label(self.selected_profile == Some(i), p.name.as_str()).clicked() {
                            chosen = Some(i);
                        }
                    }
                })
                .response
                .on_hover_text("Load a saved folder pair with its options");
            if let Some(i) = chosen {
                self.apply_profile(i);
            }
            if ui.button("Save").on_hover_text("Save the current folders and options as a profile").clicked() {
                self.ask_save_profile();
            }
            if ui.add_enabled(self.selected_profile.is_some(), Button::new("Remove")).on_hover_text("Delete the selected profile").clicked() {
                self.delete_profile();
            }
        });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.is_busy() {
                    if ui.button("Cancel (Esc)").clicked() {
                        self.cancel();
                    }
                    if self.busy == Busy::Operating {
                        ui.add(
                            egui::ProgressBar::new(self.file_progress)
                                .desired_width(80.0)
                                .desired_height(12.0),
                        )
                        .on_hover_text("Current file");
                    }
                    progress_bar(ui, self.progress, self.progress_indeterminate)
                        .on_hover_text("Overall progress");
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.label(&self.counts_text);
                    ui.separator();
                    ui.label(self.state.selection_text());
                    ui.separator();
                    ui.add(egui::Label::new(&self.status_text).truncate());
                });
            });
        });
    }

    // ---- Input -------------------------------------------------------------------------------

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if !self.dialogs.is_empty() {
            return;
        }
        let cmd = Modifiers::COMMAND;
        let busy = self.is_busy();
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F5)) && self.can_compare() {
            self.start_compare();
        }
        if busy && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.cancel();
        }
        if ctx.input_mut(|i| i.consume_key(cmd, Key::E)) {
            self.export_report();
        }
        if ctx.input_mut(|i| i.consume_key(cmd, Key::F)) {
            let id = egui::Id::new("search");
            ctx.memory_mut(|m| m.request_focus(id));
            if let Some(mut s) = egui::TextEdit::load_state(ctx, id) {
                s.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(self.search_text.chars().count()),
                )));
                s.store(ctx, id);
            }
        }

        // The remaining shortcuts would conflict with text editing.
        if ctx.text_edit_focused() {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(cmd, Key::A)) {
            self.state.select_all();
        }
        if ctx.input_mut(|i| i.consume_key(cmd, Key::ArrowRight)) {
            self.copy(CopyDirection::LeftToRight);
        }
        if ctx.input_mut(|i| i.consume_key(cmd, Key::ArrowLeft)) {
            self.copy(CopyDirection::RightToLeft);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)) {
            self.delete();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F2)) {
            self.ask_rename();
        }

        let shift = ctx.input(|i| i.modifiers.shift);
        let page = ((self.list_height() / result_list::ROW_HEIGHT) as isize - 1).max(1);
        let moves = [
            (Key::ArrowDown, 1),
            (Key::ArrowUp, -1),
            (Key::PageDown, page),
            (Key::PageUp, -page),
            (Key::End, isize::MAX / 2),
            (Key::Home, -(isize::MAX / 2)),
        ];
        for (key, delta) in moves {
            let ctrl_held = ctx.input(|i| i.modifiers.command);
            if ctx.input_mut(|i| i.consume_key(if ctrl_held { cmd } else { Modifiers::NONE }, key))
            {
                self.state.move_focus(delta, shift, ctrl_held);
                self.list.scroll_to = self.state.focus;
            }
        }
        if let Some(focus) = self.state.focus {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowRight)) {
                if self.state.is_expandable(focus) && !self.state.is_expanded(focus) {
                    self.state.set_expanded(focus, true);
                }
            } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowLeft)) {
                if self.state.is_expanded(focus) {
                    self.state.set_expanded(focus, false);
                } else {
                    self.state.focus_parent();
                    self.list.scroll_to = self.state.focus;
                }
            } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space)) {
                self.state.toggle_focused();
            } else if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
                self.activate(focus);
            }
        }
    }

    fn list_height(&self) -> f32 {
        self.ctx.content_rect().height() - 200.0
    }

    /// Enter or double-click: toggles a folder; opens the text compare for a file.
    pub fn activate(&mut self, id: NodeId) {
        if self.state.is_expandable(id) {
            self.state.toggle_expanded(id);
        } else if !self.state.node(id).is_dir {
            self.compare_contents();
        }
    }

    fn run_menu_command(&mut self, c: MenuCommand) {
        match c {
            MenuCommand::CompareContents => self.compare_contents(),
            MenuCommand::Copy(d) => self.copy(d),
            MenuCommand::Delete(s) => self.delete_side(s),
            MenuCommand::Rename => self.ask_rename(),
            MenuCommand::Shell(side, a) => self.open_side(side, a),
            MenuCommand::CopyPath(l, r) => self.copy_paths(l, r),
            MenuCommand::SelectStatus(s) => self.select_status(s),
            MenuCommand::Invert => self.state.invert_selection(),
            MenuCommand::ExpandSubtree => self.state.expand_subtree(),
            MenuCommand::CollapseSubtree => self.state.collapse_subtree(),
            MenuCommand::Rescan => self.rescan_selected(),
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let (hovered, dropped, pos) = ctx.input(|i| {
            (
                !i.raw.hovered_files.is_empty(),
                i.raw
                    .dropped_files
                    .first()
                    .map(|f| f.path().to_path_buf())
                    .filter(|p| !p.as_os_str().is_empty()),
                i.pointer.latest_pos(),
            )
        });
        let target = pos.and_then(|p| {
            let near = |r: Option<egui::Rect>| r.is_some_and(|r| r.expand(6.0).contains(p));
            if near(self.left_box) {
                Some(true)
            } else if near(self.right_box) {
                Some(false)
            } else {
                None
            }
        });
        self.drop_hover = if hovered { target } else { None };
        if let (Some(path), Some(left)) = (dropped, target) {
            self.drop_path(left, &path);
        }
    }

    fn open_log_folder(&mut self) {
        let dir = platform::log_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Err(e) = platform::open(&dir) {
            self.dialogs
                .push(crate::dialogs::Dialog::error("Open log folder", e));
        }
    }
}

/// A progress bar; indeterminate shows a moving segment.
fn progress_bar(ui: &mut Ui, value: f32, indeterminate: bool) -> egui::Response {
    if !indeterminate {
        return ui.add(
            egui::ProgressBar::new(value)
                .desired_width(160.0)
                .desired_height(12.0),
        );
    }
    let (rect, response) = ui.allocate_exact_size(vec2(160.0, 12.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
    painter.rect_stroke(
        rect,
        2.0,
        (1.0, theme::PANE_HEADER_BORDER),
        egui::StrokeKind::Inside,
    );
    let t = ui.input(|i| i.time) as f32;
    let phase = (t * 0.8).fract();
    let seg = rect.width() * 0.3;
    let x = rect.left() - seg + phase * (rect.width() + seg);
    let bar = egui::Rect::from_x_y_ranges(
        x.max(rect.left())..=(x + seg).min(rect.right()),
        rect.y_range(),
    );
    painter.rect_filled(bar, 2.0, ui.visuals().selection.bg_fill);
    ui.ctx().request_repaint();
    response
}

#[derive(Debug, Clone, Copy)]
enum MenuCommand {
    CompareContents,
    Copy(CopyDirection),
    Delete(Sides),
    Rename,
    Shell(Side, ShellAction),
    CopyPath(bool, bool),
    SelectStatus(DiffStatus),
    Invert,
    ExpandSubtree,
    CollapseSubtree,
    Rescan,
}

/// Enable states for the context menu, taken before the list borrows the state.
struct Enables {
    compare_contents: bool,
    copy_right: bool,
    copy_left: bool,
    delete_any: bool,
    rename: bool,
    left: bool,
    right: bool,
    subtree: bool,
    rescan: bool,
}

impl Enables {
    fn of(app: &FolderCompareApp) -> Self {
        let busy = app.is_busy();
        let s = &app.state;
        let first: Option<NodeId> = s.first_selected();
        Self {
            compare_contents: s.can_compare_contents(busy),
            copy_right: s.can_copy_to_right(busy),
            copy_left: s.can_copy_to_left(busy),
            delete_any: s.can_delete_any(busy),
            rename: s.can_rename(busy),
            left: first.is_some_and(|f| s.node(f).left.is_some()),
            right: first.is_some_and(|f| s.node(f).right.is_some()),
            subtree: s.can_expand_subtree(),
            rescan: s.can_refresh_selected(busy),
        }
    }
}

fn context_menu(ui: &mut Ui, e: &Enables, chosen: &mut Option<MenuCommand>) {
    let ctrl = platform::ctrl_name();
    let mut item = |ui: &mut Ui, enabled: bool, button: Button, c: MenuCommand| {
        if ui.add_enabled(enabled, button).clicked() {
            *chosen = Some(c);
        }
    };
    item(
        ui,
        e.compare_contents,
        Button::new(RichText::new("Compare contents...").strong()).shortcut_text("Enter"),
        MenuCommand::CompareContents,
    );
    ui.separator();
    item(
        ui,
        e.copy_right,
        Button::new(format!("Copy to Right {}", icons::ARROW_RIGHT))
            .shortcut_text(format!("{ctrl}+Right")),
        MenuCommand::Copy(CopyDirection::LeftToRight),
    );
    item(
        ui,
        e.copy_left,
        Button::new(format!("{} Copy to Left", icons::ARROW_LEFT))
            .shortcut_text(format!("{ctrl}+Left")),
        MenuCommand::Copy(CopyDirection::RightToLeft),
    );
    ui.separator();
    item(
        ui,
        e.copy_right,
        Button::new("Delete Left"),
        MenuCommand::Delete(Sides::Left),
    );
    item(
        ui,
        e.copy_left,
        Button::new("Delete Right"),
        MenuCommand::Delete(Sides::Right),
    );
    item(
        ui,
        e.delete_any,
        Button::new("Delete Both"),
        MenuCommand::Delete(Sides::Both),
    );
    ui.separator();
    item(
        ui,
        e.rename,
        Button::new("Rename...").shortcut_text("F2"),
        MenuCommand::Rename,
    );
    let file_manager = format!("Show in {}", platform::file_manager_name());
    for (title, action) in [
        ("Open", ShellAction::Open),
        ("Open containing folder", ShellAction::OpenFolder),
        (file_manager.as_str(), ShellAction::Reveal),
    ] {
        ui.menu_button(title, |ui| {
            if ui.add_enabled(e.left, Button::new("Left")).clicked() {
                *chosen = Some(MenuCommand::Shell(Side::Left, action));
            }
            if ui.add_enabled(e.right, Button::new("Right")).clicked() {
                *chosen = Some(MenuCommand::Shell(Side::Right, action));
            }
        });
    }
    ui.menu_button("Copy full path", |ui| {
        for (label, l, r) in [
            ("Left", true, false),
            ("Right", false, true),
            ("Both", true, true),
        ] {
            if ui.button(label).clicked() {
                *chosen = Some(MenuCommand::CopyPath(l, r));
            }
        }
    });
    ui.separator();
    for (label, status) in [
        ("Select all newer on left", DiffStatus::NewerLeft),
        ("Select all newer on right", DiffStatus::NewerRight),
        ("Select all only-left", DiffStatus::OnlyLeft),
        ("Select all only-right", DiffStatus::OnlyRight),
        (
            "Select all same time, different content",
            DiffStatus::SameTimeDifferentContent,
        ),
    ] {
        if ui.button(label).clicked() {
            *chosen = Some(MenuCommand::SelectStatus(status));
        }
    }
    if ui.button("Invert selection").clicked() {
        *chosen = Some(MenuCommand::Invert);
    }
    ui.separator();
    if ui
        .add_enabled(e.subtree, Button::new("Expand subtree"))
        .clicked()
    {
        *chosen = Some(MenuCommand::ExpandSubtree);
    }
    if ui
        .add_enabled(e.subtree, Button::new("Collapse subtree"))
        .clicked()
    {
        *chosen = Some(MenuCommand::CollapseSubtree);
    }
    if ui
        .add_enabled(e.rescan, Button::new("Rescan selected"))
        .clicked()
    {
        *chosen = Some(MenuCommand::Rescan);
    }
}
