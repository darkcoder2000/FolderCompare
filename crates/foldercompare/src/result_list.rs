//! The side-by-side result list: a header with sortable, partly resizable columns and virtualized rows. Drawn
//! by hand (not egui_extras' table) so the columns can be fitted around the swap button and the expander
//! triangle can live inside a clickable row.

use egui::{Align2, Color32, CursorIcon, FontId, Rect, Response, Sense, Ui, Vec2, pos2, vec2};
use fc_core::model::{DiffNode, DiffStatus, NodeId};

use crate::state::{
    MainState, Row, SideState, SortKey, format_date, format_size, row_tooltip, side_state,
};
use crate::theme::{self, icons};

pub const ROW_HEIGHT: f32 = 20.0;
pub const HEADER_HEIGHT: f32 = 22.0;
const INDENT: f32 = 16.0;
const EXPANDER: f32 = 16.0;
const ICON: f32 = 20.0;
const PAD: f32 = 4.0;
pub const MIN_NAME_WIDTH: f32 = 120.0;
const MIN_COLUMN_WIDTH: f32 = 30.0;
/// Keeps the rows a few pixels narrower than the viewport so nothing is cut off at the right edge.
const RIGHT_RESERVE: f32 = 6.0;
const SCROLLBAR: f32 = 12.0;

const HEADERS: [(&str, SortKey); 7] = [
    ("Left name", SortKey::Name),
    ("Size", SortKey::LeftSize),
    ("Modified", SortKey::LeftDate),
    ("", SortKey::Status),
    ("Right name", SortKey::Name),
    ("Size", SortKey::RightSize),
    ("Modified", SortKey::RightDate),
];
/// Size and date columns are user-resizable; the name columns absorb the change, the status column is fixed.
const RESIZABLE: [bool; 7] = [false, true, true, false, false, true, true];

/// What the user did with the list this frame.
#[derive(Default)]
pub struct ListEvents {
    pub sort: Option<SortKey>,
    pub double_clicked: Option<NodeId>,
    /// The context menu is open for the selection; the closure draws its contents.
    pub context_menu: bool,
    pub columns_changed: bool,
}

/// Persistent list state (scroll handling).
#[derive(Default)]
pub struct ListView {
    /// Scroll so that this row becomes visible on the next frame.
    pub scroll_to: Option<NodeId>,
    last_offset: f32,
    last_height: f32,
}

/// Name column widths so that the status column sits centered under `swap_center` (screen x).
pub fn fit_columns(
    columns: &mut [f32; 7],
    table_left: f32,
    table_width: f32,
    swap_center: Option<f32>,
) {
    let status = columns[3];
    let left_fixed = columns[1] + columns[2];
    let right_fixed = columns[5] + columns[6];
    let available = table_width - SCROLLBAR - RIGHT_RESERVE;
    let (mut left, mut right) = match swap_center {
        Some(x) => {
            let center = x - table_left;
            (
                center - status / 2.0 - left_fixed,
                available - (center + status / 2.0) - right_fixed,
            )
        }
        None => (0.0, 0.0),
    };
    if left < MIN_NAME_WIDTH || right < MIN_NAME_WIDTH {
        // Too narrow to align with the button: split what is left evenly.
        let rest = available - status - left_fixed - right_fixed;
        left = (rest / 2.0).max(MIN_NAME_WIDTH);
        right = left;
    }
    columns[0] = left.floor();
    columns[4] = right.floor();
}

impl ListView {
    /// Draws the list. `context_menu` draws the context menu's contents.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        state: &mut MainState,
        columns: &mut [f32; 7],
        context_menu: &mut dyn FnMut(&mut Ui),
    ) -> ListEvents {
        let mut events = ListEvents::default();
        let origin = ui.max_rect().min;
        let width = ui.available_width();

        // ---- Header ----
        let (header_rect, _) = ui.allocate_exact_size(vec2(width, HEADER_HEIGHT), Sense::hover());
        let painter = ui.painter_at(header_rect);
        painter.rect_filled(header_rect, 0.0, theme::STATUS_COLUMN);
        painter.hline(
            header_rect.x_range(),
            header_rect.bottom() - 0.5,
            (1.0, theme::PANE_HEADER_BORDER),
        );
        let mut x = origin.x;
        for (i, (title, key)) in HEADERS.iter().enumerate() {
            let rect =
                Rect::from_min_size(pos2(x, header_rect.top()), vec2(columns[i], HEADER_HEIGHT));
            let response = ui.interact(rect, ui.id().with(("header", i)), Sense::click());
            if response.hovered() {
                painter.rect_filled(rect, 0.0, theme::ROW_HOVER);
            }
            if response.clicked() {
                events.sort = Some(*key);
            }
            if i == 3 {
                response.on_hover_text("Status");
            }
            let mut label = title.to_string();
            // The name sort shows its arrow on the left name header only.
            if state.sort_key == Some(*key) && (*key != SortKey::Name || i == 0) {
                if !label.is_empty() {
                    label.push(' ');
                }
                label.push_str(if state.sort_ascending {
                    icons::CARET_UP
                } else {
                    icons::CARET_DOWN
                });
            }
            painter.with_clip_rect(rect).text(
                pos2(rect.left() + PAD, rect.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(13.0),
                theme::TEXT,
            );
            painter.vline(
                rect.right() - 0.5,
                rect.y_range(),
                (1.0, theme::PANE_HEADER_BORDER),
            );

            if RESIZABLE[i] {
                let grip = Rect::from_center_size(
                    pos2(rect.right(), rect.center().y),
                    vec2(8.0, HEADER_HEIGHT),
                );
                let g = ui.interact(grip, ui.id().with(("grip", i)), Sense::drag());
                if g.hovered() || g.dragged() {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                }
                if g.dragged() {
                    columns[i] = (columns[i] + g.drag_delta().x).max(MIN_COLUMN_WIDTH);
                    events.columns_changed = true;
                }
            }
            x += columns[i];
        }

        // ---- Rows ----
        let total_rows = state.rows.len();
        let mut area = egui::ScrollArea::vertical()
            .id_salt("results")
            .auto_shrink(false)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible);
        if let Some(target) = self.scroll_to.take()
            && let Some(index) = state.row_of(target)
        {
            let top = index as f32 * ROW_HEIGHT;
            let height = self.last_height.max(ROW_HEIGHT);
            if top < self.last_offset {
                area = area.vertical_scroll_offset(top);
            } else if top + ROW_HEIGHT > self.last_offset + height {
                area = area.vertical_scroll_offset(top + ROW_HEIGHT - height);
            }
        }
        let output = area.show_rows(ui, ROW_HEIGHT, total_rows, |ui, range| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            for index in range {
                let row = state.rows[index];
                let (rect, response) =
                    ui.allocate_exact_size(vec2(width - SCROLLBAR, ROW_HEIGHT), Sense::click());
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let response = self.row(ui, state, row, rect, response, columns, &mut events);
                if response.secondary_clicked() {
                    state.context_click(row.node);
                }
                response.context_menu(|ui| {
                    events.context_menu = true;
                    context_menu(ui);
                });
            }
        });
        self.last_offset = output.state.offset.y;
        self.last_height = output.inner_rect.height();
        events
    }

    #[allow(clippy::too_many_arguments)]
    fn row(
        &self,
        ui: &mut Ui,
        state: &mut MainState,
        row: Row,
        rect: Rect,
        response: Response,
        columns: &[f32; 7],
        events: &mut ListEvents,
    ) -> Response {
        let node = state.node(row.node).clone();
        let selected = state.is_selected(row.node);
        let focused = state.focus == Some(row.node);
        let expandable = state.is_expandable(row.node);
        let expanded = state.is_expanded(row.node);
        let painter = ui.painter_at(rect);

        if selected {
            painter.rect_filled(rect, 0.0, theme::SELECTION);
        } else if response.hovered() {
            painter.rect_filled(rect, 0.0, theme::ROW_HOVER);
        }
        if focused {
            painter.rect_stroke(
                rect.shrink(0.5),
                0.0,
                (1.0, theme::SELECTION_FOCUS),
                egui::StrokeKind::Inside,
            );
        }

        let mut x = [0.0f32; 8];
        x[0] = rect.left();
        for i in 0..7 {
            x[i + 1] = x[i] + columns[i];
        }
        let cell = |i: usize| Rect::from_x_y_ranges(x[i]..=x[i + 1], rect.y_range());

        // Status column background and glyph.
        let status_rect = cell(3);
        if !selected {
            painter.rect_filled(status_rect, 0.0, theme::STATUS_COLUMN);
        }
        painter.text(
            status_rect.center(),
            Align2::CENTER_CENTER,
            status_icon(node.status),
            FontId::new(14.0, theme::bold_family()),
            theme::GLYPH,
        );

        // Name cells.
        let indent = row.level as f32 * INDENT;
        let expander = Rect::from_min_size(
            pos2(x[0] + PAD + indent, rect.top()),
            vec2(EXPANDER, ROW_HEIGHT),
        );
        let hover_expander =
            expandable && response.hover_pos().is_some_and(|p| expander.contains(p));
        let click_on_expander = expandable
            && response
                .interact_pointer_pos()
                .is_some_and(|p| expander.contains(p));
        if expandable {
            painter.text(
                expander.center(),
                Align2::CENTER_CENTER,
                if expanded {
                    icons::CARET_DOWN
                } else {
                    icons::CARET_RIGHT
                },
                FontId::proportional(12.0),
                if hover_expander {
                    theme::EXPANDER_HOVER
                } else {
                    theme::EXPANDER
                },
            );
        }
        for (left, col) in [(true, 0usize), (false, 4usize)] {
            let state_of_side = side_state(&node, left);
            if state_of_side == SideState::Missing {
                continue;
            }
            let start = x[col] + PAD + indent + EXPANDER;
            let clip = painter.with_clip_rect(cell(col).intersect(rect));
            let icon_center = pos2(start + ICON / 2.0, rect.center().y);
            let (icon, color) = side_icon(&node, left);
            clip.text(
                icon_center,
                Align2::CENTER_CENTER,
                icon,
                FontId::proportional(15.0),
                color,
            );
            if node.is_dir && node.status == DiffStatus::Differs {
                clip.circle_filled(
                    pos2(start + ICON - 3.0, rect.bottom() - 5.0),
                    3.0,
                    theme::DIFF_MARKER,
                );
            }
            let (color, bold) = side_style(state_of_side);
            clip.text(
                pos2(start + ICON + PAD, rect.center().y),
                Align2::LEFT_CENTER,
                &node.name,
                font(bold),
                color,
            );
        }

        // Size and date cells.
        for (left, size_col, date_col) in [(true, 1usize, 2usize), (false, 5, 6)] {
            let s = side_state(&node, left);
            let Some(snap) = (if left { &node.left } else { &node.right }) else {
                continue;
            };
            let (color, bold) = side_style(s);
            let total = if left {
                node.left_total
            } else {
                node.right_total
            };
            let size_rect = cell(size_col);
            painter.with_clip_rect(size_rect).text(
                pos2(size_rect.right() - PAD, rect.center().y),
                Align2::RIGHT_CENTER,
                format_size(total),
                font(bold),
                color,
            );
            let date_rect = cell(date_col);
            painter.with_clip_rect(date_rect).text(
                pos2(date_rect.left() + PAD, rect.center().y),
                Align2::LEFT_CENTER,
                format_date(snap.modified),
                font(bold),
                color,
            );
        }

        let modifiers = ui.input(|i| i.modifiers);
        if response.clicked() {
            if click_on_expander {
                state.toggle_expanded(row.node);
            } else {
                state.click(row.node, modifiers.command, modifiers.shift);
            }
        }
        if response.double_clicked() && !click_on_expander {
            events.double_clicked = Some(row.node);
        }
        match row_tooltip(&node) {
            Some(tip) => response.on_hover_text(tip.to_string()),
            None => response,
        }
    }
}

fn font(bold: bool) -> FontId {
    if bold {
        FontId::new(13.0, theme::bold_family())
    } else {
        FontId::proportional(13.0)
    }
}

fn side_style(s: SideState) -> (Color32, bool) {
    match s {
        SideState::Missing | SideState::Normal => (theme::TEXT, false),
        SideState::Newer => (theme::NEWER, true),
        SideState::Older => (theme::OLDER, false),
        SideState::Only => (theme::ONLY, false),
        SideState::Error => (theme::ERROR, false),
        SideState::Conflict => (theme::CONFLICT, true),
    }
}

fn side_icon(node: &DiffNode, left: bool) -> (&'static str, Color32) {
    if matches!(node.status, DiffStatus::Error | DiffStatus::TypeConflict) {
        return (icons::WARNING, theme::ERROR);
    }
    let snap = if left { &node.left } else { &node.right };
    if snap.as_ref().is_some_and(|s| s.is_dir()) {
        (icons::FOLDER, theme::FOLDER_ICON)
    } else {
        (icons::FILE, theme::FILE_ICON)
    }
}

/// The status glyph as an icon: → ← = ≠ ⚠.
pub fn status_icon(status: DiffStatus) -> &'static str {
    match status {
        DiffStatus::NewerLeft | DiffStatus::OnlyLeft => icons::ARROW_RIGHT,
        DiffStatus::NewerRight | DiffStatus::OnlyRight => icons::ARROW_LEFT,
        DiffStatus::Identical | DiffStatus::SameContentDifferentTime => icons::EQUALS,
        DiffStatus::Differs => icons::NOT_EQUALS,
        _ => icons::WARNING,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_column_is_centered_under_the_swap_button() {
        let mut c = crate::app::DEFAULT_COLUMNS;
        fit_columns(&mut c, 0.0, 1200.0, Some(600.0));
        let status_center = c[0] + c[1] + c[2] + c[3] / 2.0;
        assert!((status_center - 600.0).abs() <= 1.0, "{c:?}");
        let total: f32 = c.iter().sum();
        assert!(total <= 1200.0 - SCROLLBAR - RIGHT_RESERVE + 1.0);
    }

    #[test]
    fn narrow_windows_split_evenly_with_a_minimum() {
        let mut c = crate::app::DEFAULT_COLUMNS;
        fit_columns(&mut c, 0.0, 600.0, Some(100.0));
        assert_eq!(c[0], c[4]);
        assert!(c[0] >= MIN_NAME_WIDTH);
    }
}
