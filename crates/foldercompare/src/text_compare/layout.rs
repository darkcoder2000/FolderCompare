//! Aligned rows of one pane: every visual row is exactly one line height and is either a text line or a filler
//! row from the comparison (`filler_above`, `filler_below`). Both panes share the same row offset.

use fc_core::textdiff::SideInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Line(usize),
    Filler,
}

#[derive(Debug, Clone, Default)]
pub struct RowMap {
    filler_above: usize,
    /// Row of each line.
    line_row: Vec<usize>,
    total: usize,
}

impl RowMap {
    /// The comparison may lag behind typing; lines it doesn't know get no fillers.
    pub fn new(line_count: usize, side: Option<&SideInfo>) -> Self {
        let filler_above = side.map_or(0, |s| s.filler_above);
        let mut line_row = Vec::with_capacity(line_count);
        let mut row = filler_above;
        for i in 0..line_count {
            line_row.push(row);
            let b = side
                .filter(|s| i < s.line_count())
                .map_or(0, |s| s.filler_below(i));
            row += 1 + b;
        }
        Self {
            filler_above,
            line_row,
            total: row,
        }
    }

    pub fn total_rows(&self) -> usize {
        self.total
    }

    pub fn row_of_line(&self, line: usize) -> usize {
        self.line_row
            .get(line)
            .copied()
            .unwrap_or(self.total.saturating_sub(1))
    }

    pub fn kind(&self, row: usize) -> Option<RowKind> {
        if row >= self.total {
            return None;
        }
        if row < self.filler_above {
            return Some(RowKind::Filler);
        }
        let i = self.line_row.partition_point(|&r| r <= row) - 1;
        Some(if self.line_row[i] == row {
            RowKind::Line(i)
        } else {
            RowKind::Filler
        })
    }

    /// The line at or nearest above a row (for clicks on filler rows).
    pub fn line_near(&self, row: usize) -> usize {
        if self.line_row.is_empty() {
            return 0;
        }
        let i = self.line_row.partition_point(|&r| r <= row);
        i.saturating_sub(1).min(self.line_row.len() - 1)
    }
}

#[cfg(test)]
mod tests {
    use fc_core::textdiff::{TextComparison, TextDiffOptions};

    use super::*;

    #[test]
    fn both_sides_have_the_same_rows() {
        let c = TextComparison::compute("a\nb\nc", "x\na\nX\nY\nZ\nc", TextDiffOptions::default());
        let l = RowMap::new(3, Some(&c.left));
        let r = RowMap::new(6, Some(&c.right));
        assert_eq!(c.total_rows, l.total_rows());
        assert_eq!(c.total_rows, r.total_rows());
        assert_eq!(Some(RowKind::Filler), l.kind(0));
        assert_eq!(Some(RowKind::Line(0)), l.kind(1));
        assert_eq!(Some(RowKind::Line(1)), l.kind(2));
        assert_eq!(Some(RowKind::Filler), l.kind(3));
        assert_eq!(Some(RowKind::Line(2)), l.kind(5));
        assert_eq!(None, l.kind(6));
        assert_eq!(1, l.line_near(4));
        assert_eq!(5, r.row_of_line(5));
    }

    #[test]
    fn without_comparison_rows_are_lines() {
        let m = RowMap::new(3, None);
        assert_eq!(3, m.total_rows());
        assert_eq!(Some(RowKind::Line(2)), m.kind(2));
    }
}
