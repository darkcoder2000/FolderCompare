//! FolderCompare core: folder scanning and comparison, file operations, reports and the text diff.
//! Has no UI dependencies (the counterpart of `FolderCompare.Core`).

pub mod compare;
pub mod model;
pub mod ops;
pub mod report;
pub mod shell_layout;
pub mod textdiff;

/// Line ending of generated reports.
pub const NEWLINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };
