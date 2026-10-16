//! Line and word diff for the text compare window (own Myers implementation), block copies and the text codec.
//! All offsets inside lines are char indices.

mod block_copy;
pub mod codec;
mod comparison;
mod inline;
mod lines;
mod myers;

pub use block_copy::{
    LineSource, StringLineSource, TextEdit, compute_block_copy, normalize_newlines,
};
pub use codec::{LoadedText, TextEncoding};
pub use comparison::{MAX_INLINE_LINE_LENGTH, SideInfo, TextComparison};
pub use inline::{TextRange, compare_inline, tokenize};
pub use lines::{DiffBlock, DiffBlockKind, TextDiffOptions, compare_lines, split_lines};
pub use myers::compute as myers_diff;
