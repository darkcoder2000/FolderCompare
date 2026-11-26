mod common;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use common::*;
use fc_core::compare::{
    BUFFER_SIZE, ByteContentComparer, Cancelled, ContentComparer, ContentError, compare_files,
};
use fc_core::model::{Attributes, CompareOptions, ContentCompareMode, DiffStatus, FileSnapshot};

fn opts(mode: ContentCompareMode) -> CompareOptions {
    CompareOptions {
        content_compare: mode,
        ..CompareOptions::default()
    }
}

fn off() -> CompareOptions {
    opts(ContentCompareMode::Off)
}
fn when_times_equal() -> CompareOptions {
    opts(ContentCompareMode::WhenTimestampsEqual)
}
fn when_sizes_equal() -> CompareOptions {
    opts(ContentCompareMode::WhenSizesEqual)
}

fn file(path: &Path, modified: SystemTime, content: &[u8]) -> FileSnapshot {
    add_file(path, modified, content);
    FileSnapshot {
        size: content.len() as u64,
        modified,
        attributes: Attributes(0),
        unix_mode: None,
        full_path: path.to_path_buf(),
    }
}

fn cmp(l: &FileSnapshot, r: &FileSnapshot, o: &CompareOptions) -> DiffStatus {
    compare_files(
        l,
        r,
        o,
        Some(&ByteContentComparer),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap()
    .status
}

struct ThrowingComparer(&'static str);

impl ContentComparer for ThrowingComparer {
    fn are_equal(
        &self,
        _: &Path,
        _: &Path,
        _: &mut dyn FnMut(u64),
        _: &AtomicBool,
    ) -> Result<bool, ContentError> {
        Err(ContentError::Io(self.0.to_string()))
    }
}

#[test]
fn same_time_different_size_is_same_time_different_content() {
    for mode in [
        ContentCompareMode::Off,
        ContentCompareMode::WhenTimestampsEqual,
        ContentCompareMode::WhenSizesEqual,
    ] {
        let f = Fixture::new();
        let l = file(&f.left("a.txt"), t0(), b"short");
        let r = file(&f.right("a.txt"), t0(), b"a lot longer");
        assert_eq!(
            DiffStatus::SameTimeDifferentContent,
            cmp(&l, &r, &opts(mode))
        );
    }
}

#[test]
fn same_time_same_size_different_content_mode_off_is_identical() {
    // Known limitation of the fast mode: the content is not read.
    let f = Fixture::new();
    let l = file(&f.left("a.txt"), t0(), b"abc");
    let r = file(&f.right("a.txt"), t0(), b"xyz");
    assert_eq!(DiffStatus::Identical, cmp(&l, &r, &off()));
}

#[test]
fn same_time_same_size_different_content_verified_is_same_time_different_content() {
    let f = Fixture::new();
    let l = file(&f.left("a.txt"), t0(), b"abc");
    let r = file(&f.right("a.txt"), t0() + secs(1), b"xyz");
    assert_eq!(
        DiffStatus::SameTimeDifferentContent,
        cmp(&l, &r, &when_times_equal())
    );
    assert_eq!(
        DiffStatus::SameTimeDifferentContent,
        cmp(&l, &r, &when_sizes_equal())
    );
}

#[test]
fn same_time_same_content_verified_is_identical() {
    let f = Fixture::new();
    let l = file(&f.left("a.txt"), t0(), b"abc");
    let r = file(&f.right("a.txt"), t0(), b"abc");
    assert_eq!(DiffStatus::Identical, cmp(&l, &r, &when_times_equal()));
}

#[test]
fn different_time_same_content() {
    let f = Fixture::new();
    let l = file(&f.left("a.txt"), t0() + secs(3 * 3600), b"abc");
    let r = file(&f.right("a.txt"), t0(), b"abc");
    assert_eq!(DiffStatus::NewerLeft, cmp(&l, &r, &off()));
    assert_eq!(DiffStatus::NewerLeft, cmp(&l, &r, &when_times_equal()));
    assert_eq!(
        DiffStatus::SameContentDifferentTime,
        cmp(&l, &r, &when_sizes_equal())
    );
}

#[test]
fn different_time_same_size_different_content_keeps_direction() {
    let f = Fixture::new();
    let l = file(&f.left("a.txt"), t0(), b"abc");
    let r = file(&f.right("a.txt"), t0() + secs(3 * 3600), b"xyz");
    assert_eq!(DiffStatus::NewerRight, cmp(&l, &r, &when_sizes_equal()));
}

#[test]
fn different_time_different_size_keeps_direction() {
    let f = Fixture::new();
    let l = file(&f.left("a.txt"), t0() + secs(300), b"short");
    let r = file(&f.right("a.txt"), t0(), b"a lot longer");
    assert_eq!(DiffStatus::NewerLeft, cmp(&l, &r, &when_sizes_equal()));
    assert_eq!(DiffStatus::NewerRight, cmp(&r, &l, &off()));
}

#[test]
fn dst_offset_with_different_content_is_not_identical() {
    let f = Fixture::new();
    let options = CompareOptions {
        ignore_dst_offset: true,
        ..when_times_equal()
    };
    let l = file(&f.left("a.txt"), t0() + secs(3600), b"abc");
    let r = file(&f.right("a.txt"), t0(), b"xyz");
    assert_eq!(DiffStatus::SameTimeDifferentContent, cmp(&l, &r, &options));
}

/// Small deterministic generator for test data.
fn pseudo_random(len: usize, mut seed: u64) -> Vec<u8> {
    (0..len)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as u8
        })
        .collect()
}

#[test]
fn large_file_differing_only_in_last_byte_is_detected() {
    let f = Fixture::new();
    let a = pseudo_random(BUFFER_SIZE * 3 + 17, 42);
    let mut b = a.clone();
    *b.last_mut().unwrap() ^= 0xFF;
    let l = file(&f.left("big.bin"), t0(), &a);
    let r = file(&f.right("big.bin"), t0(), &b);
    let same = file(&f.right("same.bin"), t0(), &a);

    assert_eq!(
        DiffStatus::SameTimeDifferentContent,
        cmp(&l, &r, &when_times_equal())
    );
    assert_eq!(DiffStatus::Identical, cmp(&l, &same, &when_times_equal()));
}

#[test]
fn empty_files_are_identical() {
    let f = Fixture::new();
    let l = file(&f.left("e.txt"), t0(), b"");
    let r = file(&f.right("e.txt"), t0(), b"");
    assert_eq!(DiffStatus::Identical, cmp(&l, &r, &when_sizes_equal()));
}

#[test]
fn mode_off_never_reads_content() {
    let f = Fixture::new();
    let l = file(&f.left("a.txt"), t0(), b"abc");
    let r = file(&f.right("a.txt"), t0(), b"abc");
    let result = compare_files(
        &l,
        &r,
        &off(),
        Some(&ThrowingComparer("must not read")),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(DiffStatus::Identical, result.status);
}

#[cfg(windows)]
#[test]
fn locked_file_is_error() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = Fixture::new();
    let l = file(&f.left("a.txt"), t0(), b"abc");
    let r = file(&f.right("a.txt"), t0(), b"abc");
    let _lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&r.full_path)
        .unwrap();

    let result = compare_files(
        &l,
        &r,
        &when_times_equal(),
        Some(&ByteContentComparer),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();

    assert_eq!(DiffStatus::Error, result.status);
    assert!(result.error.unwrap().starts_with("Content compare failed:"));
}

#[cfg(unix)]
#[test]
fn unreadable_file_is_error() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let l = file(&f.left("a.txt"), t0(), b"abc");
    let r = file(&f.right("a.txt"), t0(), b"abc");
    std::fs::set_permissions(&r.full_path, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::File::open(&r.full_path).is_ok() {
        return; // running as root: permissions are not enforced
    }
    let result = compare_files(
        &l,
        &r,
        &when_times_equal(),
        Some(&ByteContentComparer),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(DiffStatus::Error, result.status);
    assert!(result.error.unwrap().starts_with("Content compare failed:"));
}

#[test]
fn read_errors_are_error() {
    let f = Fixture::new();
    let l = file(&f.left("a.txt"), t0(), b"abc");
    let r = file(&f.right("a.txt"), t0(), b"abc");
    let result = compare_files(
        &l,
        &r,
        &when_times_equal(),
        Some(&ThrowingComparer("Access is denied.")),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(DiffStatus::Error, result.status);
    assert_eq!(
        Some("Content compare failed: Access is denied.".to_string()),
        result.error
    );
}

#[test]
fn cancellation_during_content_compare_returns_cancelled() {
    let f = Fixture::new();
    let data = vec![0u8; BUFFER_SIZE * 4];
    let l = file(&f.left("big.bin"), t0(), &data);
    let r = file(&f.right("big.bin"), t0(), &data);
    let cancel = AtomicBool::new(false);
    let result = compare_files(
        &l,
        &r,
        &when_times_equal(),
        Some(&ByteContentComparer),
        &cancel,
        &mut |_| cancel.store(true, Ordering::Relaxed),
    );
    assert_eq!(Err(Cancelled), result);
}
