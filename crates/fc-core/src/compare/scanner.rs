//! Reads one directory tree into [`ScanEntry`] objects. Read-only: never modifies the file system.

use std::collections::{HashMap, HashSet};
use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::progress::ProgressTracker;
use super::{Cancelled, PathComparison, PathPatternMatcher, io_message};
use crate::model::{Attributes, CompareOptions, FileSnapshot, SymlinkMode, join_relative};

/// Folder depth at which the scan stops (protects against link loops the ancestor check can't see).
pub const MAX_DEPTH: usize = 512;

const LOOP_MESSAGE: &str = "Symbolic link loop detected - folder skipped.";
const MISSING_TARGET_MESSAGE: &str = "Link target does not exist.";

/// Raw scan result of one side, before merging.
#[derive(Debug)]
pub(crate) struct ScanEntry {
    pub name: String,
    pub snapshot: FileSnapshot,
    pub error: Option<String>,
    /// `Some` for folders whose contents were enumerated.
    pub children: Option<Children>,
}

impl ScanEntry {
    pub fn is_dir(&self) -> bool {
        self.snapshot.is_dir()
    }

    fn mark_error(&mut self, message: String) {
        self.error = Some(message);
        self.children = None;
    }
}

/// Children in enumeration order, with a lookup by name key ([`PathComparison::key`]).
#[derive(Debug, Default)]
pub(crate) struct Children {
    pub list: Vec<ScanEntry>,
    index: HashMap<String, usize>,
}

impl Children {
    /// Adds a child; a child whose name matches an earlier one replaces it but keeps its position.
    fn insert(&mut self, paths: PathComparison, entry: ScanEntry) {
        let key = paths.key(&entry.name).into_owned();
        match self.index.get(&key) {
            Some(&i) => self.list[i] = entry,
            None => {
                self.index.insert(key, self.list.len());
                self.list.push(entry);
            }
        }
    }
}

struct Context<'a, 't> {
    options: &'a CompareOptions,
    paths: PathComparison,
    matcher: PathPatternMatcher,
    tracker: Option<&'a ProgressTracker<'t>>,
    cancel: &'a AtomicBool,
}

impl Context<'_, '_> {
    fn check(&self) -> Result<(), Cancelled> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Scans a whole root folder.
pub(crate) fn scan_root(
    root: &Path,
    options: &CompareOptions,
    tracker: Option<&ProgressTracker>,
    cancel: &AtomicBool,
) -> Result<ScanEntry, Cancelled> {
    let ctx = Context {
        options,
        paths: options.paths(),
        matcher: PathPatternMatcher::new(&options.include_patterns, &options.exclude_patterns),
        tracker,
        cancel,
    };
    let meta = fs::metadata(root);
    let mut entry = ScanEntry {
        name: String::new(),
        snapshot: match &meta {
            Ok(m) => snapshot(m, root, "", 0, true),
            Err(_) => empty_snapshot(root, Attributes::DIRECTORY),
        },
        error: None,
        children: None,
    };
    let key = key_of(root);
    let mut ancestors = HashSet::from([ctx.paths.key(&key).into_owned()]);
    fill(&mut entry, root, root, &key, "", &ctx, &mut ancestors, 0)?;
    Ok(entry)
}

/// Scans a single relative path (file or folder subtree). Returns `None` if it does not exist. The filters only
/// apply to its children.
pub(crate) fn scan_path(
    root: &Path,
    relative_path: &str,
    options: &CompareOptions,
    cancel: &AtomicBool,
) -> Result<Option<ScanEntry>, Cancelled> {
    let full = root.join(relative_path);
    let name = full
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ctx = Context {
        options,
        paths: options.paths(),
        matcher: PathPatternMatcher::new(&options.include_patterns, &options.exclude_patterns),
        tracker: None,
        cancel,
    };
    let Ok(lmeta) = fs::symlink_metadata(&full) else {
        return Ok(None);
    };
    let is_link = lmeta.file_type().is_symlink();
    if is_link && options.symlink_mode == SymlinkMode::Skip {
        return Ok(None);
    }

    if full.is_dir() {
        let mut entry = ScanEntry {
            name: name.clone(),
            snapshot: snapshot(&lmeta, &full, &name, 0, true),
            error: None,
            children: None,
        };
        if options.recursive {
            let enum_path = if is_link {
                match resolve(&full) {
                    Ok(t) => t,
                    Err(msg) => {
                        entry.mark_error(msg);
                        return Ok(Some(entry));
                    }
                }
            } else {
                full.clone()
            };
            let key = key_of(&enum_path);
            let mut ancestors = HashSet::from([
                ctx.paths.key(&key_of(root)).into_owned(),
                ctx.paths.key(&key).into_owned(),
            ]);
            fill(
                &mut entry,
                &enum_path,
                &full,
                &key,
                relative_path,
                &ctx,
                &mut ancestors,
                0,
            )?;
        }
        return Ok(Some(entry));
    }

    if full.is_file() {
        return Ok(Some(
            match file_entry(&lmeta, is_link, &name, &full, options) {
                Ok(e) => e,
                Err(msg) => error_entry(&name, &full, Attributes(0), msg),
            },
        ));
    }
    Ok(None)
}

/// Current snapshot of a path, or `None` if it does not exist or can't be read.
pub(crate) fn stat(full_path: &Path) -> Option<FileSnapshot> {
    let meta = fs::metadata(full_path).ok()?;
    let name = full_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let size = if meta.is_dir() { 0 } else { meta.len() };
    Some(snapshot(&meta, full_path, &name, size, meta.is_dir()))
}

#[allow(clippy::too_many_arguments)]
fn fill(
    entry: &mut ScanEntry,
    enum_path: &Path,
    full_path: &Path,
    key: &str,
    rel: &str,
    ctx: &Context,
    ancestors: &mut HashSet<String>,
    depth: usize,
) -> Result<(), Cancelled> {
    ctx.check()?;
    if let Some(t) = ctx.tracker {
        t.folder(&full_path.to_string_lossy());
    }
    let mut children = Children::default();

    let reader = match fs::read_dir(enum_path) {
        Ok(r) => r,
        Err(e) => {
            entry.mark_error(io_message(&e));
            return Ok(());
        }
    };
    for item in reader {
        ctx.check()?;
        let item = match item {
            Ok(i) => i,
            Err(e) => {
                entry.mark_error(io_message(&e));
                return Ok(());
            }
        };
        let name = item.file_name().to_string_lossy().into_owned();
        if let Some(child) = scan_child(
            &item, name, enum_path, full_path, key, rel, ctx, ancestors, depth,
        )? {
            children.insert(ctx.paths, child);
        }
    }
    entry.children = Some(children);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn scan_child(
    item: &fs::DirEntry,
    name: String,
    enum_path: &Path,
    full_path: &Path,
    key: &str,
    rel: &str,
    ctx: &Context,
    ancestors: &mut HashSet<String>,
    depth: usize,
) -> Result<Option<ScanEntry>, Cancelled> {
    let child_rel = join_relative(rel, &name);
    let child_full = full_path.join(item.file_name());
    let child_enum = enum_path.join(item.file_name());

    let meta = match item.metadata() {
        Ok(m) => m,
        Err(e) => {
            if let Some(t) = ctx.tracker {
                t.item();
            }
            return Ok(Some(error_entry(
                &name,
                &child_full,
                Attributes(0),
                io_message(&e),
            )));
        }
    };
    let is_link = meta.file_type().is_symlink();
    let is_dir = entry_is_dir(&meta, &child_enum);
    let attrs = attributes(&meta, &name, is_dir);

    if ctx.options.ignore_hidden_and_system && attrs.has(Attributes::HIDDEN | Attributes::SYSTEM) {
        return Ok(None);
    }
    if ctx.matcher.is_excluded(&name, &child_rel) {
        return Ok(None);
    }
    if !is_dir && !ctx.matcher.is_included_file(&name, &child_rel) {
        return Ok(None);
    }
    if is_link && ctx.options.symlink_mode == SymlinkMode::Skip {
        return Ok(None);
    }

    if let Some(t) = ctx.tracker {
        t.item();
    }

    if !is_dir {
        return Ok(Some(
            match file_entry(&meta, is_link, &name, &child_full, ctx.options) {
                Ok(e) => e,
                Err(msg) => error_entry(&name, &child_full, attrs, msg),
            },
        ));
    }

    let mut entry = ScanEntry {
        name: name.clone(),
        snapshot: snapshot(&meta, &child_full, &name, 0, true),
        error: None,
        children: None,
    };
    if !ctx.options.recursive {
        return Ok(Some(entry));
    }

    let (next_enum, next_key) = if is_link {
        match resolve(&child_enum) {
            Ok(target) => {
                let k = key_string(&target);
                (target, k)
            }
            Err(msg) => {
                entry.mark_error(msg);
                return Ok(Some(entry));
            }
        }
    } else {
        (child_enum, join_key(key, &name))
    };

    let lookup = ctx.paths.key(&next_key).into_owned();
    if depth >= MAX_DEPTH || ancestors.contains(&lookup) {
        entry.mark_error(LOOP_MESSAGE.to_string());
        return Ok(Some(entry));
    }

    ancestors.insert(lookup.clone());
    let result = fill(
        &mut entry,
        &next_enum,
        &child_full,
        &next_key,
        &child_rel,
        ctx,
        ancestors,
        depth + 1,
    );
    ancestors.remove(&lookup);
    result?;
    Ok(Some(entry))
}

fn file_entry(
    meta: &Metadata,
    is_link: bool,
    name: &str,
    full_path: &Path,
    options: &CompareOptions,
) -> Result<ScanEntry, String> {
    let own = snapshot(meta, full_path, name, meta.len(), false);
    let snapshot = if is_link && options.symlink_mode == SymlinkMode::Follow {
        let target = fs::metadata(full_path).map_err(|e| io_message(&e))?;
        FileSnapshot {
            size: target.len(),
            modified: target.modified().unwrap_or(UNIX_EPOCH),
            ..own
        }
    } else {
        own
    };
    Ok(ScanEntry {
        name: name.to_string(),
        snapshot,
        error: None,
        children: None,
    })
}

fn error_entry(name: &str, full_path: &Path, attrs: Attributes, message: String) -> ScanEntry {
    ScanEntry {
        name: name.to_string(),
        snapshot: empty_snapshot(full_path, attrs.0 & Attributes::KEPT),
        error: Some(message),
        children: None,
    }
}

fn empty_snapshot(full_path: &Path, attrs: u32) -> FileSnapshot {
    FileSnapshot {
        size: 0,
        modified: UNIX_EPOCH,
        attributes: Attributes(attrs),
        unix_mode: None,
        full_path: full_path.to_path_buf(),
    }
}

/// Resolves a link to its final target; errors carry the message for the node.
fn resolve(link: &Path) -> Result<PathBuf, String> {
    match fs::canonicalize(link) {
        Ok(target) if target.exists() => Ok(simplify(target)),
        Ok(_) => Err(MISSING_TARGET_MESSAGE.to_string()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(MISSING_TARGET_MESSAGE.to_string()),
        Err(e) => Err(io_message(&e)),
    }
}

/// Key of a folder for the loop check: the canonical path (links resolved), or the absolute path if that fails.
fn key_of(path: &Path) -> String {
    match fs::canonicalize(path) {
        Ok(p) => key_string(&simplify(p)),
        Err(_) => key_string(&std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())),
    }
}

fn key_string(path: &Path) -> String {
    let s = path.to_string_lossy();
    let trimmed = s.trim_end_matches(std::path::MAIN_SEPARATOR);
    if trimmed.is_empty() {
        s.into_owned()
    } else {
        trimmed.to_string()
    }
}

fn join_key(key: &str, name: &str) -> String {
    format!("{key}{}{name}", std::path::MAIN_SEPARATOR)
}

/// Removes the `\\?\` prefix that `canonicalize` adds on Windows, so keys and paths stay comparable.
fn simplify(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = s.strip_prefix(r"\\?\")
            && rest.len() >= 2
            && rest.as_bytes()[1] == b':'
        {
            return PathBuf::from(rest);
        }
    }
    path
}

fn entry_is_dir(meta: &Metadata, path: &Path) -> bool {
    if !meta.file_type().is_symlink() {
        return meta.is_dir();
    }
    #[cfg(windows)]
    {
        let _ = path;
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & Attributes::DIRECTORY != 0
    }
    #[cfg(not(windows))]
    {
        fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
    }
}

#[cfg(windows)]
fn attributes(meta: &Metadata, _name: &str, _is_dir: bool) -> Attributes {
    use std::os::windows::fs::MetadataExt;
    Attributes(meta.file_attributes() & Attributes::KEPT)
}

/// On Unix: dotfiles count as hidden, a missing owner write bit as read-only (like .NET does there).
#[cfg(not(windows))]
fn attributes(meta: &Metadata, name: &str, is_dir: bool) -> Attributes {
    use std::os::unix::fs::MetadataExt;
    let mut a = 0;
    if is_dir {
        a |= Attributes::DIRECTORY;
    }
    if name.starts_with('.') && name != "." && name != ".." {
        a |= Attributes::HIDDEN;
    }
    if meta.mode() & 0o200 == 0 {
        a |= Attributes::READONLY;
    }
    if meta.file_type().is_symlink() {
        a |= Attributes::REPARSE_POINT;
    }
    Attributes(a)
}

fn unix_mode(meta: &Metadata) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(meta.mode() & 0o7777)
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        None
    }
}

fn snapshot(
    meta: &Metadata,
    full_path: &Path,
    name: &str,
    size: u64,
    is_dir: bool,
) -> FileSnapshot {
    FileSnapshot {
        size,
        modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        attributes: attributes(meta, name, is_dir),
        unix_mode: unix_mode(meta),
        full_path: full_path.to_path_buf(),
    }
}
