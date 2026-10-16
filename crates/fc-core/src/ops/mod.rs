//! File operations: safety checks, planning and executing copies and deletes.

mod safety;

pub use safety::{UnsafePath, ensure_inside_root, is_inside, normalize, validate_roots};
