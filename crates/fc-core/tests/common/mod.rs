//! Shared fixtures: a temp folder with `left` and `right` roots on the real file system.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fc_core::compare::{DiffEngine, DiffResult};
use fc_core::model::{CompareOptions, DiffNode, NodeId};
use tempfile::TempDir;

/// 2024-05-01 12:00 UTC.
pub fn t0() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_714_564_800)
}

pub fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

pub fn ms(m: u64) -> Duration {
    Duration::from_millis(m)
}

/// Maps a test path written with `\` (or `/`) to the OS separator.
pub fn p(rel: &str) -> String {
    rel.replace(['\\', '/'], std::path::MAIN_SEPARATOR_STR)
}

pub struct Fixture {
    pub dir: TempDir,
    pub l: PathBuf,
    pub r: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("fc-test-")
            .tempdir()
            .unwrap();
        let l = dir.path().join("left");
        let r = dir.path().join("right");
        fs::create_dir_all(&l).unwrap();
        fs::create_dir_all(&r).unwrap();
        Self { dir, l, r }
    }

    pub fn left(&self, rel: &str) -> PathBuf {
        self.l.join(p(rel))
    }

    pub fn right(&self, rel: &str) -> PathBuf {
        self.r.join(p(rel))
    }

    pub fn ls(&self) -> String {
        self.l.to_string_lossy().into_owned()
    }

    pub fn rs(&self) -> String {
        self.r.to_string_lossy().into_owned()
    }

    pub fn compare(&self) -> DiffResult {
        self.compare_with(&CompareOptions::default())
    }

    pub fn compare_with(&self, options: &CompareOptions) -> DiffResult {
        DiffEngine::new()
            .compare(
                &self.ls(),
                &self.rs(),
                options,
                None,
                &AtomicBool::new(false),
            )
            .unwrap()
    }
}

/// Writes a file (creating its folders) and sets its modification time.
pub fn add_file(path: &Path, modified: SystemTime, content: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
    set_mtime(path, modified);
}

pub fn add(path: &Path, modified: SystemTime) {
    add_file(path, modified, b"data");
}

pub fn set_mtime(path: &Path, modified: SystemTime) {
    filetime::set_file_mtime(path, filetime::FileTime::from_system_time(modified)).unwrap();
}

pub fn mtime(path: &Path) -> SystemTime {
    fs::metadata(path).unwrap().modified().unwrap()
}

pub fn find<'a>(result: &'a DiffResult, rel: &str) -> &'a DiffNode {
    let rel = p(rel);
    let id = result
        .tree
        .find(&rel)
        .unwrap_or_else(|| panic!("node {rel} not found"));
    result.tree.node(id)
}

pub fn find_id(result: &DiffResult, rel: &str) -> NodeId {
    result.tree.find(&p(rel)).unwrap()
}

pub fn try_find<'a>(result: &'a DiffResult, rel: &str) -> Option<&'a DiffNode> {
    result.tree.find(&p(rel)).map(|id| result.tree.node(id))
}

pub fn root_children(result: &DiffResult) -> Vec<&DiffNode> {
    result
        .tree
        .root()
        .children
        .iter()
        .map(|&c| result.tree.node(c))
        .collect()
}

/// Creates a folder link (symlink on Unix; symlink or junction on Windows). False if the OS refuses.
pub fn make_dir_link(link: &Path, target: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_dir(target, link).is_ok() {
            return true;
        }
        std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Makes a file hidden: the hidden attribute on Windows; on Unix only dotfiles are hidden, so the test file
/// must be named `.something` there.
pub fn make_hidden(path: &Path) {
    #[cfg(windows)]
    {
        let ok = std::process::Command::new("attrib")
            .arg("+h")
            .arg(path)
            .status()
            .unwrap()
            .success();
        assert!(ok);
    }
    #[cfg(not(windows))]
    {
        assert!(path.file_name().unwrap().to_string_lossy().starts_with('.'));
    }
}

/// All paths below a folder with their size and modification time, sorted.
pub fn snapshot_tree(root: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).unwrap() {
            let e = e.unwrap();
            let m = fs::symlink_metadata(e.path()).unwrap();
            out.push((e.path(), m.len(), m.modified().unwrap()));
            if m.is_dir() {
                stack.push(e.path());
            }
        }
    }
    out.sort();
    out
}
