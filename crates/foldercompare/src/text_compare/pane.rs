//! One editor pane of the text compare window: buffer, caret and selection, editing commands, and drawing.
//! Every visual row is exactly one line height (text line or filler), see [`super::layout`].

use std::path::PathBuf;
use std::time::SystemTime;

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, pos2, vec2};
use fc_core::textdiff::{DiffBlockKind, LoadedText, TextComparison};

use super::buffer::{Pos, TextBuffer};
use super::layout::{RowKind, RowMap};
use crate::theme;

pub const TAB_WIDTH: usize = 4;
/// Space between the line numbers and the text.
const GUTTER_PAD: f32 = 10.0;
const TEXT_PAD: f32 = 4.0;

/// Font metrics shared by both panes.
#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    pub font: f32,
    pub line_height: f32,
    pub char_width: f32,
}

impl Metrics {
    pub fn font_id(&self) -> FontId {
        theme::mono(self.font)
    }
}

pub struct Pane {
    pub is_left: bool,
    pub buffer: TextBuffer,
    pub path: PathBuf,
    pub exists: bool,
    /// Encoding, BOM and newline to write back (its text is empty).
    pub format: LoadedText,
    pub loaded_mtime: Option<SystemTime>,
    pub caret: Pos,
    /// Selection anchor; the selection is `anchor..caret`.
    pub anchor: Pos,
    /// Column kept while moving up and down.
    preferred_col: Option<usize>,
    pub rows: RowMap,
    rows_key: (u64, u64),
}

/// Display column of every char boundary of a line (tabs expand to the next tab stop).
pub fn display_cols(text: &str) -> Vec<usize> {
    let mut cols = Vec::with_capacity(text.len() + 1);
    let mut c = 0;
    cols.push(0);
    for ch in text.chars() {
        c = if ch == '\t' {
            (c / TAB_WIDTH + 1) * TAB_WIDTH
        } else {
            c + 1
        };
        cols.push(c);
    }
    cols
}

/// The line as drawn: tabs replaced by spaces up to the tab stop.
fn display_text(text: &str) -> String {
    let mut s = String::with_capacity(text.len());
    let mut c = 0;
    for ch in text.chars() {
        if ch == '\t' {
            let next = (c / TAB_WIDTH + 1) * TAB_WIDTH;
            s.extend(std::iter::repeat_n(' ', next - c));
            c = next;
        } else {
            s.push(ch);
            c += 1;
        }
    }
    s
}

impl Pane {
    pub fn new(
        is_left: bool,
        path: PathBuf,
        exists: bool,
        loaded: LoadedText,
        mtime: Option<SystemTime>,
    ) -> Self {
        let buffer = TextBuffer::new(&loaded.text);
        Self {
            is_left,
            buffer,
            path,
            exists,
            format: LoadedText {
                text: String::new(),
                ..loaded
            },
            loaded_mtime: mtime,
            caret: Pos::default(),
            anchor: Pos::default(),
            preferred_col: None,
            rows: RowMap::default(),
            rows_key: (u64::MAX, u64::MAX),
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.buffer.is_dirty()
    }

    pub fn name(&self) -> &'static str {
        if self.is_left { "Left" } else { "Right" }
    }

    /// Rebuilds the row map when the text or the comparison changed.
    pub fn update_rows(&mut self, comparison: Option<&TextComparison>, comparison_id: u64) {
        let key = (self.buffer.version, comparison_id);
        if key != self.rows_key {
            self.rows = RowMap::new(
                self.buffer.line_count(),
                comparison.map(|c| c.side(self.is_left)),
            );
            self.rows_key = key;
        }
    }

    // ---- Selection -----------------------------------------------------------------------------

    pub fn selection(&self) -> (Pos, Pos) {
        (self.anchor.min(self.caret), self.anchor.max(self.caret))
    }

    pub fn has_selection(&self) -> bool {
        self.anchor != self.caret
    }

    pub fn selected_text(&self) -> String {
        let (a, b) = self.selection();
        self.buffer.slice(a, b)
    }

    pub fn set_caret(&mut self, p: Pos, extend: bool) {
        self.caret = self.buffer.clamp(p);
        if !extend {
            self.anchor = self.caret;
        }
    }

    pub fn set_caret_line(&mut self, line: usize) {
        self.preferred_col = None;
        self.set_caret(Pos::new(line, 0), false);
    }

    pub fn select_all(&mut self) {
        self.anchor = Pos::default();
        self.caret = self.buffer.end();
    }

    // ---- Editing ---------------------------------------------------------------------------------

    /// Replaces the selection with `text`; typed text merges into one undo step.
    pub fn insert(&mut self, text: &str, typing: bool) {
        let (a, b) = self.selection();
        let before = (self.anchor, self.caret);
        let end = self.buffer.replace(a, b, text, before, typing);
        self.set_caret(end, false);
        self.preferred_col = None;
    }

    /// Pasted text gets the document's line endings.
    pub fn paste(&mut self, text: &str) {
        let text = fc_core::textdiff::normalize_newlines(text, self.format.newline);
        self.insert(&text, false);
    }

    pub fn newline(&mut self) {
        let nl = self.format.newline;
        self.insert(nl, false);
    }

    pub fn backspace(&mut self, word: bool) {
        if !self.has_selection() {
            let target = if word {
                self.buffer.word_left(self.caret)
            } else if self.caret.col > 0 {
                Pos::new(self.caret.line, self.caret.col - 1)
            } else if self.caret.line > 0 {
                Pos::new(
                    self.caret.line - 1,
                    self.buffer.line_len(self.caret.line - 1),
                )
            } else {
                return;
            };
            self.anchor = target;
        }
        self.insert("", false);
    }

    pub fn delete(&mut self, word: bool) {
        if !self.has_selection() {
            let len = self.buffer.line_len(self.caret.line);
            let target = if word {
                self.buffer.word_right(self.caret)
            } else if self.caret.col < len {
                Pos::new(self.caret.line, self.caret.col + 1)
            } else if self.caret.line + 1 < self.buffer.line_count() {
                Pos::new(self.caret.line + 1, 0)
            } else {
                return;
            };
            self.anchor = target;
        }
        self.insert("", false);
    }

    /// Removes the selection and returns it (Cut).
    pub fn cut(&mut self) -> Option<String> {
        if !self.has_selection() {
            return None;
        }
        let text = self.selected_text();
        self.insert("", false);
        Some(text)
    }

    pub fn undo(&mut self) {
        if let Some((a, c)) = self.buffer.undo() {
            self.anchor = self.buffer.clamp(a);
            self.caret = self.buffer.clamp(c);
        }
    }

    pub fn redo(&mut self) {
        if let Some((a, c)) = self.buffer.redo() {
            self.anchor = self.buffer.clamp(a);
            self.caret = self.buffer.clamp(c);
        }
    }

    // ---- Caret movement --------------------------------------------------------------------------

    pub fn move_horizontal(&mut self, right: bool, word: bool, extend: bool) {
        self.preferred_col = None;
        if !extend && self.has_selection() && !word {
            let (a, b) = self.selection();
            self.set_caret(if right { b } else { a }, false);
            return;
        }
        let p = self.caret;
        let target = match (right, word) {
            (true, true) => self.buffer.word_right(p),
            (false, true) => self.buffer.word_left(p),
            (true, false) if p.col < self.buffer.line_len(p.line) => Pos::new(p.line, p.col + 1),
            (true, false) if p.line + 1 < self.buffer.line_count() => Pos::new(p.line + 1, 0),
            (false, false) if p.col > 0 => Pos::new(p.line, p.col - 1),
            (false, false) if p.line > 0 => Pos::new(p.line - 1, self.buffer.line_len(p.line - 1)),
            _ => p,
        };
        self.set_caret(target, extend);
    }

    /// Moves by `lines` (negative = up), keeping the display column.
    pub fn move_vertical(&mut self, lines: isize, extend: bool) {
        let p = self.caret;
        let col = *self
            .preferred_col
            .get_or_insert_with(|| display_cols(self.buffer.line(p.line))[p.col]);
        let line =
            (p.line as isize + lines).clamp(0, self.buffer.line_count() as isize - 1) as usize;
        let cols = display_cols(self.buffer.line(line));
        let c = cols
            .iter()
            .position(|&x| x >= col)
            .unwrap_or(cols.len() - 1);
        self.caret = Pos::new(line, c);
        if !extend {
            self.anchor = self.caret;
        }
    }

    pub fn home(&mut self, document: bool, extend: bool) {
        self.preferred_col = None;
        let p = if document {
            Pos::default()
        } else {
            // Smart home: first non-blank char, then column 0.
            let first = self
                .buffer
                .line(self.caret.line)
                .chars()
                .position(|c| !c.is_whitespace())
                .unwrap_or(0);
            Pos::new(
                self.caret.line,
                if self.caret.col == first { 0 } else { first },
            )
        };
        self.set_caret(p, extend);
    }

    pub fn end(&mut self, document: bool, extend: bool) {
        self.preferred_col = None;
        let p = if document {
            self.buffer.end()
        } else {
            Pos::new(self.caret.line, self.buffer.line_len(self.caret.line))
        };
        self.set_caret(p, extend);
    }

    /// Selects the word at a position (double click).
    pub fn select_word(&mut self, p: Pos) {
        let p = self.buffer.clamp(p);
        let chars: Vec<char> = self.buffer.line(p.line).chars().collect();
        if chars.is_empty() {
            self.set_caret(p, false);
            return;
        }
        let i = p.col.min(chars.len() - 1);
        let kind = |c: char| (c.is_alphanumeric() || c == '_', c.is_whitespace());
        let k = kind(chars[i]);
        let mut a = i;
        while a > 0 && kind(chars[a - 1]) == k {
            a -= 1;
        }
        let mut b = i + 1;
        while b < chars.len() && kind(chars[b]) == k {
            b += 1;
        }
        self.anchor = Pos::new(p.line, a);
        self.caret = Pos::new(p.line, b);
    }

    // ---- Geometry ----------------------------------------------------------------------------------

    pub fn gutter_width(&self, m: &Metrics) -> f32 {
        let digits = self.buffer.line_count().max(1).to_string().len().max(3);
        digits as f32 * m.char_width + GUTTER_PAD
    }

    /// x of the text start (column 0) within the pane, before horizontal scrolling.
    pub fn text_left(&self, rect: Rect, m: &Metrics) -> f32 {
        rect.left() + self.gutter_width(m) + TEXT_PAD
    }

    /// The buffer position under a point.
    pub fn pos_at(&self, rect: Rect, m: &Metrics, v_offset: f32, h_offset: f32, p: Pos2) -> Pos {
        let row = ((p.y - rect.top() + v_offset) / m.line_height)
            .floor()
            .max(0.0) as usize;
        let line = match self.rows.kind(row) {
            Some(RowKind::Line(l)) => l,
            _ => self
                .rows
                .line_near(row.min(self.rows.total_rows().saturating_sub(1))),
        };
        let line = line.min(self.buffer.line_count() - 1);
        let x = p.x - self.text_left(rect, m) + h_offset;
        let col_display = (x / m.char_width).round().max(0.0) as usize;
        let cols = display_cols(self.buffer.line(line));
        let col = cols
            .iter()
            .position(|&c| c >= col_display)
            .unwrap_or(cols.len() - 1);
        Pos::new(line, col)
    }

    /// Screen rect of the caret, for scrolling it into view.
    pub fn caret_x(&self, m: &Metrics) -> f32 {
        display_cols(self.buffer.line(self.caret.line))[self.caret.col] as f32 * m.char_width
    }

    // ---- Drawing -----------------------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        painter: &Painter,
        rect: Rect,
        m: &Metrics,
        v_offset: f32,
        h_offset: f32,
        comparison: Option<&TextComparison>,
        current_block: Option<usize>,
        show_whitespace: bool,
        focused: bool,
    ) {
        let lh = m.line_height;
        painter.rect_filled(rect, 0.0, theme::EDITOR_BACKGROUND);
        let gutter = self.gutter_width(m);
        let text_rect = Rect::from_min_max(pos2(rect.left() + gutter, rect.top()), rect.max);
        let text_painter = painter.with_clip_rect(text_rect.intersect(painter.clip_rect()));
        let x0 = self.text_left(rect, m) - h_offset;
        let side = comparison.map(|c| c.side(self.is_left));
        let (sel_a, sel_b) = self.selection();
        let font = m.font_id();

        let first_row = (v_offset / lh).floor().max(0.0) as usize;
        let visible = (rect.height() / lh).ceil() as usize + 1;
        for row in first_row..first_row + visible {
            let Some(kind) = self.rows.kind(row) else {
                break;
            };
            let top = rect.top() + row as f32 * lh - v_offset;
            let row_rect =
                Rect::from_min_size(pos2(text_rect.left(), top), vec2(text_rect.width(), lh));
            let line = match kind {
                RowKind::Filler => {
                    draw_filler(&text_painter, row_rect);
                    continue;
                }
                RowKind::Line(l) => l,
            };
            let text = self.buffer.line(line);
            let cols = display_cols(text);
            let kind = side
                .filter(|s| line < s.line_count())
                .map_or(DiffBlockKind::Equal, |s| s.kinds[line]);
            if kind != DiffBlockKind::Equal {
                let important = kind != DiffBlockKind::Unimportant;
                text_painter.rect_filled(
                    row_rect,
                    0.0,
                    if important {
                        theme::CHANGED_LINE
                    } else {
                        theme::UNIMPORTANT_LINE
                    },
                );
                if let Some(ranges) = side.and_then(|s| s.inline.get(&line)) {
                    let color = if important {
                        theme::CHANGED_TEXT
                    } else {
                        theme::UNIMPORTANT_TEXT
                    };
                    for r in ranges {
                        // The comparison may lag behind typing for a moment; clamp to the current line.
                        let a = cols[r.start.min(cols.len() - 1)];
                        let b = cols[r.end().min(cols.len() - 1)];
                        if b > a {
                            text_painter.rect_filled(
                                Rect::from_x_y_ranges(
                                    x0 + a as f32 * m.char_width..=x0 + b as f32 * m.char_width,
                                    top..=top + lh,
                                ),
                                0.0,
                                color,
                            );
                        }
                    }
                }
            }

            // Selection.
            if sel_a != sel_b && line >= sel_a.line && line <= sel_b.line {
                let a = if line == sel_a.line {
                    cols[sel_a.col]
                } else {
                    0
                };
                let (b, eol) = if line == sel_b.line {
                    (cols[sel_b.col], 0.0)
                } else {
                    (*cols.last().unwrap_or(&0), m.char_width * 0.6)
                };
                text_painter.rect_filled(
                    Rect::from_x_y_ranges(
                        x0 + a as f32 * m.char_width..=x0 + b as f32 * m.char_width + eol,
                        top..=top + lh,
                    ),
                    0.0,
                    theme::EDITOR_SELECTION,
                );
            }

            // Text: only the visible columns are laid out (long lines stay cheap).
            let first_col = (h_offset / m.char_width).floor().max(0.0) as usize;
            let last_col = first_col + (text_rect.width() / m.char_width).ceil() as usize + 2;
            let display = display_text(text);
            let slice: String = display
                .chars()
                .skip(first_col)
                .take(last_col - first_col)
                .collect();
            if !slice.is_empty() {
                text_painter.text(
                    pos2(x0 + first_col as f32 * m.char_width, top + lh / 2.0),
                    Align2::LEFT_CENTER,
                    slice,
                    font.clone(),
                    theme::TEXT,
                );
            }
            if show_whitespace {
                for (i, ch) in text.chars().enumerate() {
                    let mark = match ch {
                        ' ' => "·",
                        '\t' => "→",
                        _ => continue,
                    };
                    let c = cols[i];
                    if c + 1 < first_col || c > last_col {
                        continue;
                    }
                    text_painter.text(
                        pos2(
                            x0 + c as f32 * m.char_width + m.char_width / 2.0,
                            top + lh / 2.0,
                        ),
                        Align2::CENTER_CENTER,
                        mark,
                        font.clone(),
                        theme::WHITESPACE_MARK,
                    );
                }
            }

            // Line number.
            painter.with_clip_rect(rect).text(
                pos2(rect.left() + gutter - GUTTER_PAD / 2.0, top + lh / 2.0),
                Align2::RIGHT_CENTER,
                (line + 1).to_string(),
                font.clone(),
                theme::LINE_NUMBER,
            );

            if focused && line == self.caret.line {
                let x = x0 + cols[self.caret.col] as f32 * m.char_width;
                text_painter.line_segment(
                    [pos2(x, top + 1.0), pos2(x, top + lh - 1.0)],
                    Stroke::new(1.5, theme::CARET),
                );
            }
        }

        // The current section outline.
        if let (Some(c), Some(block)) = (comparison, current_block)
            && block < c.blocks.len()
        {
            let b = &c.blocks[block];
            let y = rect.top() + c.row_of(block) as f32 * lh - v_offset;
            let h = (b.height() as f32 * lh - 1.5).max(2.0);
            let outline = Rect::from_min_size(
                pos2(text_rect.left() + 0.75, y + 0.75),
                vec2(text_rect.width() - 1.5, h),
            );
            painter.with_clip_rect(text_rect).rect_stroke(
                outline,
                0.0,
                Stroke::new(1.5, theme::CURRENT_SECTION),
                egui::StrokeKind::Inside,
            );
        }
    }
}

/// Filler background with a 45° hatch on an 8 px tile.
pub fn draw_filler(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 0.0, theme::FILLER);
    let p = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let stroke = Stroke::new(1.0, theme::FILLER_HATCH);
    let tile = 8.0;
    // Lines x + y = k * tile, anchored to the pane origin so the pattern doesn't move with the row.
    let start = ((rect.left() + rect.top()) / tile).floor() * tile;
    let mut k = start;
    while k < rect.right() + rect.bottom() + tile {
        let a = pos2(k - rect.bottom(), rect.bottom());
        let b = pos2(k - rect.top(), rect.top());
        p.line_segment([a, b], stroke);
        k += tile;
    }
}

/// A band color for a block in the overview and the center strip.
pub fn band_color(kind: DiffBlockKind) -> Color32 {
    if kind == DiffBlockKind::Unimportant {
        theme::UNIMPORTANT_TEXT
    } else {
        theme::CHANGED_TEXT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_expand_to_tab_stops() {
        assert_eq!(vec![0, 4, 5, 8, 9], display_cols("\ta\tb"));
        assert_eq!("    a   b", display_text("\ta\tb"));
    }

    #[test]
    fn editing_commands() {
        let mut p = Pane::new(
            true,
            PathBuf::new(),
            true,
            fc_core::textdiff::codec::decode(b"ab\r\ncd"),
            None,
        );
        p.set_caret(Pos::new(0, 2), false);
        p.newline();
        assert_eq!("ab\r\n\r\ncd", p.buffer.text());
        p.backspace(false);
        assert_eq!("ab\r\ncd", p.buffer.text());
        p.paste("x\ny");
        assert_eq!("abx\r\ny\r\ncd", p.buffer.text());
        p.undo();
        assert_eq!("ab\r\ncd", p.buffer.text());
        p.select_all();
        assert_eq!(Some("ab\r\ncd".to_string()), p.cut());
        assert_eq!("", p.buffer.text());
        p.undo();
        p.set_caret(Pos::new(1, 0), false);
        p.move_vertical(-1, true);
        assert_eq!("ab\r\n", p.selected_text());
    }
}
