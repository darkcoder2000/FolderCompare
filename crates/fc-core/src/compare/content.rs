use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{Cancelled, compare_times, io_message};
use crate::model::{CompareOptions, ContentCompareMode, DiffStatus, FileSnapshot};

/// Chunk size of [`ByteContentComparer`].
pub const BUFFER_SIZE: usize = 128 * 1024;

#[derive(Debug)]
pub enum ContentError {
    /// Reading failed (message of the I/O error).
    Io(String),
    Cancelled,
}

impl From<io::Error> for ContentError {
    fn from(e: io::Error) -> Self {
        ContentError::Io(io_message(&e))
    }
}

/// Pairwise byte comparison of two files. Implementations must never write.
pub trait ContentComparer: Sync {
    /// Returns true when both files have the same bytes. `bytes_read` receives the number of bytes compared per
    /// chunk (per pair, not per side).
    fn are_equal(
        &self,
        left: &Path,
        right: &Path,
        bytes_read: &mut dyn FnMut(u64),
        cancel: &AtomicBool,
    ) -> Result<bool, ContentError>;
}

/// Streams both files in chunks and stops at the first difference. Opens files read-only (on Windows sharing
/// read, write and delete, so files open in other programs can still be compared).
#[derive(Debug, Default, Clone, Copy)]
pub struct ByteContentComparer;

impl ContentComparer for ByteContentComparer {
    fn are_equal(
        &self,
        left: &Path,
        right: &Path,
        bytes_read: &mut dyn FnMut(u64),
        cancel: &AtomicBool,
    ) -> Result<bool, ContentError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(ContentError::Cancelled);
        }
        let mut l = File::open(left)?;
        let mut r = File::open(right)?;
        if l.metadata()?.len() != r.metadata()?.len() {
            return Ok(false);
        }
        let mut a = vec![0u8; BUFFER_SIZE];
        let mut b = vec![0u8; BUFFER_SIZE];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(ContentError::Cancelled);
            }
            let na = fill(&mut l, &mut a)?;
            let nb = fill(&mut r, &mut b)?;
            if na != nb || a[..na] != b[..nb] {
                return Ok(false);
            }
            if na == 0 {
                return Ok(true);
            }
            bytes_read(na as u64);
        }
    }
}

/// Reads until the chunk is full or the file ends, so short reads cannot misalign both sides.
fn fill(file: &mut File, buffer: &mut [u8]) -> io::Result<usize> {
    let mut total = 0;
    while total < buffer.len() {
        match file.read(&mut buffer[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(total)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileComparison {
    pub status: DiffStatus,
    pub error: Option<String>,
}

impl FileComparison {
    fn of(status: DiffStatus) -> Self {
        Self {
            status,
            error: None,
        }
    }
}

fn time_status(left: &FileSnapshot, right: &FileSnapshot, options: &CompareOptions) -> DiffStatus {
    compare_times(
        left.modified,
        right.modified,
        options.tolerance,
        options.ignore_dst_offset,
    )
}

/// True when [`compare_files`] would read the file contents for this pair.
pub fn needs_content_check(
    left: &FileSnapshot,
    right: &FileSnapshot,
    options: &CompareOptions,
) -> bool {
    if left.size != right.size || options.content_compare == ContentCompareMode::Off {
        return false;
    }
    let same_time = time_status(left, right, options) == DiffStatus::Identical;
    same_time || options.content_compare == ContentCompareMode::WhenSizesEqual
}

/// Decides the status of a file that exists on both sides: size first (always), then timestamp, then optionally
/// the content. With `content` = `None` the content is never read. Read errors produce
/// [`DiffStatus::Error`]; cancellation returns `Err`.
pub fn compare_files(
    left: &FileSnapshot,
    right: &FileSnapshot,
    options: &CompareOptions,
    content: Option<&dyn ContentComparer>,
    cancel: &AtomicBool,
    bytes_read: &mut dyn FnMut(u64),
) -> Result<FileComparison, Cancelled> {
    let time = time_status(left, right, options);
    let same_time = time == DiffStatus::Identical;

    if left.size != right.size {
        return Ok(FileComparison::of(if same_time {
            DiffStatus::SameTimeDifferentContent
        } else {
            time
        }));
    }

    let Some(content) = content else {
        return Ok(FileComparison::of(time));
    };
    if !needs_content_check(left, right, options) {
        return Ok(FileComparison::of(time));
    }

    let equal = match content.are_equal(&left.full_path, &right.full_path, bytes_read, cancel) {
        Ok(eq) => eq,
        Err(ContentError::Cancelled) => return Err(Cancelled),
        Err(ContentError::Io(msg)) => {
            return Ok(FileComparison {
                status: DiffStatus::Error,
                error: Some(format!("Content compare failed: {msg}")),
            });
        }
    };

    Ok(FileComparison::of(match (same_time, equal) {
        (true, true) => DiffStatus::Identical,
        (true, false) => DiffStatus::SameTimeDifferentContent,
        (false, true) => DiffStatus::SameContentDifferentTime,
        (false, false) => time,
    }))
}
