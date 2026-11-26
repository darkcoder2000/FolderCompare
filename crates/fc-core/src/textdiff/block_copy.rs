//! Copying one diff block from one side to the other, as a single text edit. Offsets are in chars.

use super::{DiffBlock, split_lines};

/// Line-oriented view of a text buffer (a string in tests, the editor buffer in the app).
pub trait LineSource {
    fn line_count(&self) -> usize;
    /// Length of the whole text in chars.
    fn text_length(&self) -> usize;
    /// Offset of the first char of a 0-based line.
    fn line_start(&self, line: usize) -> usize;
    /// Offset just after the last char of a line, excluding its line break.
    fn line_end(&self, line: usize) -> usize;
    fn get_text(&self, offset: usize, length: usize) -> String;
}

/// A [`LineSource`] over a string.
pub struct StringLineSource {
    chars: Vec<char>,
    lines: Vec<(usize, usize)>,
}

impl StringLineSource {
    pub fn new(text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let mut lines = Vec::new();
        let (mut start, mut n) = (0, 0);
        while n < chars.len() {
            let c = chars[n];
            if c == '\r' || c == '\n' {
                lines.push((start, n));
                if c == '\r' && chars.get(n + 1) == Some(&'\n') {
                    n += 1;
                }
                start = n + 1;
            }
            n += 1;
        }
        lines.push((start, chars.len()));
        Self { chars, lines }
    }
}

impl LineSource for StringLineSource {
    fn line_count(&self) -> usize {
        self.lines.len()
    }
    fn text_length(&self) -> usize {
        self.chars.len()
    }
    fn line_start(&self, line: usize) -> usize {
        self.lines[line].0
    }
    fn line_end(&self, line: usize) -> usize {
        self.lines[line].1
    }
    fn get_text(&self, offset: usize, length: usize) -> String {
        self.chars[offset..offset + length].iter().collect()
    }
}

/// A single replace operation on the target text (char offsets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub offset: usize,
    pub length: usize,
    pub text: String,
}

impl TextEdit {
    fn new(offset: usize, length: usize, text: String) -> Self {
        Self {
            offset,
            length,
            text,
        }
    }

    /// Applies the edit to a string (for tests and simple buffers).
    pub fn apply(&self, target: &str) -> String {
        let chars: Vec<char> = target.chars().collect();
        let mut out: String = chars[..self.offset].iter().collect();
        out.push_str(&self.text);
        out.extend(&chars[self.offset + self.length..]);
        out
    }
}

/// Computes the edit that makes the target's lines of `block` equal to the source's. `source_is_left` picks
/// which side of the block is the source. Line breaks inside the copied text become `target_newline`.
pub fn compute_block_copy(
    source: &dyn LineSource,
    target: &dyn LineSource,
    block: &DiffBlock,
    source_is_left: bool,
    target_newline: &str,
) -> TextEdit {
    let (s_start, s_count) = (block.start(source_is_left), block.count(source_is_left));
    let (t_start, t_count) = (block.start(!source_is_left), block.count(!source_is_left));

    let text = if s_count == 0 {
        String::new()
    } else {
        let start = source.line_start(s_start);
        normalize_newlines(
            &source.get_text(start, source.line_end(s_start + s_count - 1) - start),
            target_newline,
        )
    };

    if s_count > 0 && t_count > 0 {
        let start = target.line_start(t_start);
        return TextEdit::new(start, target.line_end(t_start + t_count - 1) - start, text);
    }

    if s_count > 0 {
        // Insert whole lines before target line t_start, or append after the last line.
        return if t_start < target.line_count() {
            TextEdit::new(
                target.line_start(t_start),
                0,
                format!("{text}{target_newline}"),
            )
        } else {
            TextEdit::new(target.text_length(), 0, format!("{target_newline}{text}"))
        };
    }

    if t_count == 0 {
        return TextEdit::new(0, 0, String::new());
    }

    // Remove target lines together with one line break.
    let t_end = t_start + t_count;
    if t_end < target.line_count() {
        let start = target.line_start(t_start);
        return TextEdit::new(start, target.line_start(t_end) - start, String::new());
    }
    if t_start > 0 {
        let start = target.line_end(t_start - 1);
        return TextEdit::new(start, target.text_length() - start, String::new());
    }
    TextEdit::new(0, target.text_length(), String::new())
}

pub fn normalize_newlines(text: &str, newline: &str) -> String {
    if !text.contains(['\r', '\n']) {
        return text.to_string();
    }
    split_lines(text).join(newline)
}
