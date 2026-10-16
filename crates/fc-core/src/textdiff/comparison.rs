use std::collections::HashMap;

use super::{
    DiffBlock, DiffBlockKind, TextDiffOptions, TextRange, compare_inline, compare_lines,
    split_lines,
};

/// In-line highlighting is skipped for lines longer than this (in chars); the whole line is marked instead.
pub const MAX_INLINE_LINE_LENGTH: usize = 4000;

/// One side of a [`TextComparison`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SideInfo {
    /// Kind of every line on this side.
    pub kinds: Vec<DiffBlockKind>,
    /// Differing char ranges inside changed lines.
    pub inline: HashMap<usize, Vec<TextRange>>,
    /// Filler rows to show below a line.
    pub filler_below: HashMap<usize, usize>,
    /// Filler rows to show above the first line.
    pub filler_above: usize,
}

impl SideInfo {
    fn new(line_count: usize) -> Self {
        Self {
            kinds: vec![DiffBlockKind::Equal; line_count],
            ..Self::default()
        }
    }

    pub fn line_count(&self) -> usize {
        self.kinds.len()
    }

    pub fn filler_below(&self, line: usize) -> usize {
        self.filler_below.get(&line).copied().unwrap_or(0)
    }

    fn add_filler(&mut self, start: usize, count: usize, rows: usize) {
        if rows == 0 {
            return;
        }
        if count > 0 || start > 0 {
            let line = if count > 0 {
                start + count - 1
            } else {
                start - 1
            };
            *self.filler_below.entry(line).or_insert(0) += rows;
        } else {
            self.filler_above += rows;
        }
    }
}

/// The result of comparing two texts, prepared for a side-by-side view: per-line kinds, in-line ranges, and the
/// filler rows each side needs so that corresponding lines sit at the same height.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextComparison {
    pub blocks: Vec<DiffBlock>,
    pub left: SideInfo,
    pub right: SideInfo,
    pub difference_count: usize,
    /// Number of aligned rows (lines plus fillers); the same on both sides.
    pub total_rows: usize,
    block_row: Vec<usize>,
}

impl TextComparison {
    pub fn compute(left_text: &str, right_text: &str, options: TextDiffOptions) -> Self {
        Self::compute_lines(&split_lines(left_text), &split_lines(right_text), options)
    }

    /// Compares two texts that are already split into lines (without line breaks).
    pub fn compute_lines<S: AsRef<str>>(
        left_lines: &[S],
        right_lines: &[S],
        options: TextDiffOptions,
    ) -> Self {
        let blocks = compare_lines(left_lines, right_lines, options);
        let mut left = SideInfo::new(left_lines.len());
        let mut right = SideInfo::new(right_lines.len());
        let exact = TextDiffOptions::default();

        for b in &blocks {
            left.kinds[b.left_start..b.left_end()].fill(b.kind);
            right.kinds[b.right_start..b.right_end()].fill(b.kind);

            if matches!(b.kind, DiffBlockKind::Changed | DiffBlockKind::Unimportant) {
                for n in 0..b.left_count.min(b.right_count) {
                    let l = left_lines[b.left_start + n].as_ref();
                    let r = right_lines[b.right_start + n].as_ref();
                    if l.len() > MAX_INLINE_LINE_LENGTH
                        && l.chars().count() > MAX_INLINE_LINE_LENGTH
                        || r.len() > MAX_INLINE_LINE_LENGTH
                            && r.chars().count() > MAX_INLINE_LINE_LENGTH
                    {
                        continue;
                    }
                    let opts = if b.kind == DiffBlockKind::Unimportant {
                        exact
                    } else {
                        options
                    };
                    let (lr, rr) = compare_inline(l, r, opts);
                    if !lr.is_empty() {
                        left.inline.insert(b.left_start + n, lr);
                    }
                    if !rr.is_empty() {
                        right.inline.insert(b.right_start + n, rr);
                    }
                }
            }

            left.add_filler(b.left_start, b.left_count, b.height() - b.left_count);
            right.add_filler(b.right_start, b.right_count, b.height() - b.right_count);
        }

        let mut block_row = Vec::with_capacity(blocks.len());
        let mut row = 0;
        for b in &blocks {
            block_row.push(row);
            row += b.height();
        }
        let difference_count = blocks.iter().filter(|b| b.is_difference()).count();
        Self {
            blocks,
            left,
            right,
            difference_count,
            total_rows: row,
            block_row,
        }
    }

    pub fn side(&self, left: bool) -> &SideInfo {
        if left { &self.left } else { &self.right }
    }

    /// The aligned row at which a block starts.
    pub fn row_of(&self, block_index: usize) -> usize {
        self.block_row[block_index]
    }

    /// Index of the block that contains a line on one side. Blocks without lines on that side are never returned.
    pub fn block_at(&self, left: bool, line: usize) -> Option<usize> {
        let (mut lo, mut hi) = (0isize, self.blocks.len() as isize - 1);
        while lo <= hi {
            let mid = (lo + hi) / 2;
            let b = &self.blocks[mid as usize];
            let (start, end) = (b.start(left), b.start(left) + b.count(left));
            if line < start {
                hi = mid - 1;
            } else if line >= end {
                lo = mid + 1;
            } else {
                return Some(mid as usize);
            }
        }
        None
    }

    /// The line on the other side that sits next to `line`, if any.
    pub fn partner_line(&self, from_left: bool, line: usize) -> Option<usize> {
        let b = &self.blocks[self.block_at(from_left, line)?];
        let offset = line - b.start(from_left);
        (offset < b.count(!from_left)).then(|| b.start(!from_left) + offset)
    }
}
