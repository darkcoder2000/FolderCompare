use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;

use super::progress::{ProgressFn, ProgressTracker};
use super::scanner::{self, ScanEntry};
use super::{
    ByteContentComparer, Cancelled, ContentComparer, FileComparison, PathComparison, compare_files,
    needs_content_check,
};
use crate::model::{
    CompareOptions, DiffNode, DiffStatus, DiffTree, FileSnapshot, NodeId, Side, join_relative,
};
use crate::ops;

/// Content comparisons run in parallel; kept low because both trees may share one physical disk.
const CONTENT_PARALLELISM: usize = 2;
/// Scanner threads recurse once per folder level (up to [`super::MAX_DEPTH`]).
const SCAN_STACK_SIZE: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareError {
    /// The roots can't be compared; the message is shown to the user.
    InvalidRoots(String),
    Cancelled,
}

impl std::fmt::Display for CompareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompareError::InvalidRoots(m) => f.write_str(m),
            CompareError::Cancelled => f.write_str("The operation was cancelled."),
        }
    }
}

impl std::error::Error for CompareError {}

impl From<Cancelled> for CompareError {
    fn from(_: Cancelled) -> Self {
        CompareError::Cancelled
    }
}

/// A finished comparison: normalized roots, the options used (with the resolved path comparison) and the tree.
#[derive(Debug, Clone)]
pub struct DiffResult {
    pub left_root: PathBuf,
    pub right_root: PathBuf,
    pub options: CompareOptions,
    pub tree: DiffTree,
}

impl DiffResult {
    pub fn root_for(&self, side: Side) -> &Path {
        match side {
            Side::Left => &self.left_root,
            Side::Right => &self.right_root,
        }
    }

    /// The resolved name comparison of this result.
    pub fn paths(&self) -> PathComparison {
        self.options.paths()
    }

    /// Collects what [`DiffEngine::prepare_refresh`] needs to rescan the child `name` of `parent`.
    pub fn refresh_request(&self, parent: NodeId, name: &str) -> RefreshRequest {
        let mut ancestors = Vec::new();
        let mut a = Some(parent);
        while let Some(id) = a {
            let node = self.tree.node(id);
            ancestors.push((id, node.relative_path.clone()));
            a = node.parent;
        }
        RefreshRequest {
            left_root: self.left_root.clone(),
            right_root: self.right_root.clone(),
            options: self.options.clone(),
            parent,
            name: name.to_string(),
            relative_path: join_relative(&self.tree.node(parent).relative_path, name),
            ancestors,
        }
    }

    /// Applies a prepared refresh: replaces, appends or removes the child, then updates the ancestors' snapshots
    /// and roll-ups.
    pub fn apply_refresh(&mut self, data: RefreshData) {
        let paths = self.paths();
        let tree = &mut self.tree;
        let index = tree
            .node(data.parent)
            .children
            .iter()
            .position(|&c| paths.eq(&tree.node(c).name, &data.name));
        match data.fresh {
            None => {
                if let Some(i) = index {
                    let old = tree.node_mut(data.parent).children.remove(i);
                    tree.kill(old);
                }
            }
            Some(sub) => {
                let id = tree.graft(sub);
                tree.node_mut(id).parent = Some(data.parent);
                match index {
                    Some(i) => {
                        let old =
                            std::mem::replace(&mut tree.node_mut(data.parent).children[i], id);
                        tree.kill(old);
                    }
                    None => tree.node_mut(data.parent).children.push(id),
                }
            }
        }
        for (id, left, right) in data.ancestors {
            if tree.node(id).parent.is_some() {
                let node = tree.node_mut(id);
                node.left = left;
                node.right = right;
            }
            roll_up(tree, id);
        }
    }
}

/// Everything needed to rescan one child off the UI thread.
#[derive(Debug, Clone)]
pub struct RefreshRequest {
    left_root: PathBuf,
    right_root: PathBuf,
    options: CompareOptions,
    parent: NodeId,
    name: String,
    relative_path: String,
    ancestors: Vec<(NodeId, String)>,
}

impl RefreshRequest {
    /// The same request for another child of the same parent (e.g. a "keep both" copy).
    pub fn with_name(&self, name: &str) -> RefreshRequest {
        let parent_rel = &self.ancestors[0].1;
        RefreshRequest {
            name: name.to_string(),
            relative_path: join_relative(parent_rel, name),
            ..self.clone()
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn parent(&self) -> NodeId {
        self.parent
    }
}

/// Data gathered off the UI thread for refreshing one child of a folder node.
#[derive(Debug)]
pub struct RefreshData {
    pub parent: NodeId,
    pub name: String,
    /// The rescanned subtree (its root is the child), or `None` if the path no longer exists on either side.
    fresh: Option<DiffTree>,
    ancestors: Vec<(NodeId, Option<FileSnapshot>, Option<FileSnapshot>)>,
}

impl RefreshData {
    pub fn fresh(&self) -> Option<&DiffNode> {
        self.fresh.as_ref().map(|t| t.root())
    }
}

/// Scans both trees and merges them into a [`DiffTree`]. Never modifies the file system.
pub struct DiffEngine {
    content: Box<dyn ContentComparer + Send>,
}

impl Default for DiffEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DiffEngine {
    pub fn new() -> Self {
        Self::with_content(Box::new(ByteContentComparer))
    }

    pub fn with_content(content: Box<dyn ContentComparer + Send>) -> Self {
        Self { content }
    }

    pub fn compare(
        &self,
        left_root: &str,
        right_root: &str,
        options: &CompareOptions,
        progress: Option<ProgressFn>,
        cancel: &AtomicBool,
    ) -> Result<DiffResult, CompareError> {
        if let Some(error) = ops::validate_roots(left_root, right_root) {
            return Err(CompareError::InvalidRoots(error));
        }
        let left_root = ops::normalize(left_root).map_err(CompareError::InvalidRoots)?;
        let right_root = ops::normalize(right_root).map_err(CompareError::InvalidRoots)?;
        let mut options = options.clone();
        if options.path_comparison.is_none() {
            options.path_comparison = Some(PathComparison::for_roots(&[&left_root, &right_root]));
        }
        let tracker = ProgressTracker::new(progress);

        let (left, right) = thread::scope(|s| {
            let scan = |root: &Path| {
                let (options, tracker) = (&options, &tracker);
                let root = root.to_path_buf();
                thread::Builder::new()
                    .name("scan".into())
                    .stack_size(SCAN_STACK_SIZE)
                    .spawn_scoped(s, move || {
                        scanner::scan_root(&root, options, Some(tracker), cancel)
                    })
                    .expect("spawn scanner thread")
            };
            let l = scan(&left_root);
            let r = scan(&right_root);
            (
                l.join().expect("scanner thread panicked"),
                r.join().expect("scanner thread panicked"),
            )
        });
        let (left, right) = (left?, right?);

        let mut tree = DiffTree::empty();
        let mut pending = Vec::new();
        merge(
            &mut tree,
            String::new(),
            String::new(),
            Some(left),
            Some(right),
            &options,
            Some(&mut pending),
        );

        if !pending.is_empty() {
            let total = pending
                .iter()
                .map(|&id| tree.node(id).left.as_ref().map_or(0, |s| s.size))
                .sum();
            tracker.begin_content(total);
            let results = self.verify_parallel(&tree, &pending, &options, &tracker, cancel)?;
            for (id, r) in results {
                let node = tree.node_mut(id);
                node.status = r.status;
                node.error = r.error;
            }
            roll_up_tree(&mut tree, NodeId::ROOT);
        }

        tracker.report_final();
        Ok(DiffResult {
            left_root,
            right_root,
            options,
            tree,
        })
    }

    fn verify_parallel(
        &self,
        tree: &DiffTree,
        pending: &[NodeId],
        options: &CompareOptions,
        tracker: &ProgressTracker,
        cancel: &AtomicBool,
    ) -> Result<Vec<(NodeId, FileComparison)>, Cancelled> {
        let next = AtomicUsize::new(0);
        let results = Mutex::new(Vec::with_capacity(pending.len()));
        let failed = AtomicBool::new(false);
        let content: &dyn ContentComparer = self.content.as_ref();
        thread::scope(|s| {
            for _ in 0..CONTENT_PARALLELISM.min(pending.len()) {
                s.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= pending.len() || failed.load(Ordering::Relaxed) {
                            break;
                        }
                        let node = tree.node(pending[i]);
                        let (Some(l), Some(r)) = (&node.left, &node.right) else {
                            continue;
                        };
                        tracker.content_file(&node.relative_path);
                        let mut done = 0u64;
                        let result =
                            compare_files(l, r, options, Some(content), cancel, &mut |n| {
                                done += n;
                                tracker.content_bytes(n);
                            });
                        match result {
                            Ok(r) => {
                                // Files that differ stop early; count their remaining bytes anyway.
                                tracker.content_bytes(l.size.saturating_sub(done));
                                results.lock().unwrap().push((pending[i], r));
                            }
                            Err(Cancelled) => {
                                failed.store(true, Ordering::Relaxed);
                                break;
                            }
                        }
                    }
                });
            }
        });
        if failed.load(Ordering::Relaxed) || cancel.load(Ordering::Relaxed) {
            return Err(Cancelled);
        }
        Ok(results.into_inner().unwrap())
    }

    /// Rescans one child on both sides (I/O, call off the UI thread).
    pub fn prepare_refresh(
        &self,
        req: &RefreshRequest,
        cancel: &AtomicBool,
    ) -> Result<RefreshData, Cancelled> {
        let left = scanner::scan_path(&req.left_root, &req.relative_path, &req.options, cancel)?;
        let right = scanner::scan_path(&req.right_root, &req.relative_path, &req.options, cancel)?;
        let fresh = if left.is_none() && right.is_none() {
            None
        } else {
            let name = left
                .as_ref()
                .or(right.as_ref())
                .map(|e| e.name.clone())
                .unwrap_or_default();
            let mut sub = DiffTree::empty();
            let mut pending = Vec::new();
            merge(
                &mut sub,
                name,
                req.relative_path.clone(),
                left,
                right,
                &req.options,
                Some(&mut pending),
            );
            if !pending.is_empty() {
                for id in pending {
                    let node = sub.node(id);
                    let (Some(l), Some(r)) = (&node.left, &node.right) else {
                        continue;
                    };
                    let result = compare_files(
                        l,
                        r,
                        &req.options,
                        Some(self.content.as_ref()),
                        cancel,
                        &mut |_| {},
                    )?;
                    let node = sub.node_mut(id);
                    node.status = result.status;
                    node.error = result.error;
                }
                roll_up_tree(&mut sub, NodeId::ROOT);
            }
            Some(sub)
        };

        let ancestors = req
            .ancestors
            .iter()
            .map(|(id, rel)| {
                (
                    *id,
                    scanner::stat(&req.left_root.join(rel)),
                    scanner::stat(&req.right_root.join(rel)),
                )
            })
            .collect();
        Ok(RefreshData {
            parent: req.parent,
            name: req.name.clone(),
            fresh,
            ancestors,
        })
    }

    /// Convenience: rescans the child `name` of `parent` and applies the result.
    pub fn refresh(
        &self,
        result: &mut DiffResult,
        parent: NodeId,
        name: &str,
    ) -> Result<(), Cancelled> {
        let req = result.refresh_request(parent, name);
        let data = self.prepare_refresh(&req, &AtomicBool::new(false))?;
        result.apply_refresh(data);
        Ok(())
    }
}

/// Merges two scanned entries into `tree` and returns the new node. File pairs get their size/timestamp status;
/// pairs that also need a content check are added to `pending` (when `None`, content is never checked).
fn merge(
    tree: &mut DiffTree,
    name: String,
    rel: String,
    left: Option<ScanEntry>,
    right: Option<ScanEntry>,
    options: &CompareOptions,
    mut pending: Option<&mut Vec<NodeId>>,
) -> NodeId {
    let left_dir = left.as_ref().is_some_and(|e| e.is_dir());
    let right_dir = right.as_ref().is_some_and(|e| e.is_dir());

    if let (Some(l), Some(r)) = (&left, &right)
        && left_dir != right_dir
    {
        let mut node = DiffNode::new(rel, name, false);
        node.status = DiffStatus::TypeConflict;
        node.error = Some(
            if left_dir {
                "Folder on the left, file on the right."
            } else {
                "File on the left, folder on the right."
            }
            .to_string(),
        );
        node.left_total = l.snapshot.size;
        node.right_total = r.snapshot.size;
        node.left = Some(l.snapshot.clone());
        node.right = Some(r.snapshot.clone());
        return tree.push(node);
    }

    let is_dir = left_dir || right_dir;
    let mut node = DiffNode::new(rel.clone(), name, is_dir);
    node.left = left.as_ref().map(|e| e.snapshot.clone());
    node.right = right.as_ref().map(|e| e.snapshot.clone());

    let error = match (&left, &right) {
        (Some(ScanEntry { error: Some(e), .. }), _) => Some(format!("Left: {e}")),
        (_, Some(ScanEntry { error: Some(e), .. })) => Some(format!("Right: {e}")),
        _ => None,
    };
    if error.is_some() {
        node.status = DiffStatus::Error;
        node.error = error;
        return tree.push(node);
    }

    if !is_dir {
        node.left_total = left.as_ref().map_or(0, |e| e.snapshot.size);
        node.right_total = right.as_ref().map_or(0, |e| e.snapshot.size);
        let mut check = false;
        node.status = match (&left, &right) {
            (None, _) => DiffStatus::OnlyRight,
            (_, None) => DiffStatus::OnlyLeft,
            (Some(l), Some(r)) => {
                check = needs_content_check(&l.snapshot, &r.snapshot, options);
                compare_files(
                    &l.snapshot,
                    &r.snapshot,
                    options,
                    None,
                    &AtomicBool::new(false),
                    &mut |_| {},
                )
                .map(|c| c.status)
                .unwrap_or(DiffStatus::Error)
            }
        };
        let id = tree.push(node);
        if check && let Some(p) = pending.as_deref_mut() {
            p.push(id);
        }
        return id;
    }

    let id = tree.push(node);
    let paths = options.paths();
    let left_children = left.and_then(|e| e.children).map(|c| c.list);
    let mut right_children: Vec<Option<ScanEntry>> = Vec::new();
    let mut right_index = std::collections::HashMap::new();
    if let Some(c) = right.and_then(|e| e.children) {
        for (i, e) in c.list.into_iter().enumerate() {
            right_index.insert(paths.key(&e.name).into_owned(), i);
            right_children.push(Some(e));
        }
    }

    for l in left_children.into_iter().flatten() {
        let r = right_index
            .get(paths.key(&l.name).as_ref())
            .and_then(|&i| right_children[i].take());
        let child_name = l.name.clone();
        let child = merge(
            tree,
            child_name.clone(),
            join_relative(&rel, &child_name),
            Some(l),
            r,
            options,
            pending.as_deref_mut(),
        );
        tree.add_child(id, child);
    }
    for r in right_children.into_iter().flatten() {
        let child_name = r.name.clone();
        let child = merge(
            tree,
            child_name.clone(),
            join_relative(&rel, &child_name),
            None,
            Some(r),
            options,
            pending.as_deref_mut(),
        );
        tree.add_child(id, child);
    }

    roll_up(tree, id);
    id
}

/// Recomputes a folder's sizes and rolled-up status from its children.
pub fn roll_up(tree: &mut DiffTree, id: NodeId) {
    let node = tree.node(id);
    if !node.is_dir {
        return;
    }
    let (mut left, mut right, mut all_identical) = (0u64, 0u64, true);
    for &c in &node.children {
        let child = tree.node(c);
        left += child.left_total;
        right += child.right_total;
        if !matches!(
            child.status,
            DiffStatus::Identical | DiffStatus::SameContentDifferentTime
        ) {
            all_identical = false;
        }
    }
    let node = tree.node_mut(id);
    node.left_total = if node.left.is_none() { 0 } else { left };
    node.right_total = if node.right.is_none() { 0 } else { right };
    node.status = if node.error.is_some() {
        DiffStatus::Error
    } else if node.left.is_none() {
        DiffStatus::OnlyRight
    } else if node.right.is_none() {
        DiffStatus::OnlyLeft
    } else if all_identical {
        DiffStatus::Identical
    } else {
        DiffStatus::Differs
    };
}

/// Recomputes the roll-up of `id` and every folder below it, children first.
pub fn roll_up_tree(tree: &mut DiffTree, id: NodeId) {
    let node = tree.node(id);
    if !node.is_dir || node.status == DiffStatus::TypeConflict {
        return;
    }
    let children = node.children.clone();
    for c in children {
        roll_up_tree(tree, c);
    }
    roll_up(tree, id);
}
