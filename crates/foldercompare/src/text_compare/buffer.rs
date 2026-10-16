//! The text of one pane: lines that keep their own line endings (so mixed endings save back unchanged), char
//! positions, and an undo stack with groups (a block copy is one undo step). Free of egui, unit-tested.

use fc_core::textdiff::LineSource;

/// A position in the buffer: 0-based line and char column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

impl Pos {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    text: String,
    /// "", "\n", "\r\n" or "\r"; only the last line has "".
    end: &'static str,
    /// Number of chars in `text`.
    chars: usize,
}

impl Line {
    fn new(text: String, end: &'static str) -> Self {
        let chars = text.chars().count();
        Self { text, end, chars }
    }
}

/// One replace operation, with what is needed to undo it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Change {
    start: Pos,
    removed: String,
    inserted: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Group {
    changes: Vec<Change>,
    caret_before: (Pos, Pos),
    caret_after: (Pos, Pos),
    /// Typing that may be merged with the next keystroke.
    mergeable: bool,
}

#[derive(Debug, Clone)]
pub struct TextBuffer {
    lines: Vec<Line>,
    undo: Vec<Group>,
    redo: Vec<Group>,
    /// Number of undo groups at the last save; `None` when that state can't be reached any more.
    save_point: Option<usize>,
    /// Incremented on every change (for stale comparisons).
    pub version: u64,
    open_group: Option<Group>,
}

fn byte_index(s: &str, col: usize) -> usize {
    s.char_indices().nth(col).map_or(s.len(), |(i, _)| i)
}

/// Splits text into (line text, ending) pieces; there is always at least one piece.
fn split_with_endings(text: &str) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let (mut start, mut n) = (0, 0);
    while n < bytes.len() {
        match bytes[n] {
            b'\r' if bytes.get(n + 1) == Some(&b'\n') => {
                out.push((text[start..n].to_string(), "\r\n"));
                n += 1;
                start = n + 1;
            }
            b'\r' => {
                out.push((text[start..n].to_string(), "\r"));
                start = n + 1;
            }
            b'\n' => {
                out.push((text[start..n].to_string(), "\n"));
                start = n + 1;
            }
            _ => {}
        }
        n += 1;
    }
    out.push((text[start..].to_string(), ""));
    out
}

impl TextBuffer {
    pub fn new(text: &str) -> Self {
        let lines = split_with_endings(text)
            .into_iter()
            .map(|(t, e)| Line::new(t, e))
            .collect();
        Self {
            lines,
            undo: Vec::new(),
            redo: Vec::new(),
            save_point: Some(0),
            version: 0,
            open_group: None,
        }
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line(&self, line: usize) -> &str {
        &self.lines[line].text
    }

    pub fn line_len(&self, line: usize) -> usize {
        self.lines[line].chars
    }

    /// Line texts without their endings (input of the comparison).
    pub fn lines_text(&self) -> Vec<String> {
        self.lines.iter().map(|l| l.text.clone()).collect()
    }

    pub fn text(&self) -> String {
        let mut s = String::with_capacity(self.lines.iter().map(|l| l.text.len() + 2).sum());
        for l in &self.lines {
            s.push_str(&l.text);
            s.push_str(l.end);
        }
        s
    }

    pub fn clamp(&self, p: Pos) -> Pos {
        let line = p.line.min(self.lines.len() - 1);
        Pos::new(line, p.col.min(self.lines[line].chars))
    }

    pub fn end(&self) -> Pos {
        let last = self.lines.len() - 1;
        Pos::new(last, self.lines[last].chars)
    }

    /// The text between two positions (endings included).
    pub fn slice(&self, a: Pos, b: Pos) -> String {
        let (a, b) = (self.clamp(a.min(b)), self.clamp(a.max(b)));
        if a.line == b.line {
            let t = &self.lines[a.line].text;
            return t[byte_index(t, a.col)..byte_index(t, b.col)].to_string();
        }
        let mut s = String::new();
        let first = &self.lines[a.line];
        s.push_str(&first.text[byte_index(&first.text, a.col)..]);
        s.push_str(first.end);
        for l in &self.lines[a.line + 1..b.line] {
            s.push_str(&l.text);
            s.push_str(l.end);
        }
        let last = &self.lines[b.line].text;
        s.push_str(&last[..byte_index(last, b.col)]);
        s
    }

    /// Position `chars` characters after `p` (a line ending counts with its length, like offsets).
    pub fn advance(&self, p: Pos, chars: usize) -> Pos {
        let p = self.clamp(p);
        let line = &self.lines[p.line];
        if p.col + chars <= line.chars {
            return Pos::new(p.line, p.col + chars);
        }
        self.pos_of(self.offset_of(p) + chars)
    }

    /// The char offset of a position (each line ending counts as its own length in chars).
    pub fn offset_of(&self, p: Pos) -> usize {
        let p = self.clamp(p);
        self.lines[..p.line]
            .iter()
            .map(|l| l.chars + l.end.len())
            .sum::<usize>()
            + p.col
    }

    /// The position of a char offset.
    pub fn pos_of(&self, mut offset: usize) -> Pos {
        for (i, l) in self.lines.iter().enumerate() {
            let len = l.chars + l.end.len();
            if offset <= l.chars || i + 1 == self.lines.len() {
                return Pos::new(i, offset.min(l.chars));
            }
            if offset < len {
                return Pos::new(i, l.chars);
            }
            offset -= len;
        }
        self.end()
    }

    /// Replaces `a..b` with `text` (raw, endings kept as given) without recording undo. Returns the end of the
    /// inserted text.
    fn apply(&mut self, a: Pos, b: Pos, text: &str) -> Pos {
        let (a, b) = (self.clamp(a.min(b)), self.clamp(a.max(b)));
        let head = {
            let t = &self.lines[a.line].text;
            t[..byte_index(t, a.col)].to_string()
        };
        let (tail, tail_end) = {
            let l = &self.lines[b.line];
            (l.text[byte_index(&l.text, b.col)..].to_string(), l.end)
        };
        let mut pieces = split_with_endings(text);
        let n = pieces.len();
        // Measured before the head and tail are added.
        let end_col = pieces[n - 1].0.chars().count();
        pieces[0].0.insert_str(0, &head);
        pieces[n - 1].0.push_str(&tail);
        pieces[n - 1].1 = tail_end;
        let new_lines: Vec<Line> = pieces.into_iter().map(|(t, e)| Line::new(t, e)).collect();
        self.lines.splice(a.line..=b.line, new_lines);
        self.version += 1;
        if n == 1 {
            Pos::new(a.line, a.col + end_col)
        } else {
            Pos::new(a.line + n - 1, end_col)
        }
    }

    /// Starts an undo group: all changes until [`Self::end_group`] are undone together.
    pub fn begin_group(&mut self, caret: (Pos, Pos)) {
        self.close_group();
        self.open_group = Some(Group {
            changes: Vec::new(),
            caret_before: caret,
            caret_after: caret,
            mergeable: false,
        });
    }

    pub fn end_group(&mut self, caret: (Pos, Pos)) {
        if let Some(mut g) = self.open_group.take() {
            g.caret_after = caret;
            if !g.changes.is_empty() {
                self.push_group(g);
            }
        }
    }

    fn close_group(&mut self) {
        if let Some(g) = self.open_group.take()
            && !g.changes.is_empty()
        {
            self.push_group(g);
        }
    }

    fn push_group(&mut self, g: Group) {
        if self.save_point.is_some_and(|s| s > self.undo.len()) {
            self.save_point = None; // the saved state was in the redo branch
        }
        self.undo.push(g);
        self.redo.clear();
    }

    /// Replaces `a..b` with `text` as one undo step (or as part of an open group). `typing` lets consecutive
    /// single-char inserts merge into one step. Returns the end of the inserted text.
    pub fn replace(
        &mut self,
        a: Pos,
        b: Pos,
        text: &str,
        caret_before: (Pos, Pos),
        typing: bool,
    ) -> Pos {
        let (a, b) = (self.clamp(a.min(b)), self.clamp(a.max(b)));
        let removed = self.slice(a, b);
        let end = self.apply(a, b, text);
        let change = Change {
            start: a,
            removed,
            inserted: text.to_string(),
        };
        if let Some(g) = &mut self.open_group {
            g.changes.push(change);
            g.caret_after = (end, end);
            return end;
        }
        let merge = typing
            && !text.contains(['\n', '\r', ' ', '\t'])
            && change.removed.is_empty()
            && self.save_point != Some(self.undo.len())
            && self.redo.is_empty()
            && self.undo.last().is_some_and(|g| {
                g.mergeable
                    && g.caret_after.0 == a
                    && g.changes.len() == 1
                    && g.changes[0].removed.is_empty()
            });
        if merge {
            let g = self.undo.last_mut().expect("checked");
            g.changes[0].inserted.push_str(text);
            g.caret_after = (end, end);
        } else {
            self.push_group(Group {
                changes: vec![change],
                caret_before,
                caret_after: (end, end),
                // Whitespace ends a word: the next keystroke starts a new undo step.
                mergeable: typing && !text.contains(char::is_whitespace),
            });
        }
        end
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Undoes the last step; returns the caret/selection to restore.
    pub fn undo(&mut self) -> Option<(Pos, Pos)> {
        self.close_group();
        let g = self.undo.pop()?;
        for c in g.changes.iter().rev() {
            let end = self.advance(c.start, c.inserted.chars().count());
            self.apply(c.start, end, &c.removed);
        }
        let caret = g.caret_before;
        self.redo.push(g);
        Some(caret)
    }

    pub fn redo(&mut self) -> Option<(Pos, Pos)> {
        let g = self.redo.pop()?;
        for c in &g.changes {
            let end = self.advance(c.start, c.removed.chars().count());
            self.apply(c.start, end, &c.inserted);
        }
        let caret = g.caret_after;
        self.undo.push(g);
        Some(caret)
    }

    pub fn is_dirty(&self) -> bool {
        self.save_point != Some(self.undo.len())
            || self
                .open_group
                .as_ref()
                .is_some_and(|g| !g.changes.is_empty())
    }

    pub fn mark_saved(&mut self) {
        self.close_group();
        self.save_point = Some(self.undo.len());
    }

    /// Start of the previous word (Ctrl+Left).
    pub fn word_left(&self, p: Pos) -> Pos {
        if p.col == 0 {
            return if p.line == 0 {
                p
            } else {
                Pos::new(p.line - 1, self.lines[p.line - 1].chars)
            };
        }
        let chars: Vec<char> = self.lines[p.line].text.chars().collect();
        let mut i = p.col;
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        let word = i > 0 && is_word(chars[i - 1]);
        while i > 0 && !chars[i - 1].is_whitespace() && is_word(chars[i - 1]) == word {
            i -= 1;
        }
        Pos::new(p.line, i)
    }

    /// Start of the next word (Ctrl+Right).
    pub fn word_right(&self, p: Pos) -> Pos {
        let len = self.lines[p.line].chars;
        if p.col >= len {
            return if p.line + 1 < self.lines.len() {
                Pos::new(p.line + 1, 0)
            } else {
                p
            };
        }
        let chars: Vec<char> = self.lines[p.line].text.chars().collect();
        let mut i = p.col;
        let word = is_word(chars[i]);
        if !chars[i].is_whitespace() {
            while i < len && !chars[i].is_whitespace() && is_word(chars[i]) == word {
                i += 1;
            }
        }
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        Pos::new(p.line, i)
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl LineSource for TextBuffer {
    fn line_count(&self) -> usize {
        self.lines.len()
    }

    fn text_length(&self) -> usize {
        self.offset_of(self.end())
    }

    fn line_start(&self, line: usize) -> usize {
        self.offset_of(Pos::new(line, 0))
    }

    fn line_end(&self, line: usize) -> usize {
        self.offset_of(Pos::new(line, self.lines[line].chars))
    }

    fn get_text(&self, offset: usize, length: usize) -> String {
        let a = self.pos_of(offset);
        let b = self.pos_of(offset + length);
        self.slice(a, b)
    }
}

#[cfg(test)]
mod tests {
    use fc_core::textdiff::{TextDiffOptions, compare_lines, compute_block_copy};

    use super::*;

    const NONE: (Pos, Pos) = (Pos { line: 0, col: 0 }, Pos { line: 0, col: 0 });

    #[test]
    fn keeps_mixed_line_endings() {
        let text = "a\r\nb\nc\rd";
        let b = TextBuffer::new(text);
        assert_eq!(4, b.line_count());
        assert_eq!(text, b.text());
        assert_eq!("", TextBuffer::new("").text());
        assert_eq!(2, TextBuffer::new("x\n").line_count());
    }

    #[test]
    fn replace_and_undo_redo_restore_bytes() {
        let original = "one\r\ntwo\nthree";
        let mut b = TextBuffer::new(original);
        let end = b.replace(Pos::new(0, 1), Pos::new(1, 2), "X\r\nY", NONE, false);
        assert_eq!("oX\r\nYo\nthree", b.text());
        assert_eq!(Pos::new(1, 1), end);
        assert!(b.is_dirty());
        b.undo();
        assert_eq!(original, b.text());
        assert!(!b.is_dirty());
        b.redo();
        assert_eq!("oX\r\nYo\nthree", b.text());
    }

    #[test]
    fn typing_merges_into_one_step_until_a_space() {
        let mut b = TextBuffer::new("");
        let mut p = Pos::default();
        for c in ["a", "b", "c", " ", "d"] {
            p = b.replace(p, p, c, (p, p), true);
        }
        assert_eq!("abc d", b.text());
        b.undo();
        assert_eq!("abc ", b.text());
        b.undo();
        assert_eq!("abc", b.text());
        b.undo();
        assert_eq!("", b.text());
        assert!(!b.is_dirty());
    }

    #[test]
    fn save_point_and_dirty() {
        let mut b = TextBuffer::new("x");
        b.replace(Pos::new(0, 1), Pos::new(0, 1), "y", NONE, false);
        b.mark_saved();
        assert!(!b.is_dirty());
        b.undo();
        assert!(b.is_dirty());
        b.redo();
        assert!(!b.is_dirty());
        b.undo();
        b.replace(Pos::new(0, 0), Pos::new(0, 0), "z", NONE, false);
        b.undo();
        assert!(b.is_dirty(), "the saved state is gone after a new edit");
    }

    #[test]
    fn group_is_one_undo_step() {
        let mut b = TextBuffer::new("a\nb\nc");
        b.begin_group(NONE);
        b.replace(Pos::new(2, 0), Pos::new(2, 1), "C", NONE, false);
        b.replace(Pos::new(0, 0), Pos::new(0, 1), "A", NONE, false);
        b.end_group(NONE);
        assert_eq!("A\nb\nC", b.text());
        b.undo();
        assert_eq!("a\nb\nc", b.text());
    }

    #[test]
    fn offsets_and_line_source_match_a_string() {
        let text = "ä\r\nbc\nd";
        let b = TextBuffer::new(text);
        let s = fc_core::textdiff::StringLineSource::new(text);
        assert_eq!(s.text_length(), b.text_length());
        for l in 0..3 {
            assert_eq!(s.line_start(l), b.line_start(l));
            assert_eq!(s.line_end(l), b.line_end(l));
        }
        assert_eq!("\r\nbc", b.get_text(1, 4));
        assert_eq!(Pos::new(1, 0), b.pos_of(3));
        assert_eq!(
            Pos::new(0, 1),
            b.pos_of(2),
            "inside a CRLF snaps to the line end"
        );
    }

    #[test]
    fn block_copy_through_the_buffer() {
        let left = "a\r\nX\r\nY\r\nb";
        let right = "a\r\nq\r\nb";
        let mut target = TextBuffer::new(right);
        let source = TextBuffer::new(left);
        let blocks = compare_lines(
            &source.lines_text(),
            &target.lines_text(),
            TextDiffOptions::default(),
        );
        let block = blocks.iter().find(|b| b.is_difference()).unwrap();
        let edit = compute_block_copy(&source, &target, block, true, "\r\n");
        let a = target.pos_of(edit.offset);
        let e = target.pos_of(edit.offset + edit.length);
        target.replace(a, e, &edit.text, NONE, false);
        assert_eq!(left, target.text());
    }

    #[test]
    fn words() {
        let b = TextBuffer::new("foo  bar.baz");
        assert_eq!(Pos::new(0, 5), b.word_right(Pos::new(0, 0)));
        assert_eq!(Pos::new(0, 8), b.word_right(Pos::new(0, 5)));
        assert_eq!(Pos::new(0, 9), b.word_left(Pos::new(0, 12)));
        assert_eq!(Pos::new(0, 5), b.word_left(Pos::new(0, 8)));
    }
}
