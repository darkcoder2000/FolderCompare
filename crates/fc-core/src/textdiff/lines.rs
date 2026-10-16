use std::collections::HashMap;

use super::myers;
use crate::compare::upper_char;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextDiffOptions {
    /// Treat lines that differ only in the amount of whitespace as unimportant.
    pub ignore_whitespace: bool,
    /// Treat lines that differ only in letter case as unimportant.
    pub ignore_case: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DiffBlockKind {
    #[default]
    Equal,
    /// Lines differ only in ways the options declare unimportant (shown blue).
    Unimportant,
    Changed,
    LeftOnly,
    RightOnly,
}

/// A run of lines. For one-sided blocks the empty side's start is the insertion point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiffBlock {
    pub left_start: usize,
    pub left_count: usize,
    pub right_start: usize,
    pub right_count: usize,
    pub kind: DiffBlockKind,
}

impl DiffBlock {
    pub fn new(
        left_start: usize,
        left_count: usize,
        right_start: usize,
        right_count: usize,
        kind: DiffBlockKind,
    ) -> Self {
        Self {
            left_start,
            left_count,
            right_start,
            right_count,
            kind,
        }
    }

    pub fn left_end(&self) -> usize {
        self.left_start + self.left_count
    }

    pub fn right_end(&self) -> usize {
        self.right_start + self.right_count
    }

    pub fn start(&self, left: bool) -> usize {
        if left {
            self.left_start
        } else {
            self.right_start
        }
    }

    pub fn count(&self, left: bool) -> usize {
        if left {
            self.left_count
        } else {
            self.right_count
        }
    }

    pub fn is_difference(&self) -> bool {
        matches!(
            self.kind,
            DiffBlockKind::Changed | DiffBlockKind::LeftOnly | DiffBlockKind::RightOnly
        )
    }

    pub fn height(&self) -> usize {
        self.left_count.max(self.right_count)
    }
}

/// Splits text at `\r\n`, `\n` or `\r`. Like an editor, `"a\n"` yields two lines (`"a"` and `""`), `""` one.
pub fn split_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let bytes = text.as_bytes();
    let (mut start, mut n) = (0, 0);
    while n < bytes.len() {
        let c = bytes[n];
        if c == b'\r' || c == b'\n' {
            lines.push(text[start..n].to_string());
            if c == b'\r' && bytes.get(n + 1) == Some(&b'\n') {
                n += 1;
            }
            start = n + 1;
        }
        n += 1;
    }
    lines.push(text[start..].to_string());
    lines
}

/// Compares two line lists and returns consecutive blocks that together cover both sides.
pub fn compare_lines<S: AsRef<str>>(
    left: &[S],
    right: &[S],
    options: TextDiffOptions,
) -> Vec<DiffBlock> {
    let mut keys: HashMap<String, u32> = HashMap::new();
    let a = line_keys(left, options, &mut keys);
    let b = line_keys(right, options, &mut keys);
    let (mod_a, mod_b) = myers::compute(&a, &b);

    let mut blocks = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        let (si, sj) = (i, j);
        while i < a.len() && j < b.len() && !mod_a[i] && !mod_b[j] {
            i += 1;
            j += 1;
        }
        if i > si {
            add_equal_run(&mut blocks, left, right, si, sj, i - si);
        }

        let (si, sj) = (i, j);
        while i < a.len() && mod_a[i] {
            i += 1;
        }
        while j < b.len() && mod_b[j] {
            j += 1;
        }
        let (ca, cb) = (i - si, j - sj);
        if ca == 0 && cb == 0 {
            continue;
        }
        let kind = if ca == 0 {
            DiffBlockKind::RightOnly
        } else if cb == 0 {
            DiffBlockKind::LeftOnly
        } else {
            DiffBlockKind::Changed
        };
        blocks.push(DiffBlock::new(si, ca, sj, cb, kind));
    }
    blocks
}

/// Splits an equal-key run into Equal and Unimportant blocks by comparing the raw text.
fn add_equal_run<S: AsRef<str>>(
    blocks: &mut Vec<DiffBlock>,
    left: &[S],
    right: &[S],
    i: usize,
    j: usize,
    count: usize,
) {
    let same_at = |n: usize| left[i + n].as_ref() == right[j + n].as_ref();
    let mut start = 0;
    while start < count {
        let same = same_at(start);
        let mut end = start + 1;
        while end < count && same_at(end) == same {
            end += 1;
        }
        let kind = if same {
            DiffBlockKind::Equal
        } else {
            DiffBlockKind::Unimportant
        };
        blocks.push(DiffBlock::new(
            i + start,
            end - start,
            j + start,
            end - start,
            kind,
        ));
        start = end;
    }
}

fn line_keys<S: AsRef<str>>(
    lines: &[S],
    options: TextDiffOptions,
    keys: &mut HashMap<String, u32>,
) -> Vec<u32> {
    lines
        .iter()
        .map(|line| {
            let key = normalize(line.as_ref(), options);
            let next = keys.len() as u32;
            *keys.entry(key).or_insert(next)
        })
        .collect()
}

/// Collapses whitespace runs to one space and trims (ignore whitespace), upper-cases 1:1 per char (ignore case).
pub(crate) fn normalize(line: &str, options: TextDiffOptions) -> String {
    let mut out = if options.ignore_whitespace {
        let mut s = String::with_capacity(line.len());
        let mut pending_space = false;
        for c in line.chars() {
            if c.is_whitespace() {
                pending_space = !s.is_empty();
                continue;
            }
            if pending_space {
                s.push(' ');
            }
            pending_space = false;
            s.push(c);
        }
        s
    } else {
        line.to_string()
    };
    if options.ignore_case {
        out = out.chars().map(upper_char).collect();
    }
    out
}
