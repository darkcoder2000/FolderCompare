//! Scanning both folder trees and merging them into a [`crate::model::DiffTree`]. Never writes.

mod content;
mod engine;
mod glob;
mod path_comparison;
mod progress;
mod scanner;
mod timestamp;

pub use content::{
    BUFFER_SIZE, ByteContentComparer, ContentComparer, ContentError, FileComparison, compare_files,
    needs_content_check,
};
pub use engine::{
    CompareError, DiffEngine, DiffResult, RefreshData, RefreshRequest, roll_up, roll_up_tree,
};
pub use glob::PathPatternMatcher;
pub use path_comparison::{PathComparison, upper_char};
pub use scanner::MAX_DEPTH;
pub use timestamp::{compare_times, is_newer};

/// The operation was cancelled through its cancel flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("The operation was cancelled.")
    }
}

impl std::error::Error for Cancelled {}

/// The message of an I/O error without the " (os error N)" suffix.
pub fn io_message(err: &std::io::Error) -> String {
    let text = err.to_string();
    match text.rfind(" (os error ") {
        Some(i) if text.ends_with(')') => text[..i].to_string(),
        _ => text,
    }
}
