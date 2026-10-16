//! Data model of a comparison: options, statuses, file snapshots and the diff tree (an arena of nodes).

use std::path::{MAIN_SEPARATOR, PathBuf};
use std::time::{Duration, SystemTime};

use crate::compare::PathComparison;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SymlinkMode {
    /// Links are dropped from the comparison completely.
    #[default]
    Skip,
    /// Links are entered (folders) or compared by their target (files).
    Follow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ContentCompareMode {
    /// Size and timestamp only (default, fast).
    #[default]
    Off,
    /// Verify the content of files that look identical by timestamp and size.
    WhenTimestampsEqual,
    /// Also verify files whose timestamp differs but whose size matches.
    WhenSizesEqual,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompareOptions {
    /// When false, subfolders appear but get no children.
    pub recursive: bool,
    /// Timestamps within this distance (inclusive) count as equal.
    pub tolerance: Duration,
    /// When set, a difference of exactly one hour (± tolerance) counts as identical.
    pub ignore_dst_offset: bool,
    pub symlink_mode: SymlinkMode,
    /// Semicolon-separated glob patterns; when non-empty only matching files are compared.
    pub include_patterns: String,
    /// Semicolon-separated glob patterns for files and folders to skip.
    pub exclude_patterns: String,
    /// Hidden and system entries (dotfiles on Unix) are skipped.
    pub ignore_hidden_and_system: bool,
    pub content_compare: ContentCompareMode,
    /// How names are matched. `None` = decided per compare from the OS and the roots
    /// ([`PathComparison::for_roots`]); the result's options always carry the resolved value.
    pub path_comparison: Option<PathComparison>,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            tolerance: Duration::from_secs(2),
            ignore_dst_offset: false,
            symlink_mode: SymlinkMode::Skip,
            include_patterns: String::new(),
            exclude_patterns: String::new(),
            ignore_hidden_and_system: false,
            content_compare: ContentCompareMode::Off,
            path_comparison: None,
        }
    }
}

impl CompareOptions {
    /// The effective name comparison.
    pub fn paths(&self) -> PathComparison {
        self.path_comparison
            .unwrap_or_else(PathComparison::for_current_os)
    }
}

/// Status of one node. The variant order matters: sorting by status uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum DiffStatus {
    #[default]
    Identical,
    OnlyLeft,
    OnlyRight,
    NewerLeft,
    NewerRight,
    /// Rolled-up status of a folder whose contents are not all identical.
    Differs,
    TypeConflict,
    Error,
    /// Same timestamp (within tolerance) but size or content differs. Direction is unknown.
    SameTimeDifferentContent,
    /// Content verified identical, only the timestamp differs.
    SameContentDifferentTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

/// File attributes kept in a snapshot (the values are the Windows `FILE_ATTRIBUTE_*` bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Attributes(pub u32);

impl Attributes {
    pub const READONLY: u32 = 0x1;
    pub const HIDDEN: u32 = 0x2;
    pub const SYSTEM: u32 = 0x4;
    pub const DIRECTORY: u32 = 0x10;
    pub const ARCHIVE: u32 = 0x20;
    pub const REPARSE_POINT: u32 = 0x400;
    /// The bits a snapshot keeps.
    pub const KEPT: u32 = Self::DIRECTORY
        | Self::READONLY
        | Self::HIDDEN
        | Self::SYSTEM
        | Self::ARCHIVE
        | Self::REPARSE_POINT;

    pub fn has(self, bits: u32) -> bool {
        self.0 & bits != 0
    }
}

/// What the scan saw of one side of a node.
#[derive(Debug, Clone, PartialEq)]
pub struct FileSnapshot {
    /// File size; 0 for folders.
    pub size: u64,
    pub modified: SystemTime,
    pub attributes: Attributes,
    /// Unix permission bits (`st_mode & 0o7777`); `None` on Windows.
    pub unix_mode: Option<u32>,
    pub full_path: PathBuf,
}

impl FileSnapshot {
    pub fn is_dir(&self) -> bool {
        self.attributes.has(Attributes::DIRECTORY)
    }
}

/// Scan progress. While file contents are being verified, `bytes_total` is greater than zero and
/// `current_folder` holds the file being compared.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScanProgress {
    /// Items of both sides combined.
    pub items_scanned: u64,
    pub current_folder: String,
    pub bytes_compared: u64,
    pub bytes_total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const ROOT: NodeId = NodeId(0);

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// One relative path in the comparison tree.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffNode {
    /// Path below the roots, using the OS separator; "" for the root.
    pub relative_path: String,
    pub name: String,
    pub is_dir: bool,
    pub left: Option<FileSnapshot>,
    pub right: Option<FileSnapshot>,
    pub status: DiffStatus,
    pub error: Option<String>,
    pub children: Vec<NodeId>,
    pub parent: Option<NodeId>,
    /// File size, or the sum of all file sizes below a folder, on the left side.
    pub left_total: u64,
    pub right_total: u64,
    /// False once the node was replaced or removed by a refresh.
    pub alive: bool,
}

impl DiffNode {
    pub fn new(relative_path: String, name: String, is_dir: bool) -> Self {
        Self {
            relative_path,
            name,
            is_dir,
            left: None,
            right: None,
            status: DiffStatus::Identical,
            error: None,
            children: Vec::new(),
            parent: None,
            left_total: 0,
            right_total: 0,
            alive: true,
        }
    }

    pub fn get(&self, side: Side) -> Option<&FileSnapshot> {
        match side {
            Side::Left => self.left.as_ref(),
            Side::Right => self.right.as_ref(),
        }
    }

    pub fn total(&self, side: Side) -> u64 {
        match side {
            Side::Left => self.left_total,
            Side::Right => self.right_total,
        }
    }

    pub fn exists(&self, side: Side) -> bool {
        self.get(side).is_some()
    }
}

/// Joins a relative path and a name with the OS separator.
pub fn join_relative(rel: &str, name: &str) -> String {
    if rel.is_empty() {
        name.to_string()
    } else {
        format!("{rel}{MAIN_SEPARATOR}{name}")
    }
}

/// The comparison tree. Nodes live in an arena; [`NodeId::ROOT`] is the root. A refresh appends new nodes and
/// marks replaced ones as not alive, so IDs held by the UI never point to a different node.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffTree {
    nodes: Vec<DiffNode>,
}

impl DiffTree {
    pub fn new(root: DiffNode) -> Self {
        Self { nodes: vec![root] }
    }

    /// A tree without nodes; the first node pushed becomes the root.
    pub(crate) fn empty() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn root(&self) -> &DiffNode {
        &self.nodes[0]
    }

    pub fn node(&self, id: NodeId) -> &DiffNode {
        &self.nodes[id.index()]
    }

    pub fn node_mut(&mut self, id: NodeId) -> &mut DiffNode {
        &mut self.nodes[id.index()]
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Adds a detached node and returns its ID.
    pub fn push(&mut self, node: DiffNode) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(node);
        id
    }

    pub fn add_child(&mut self, parent: NodeId, child: NodeId) {
        self.nodes[child.index()].parent = Some(parent);
        self.nodes[parent.index()].children.push(child);
    }

    /// All nodes below `id` in pre-order (not including `id`).
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.node(id).children.iter().rev().copied().collect();
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.node(n).children.iter().rev());
        }
        out
    }

    /// True when `id` lies below `ancestor`.
    pub fn is_descendant_of(&self, id: NodeId, ancestor: NodeId) -> bool {
        let mut p = self.node(id).parent;
        while let Some(a) = p {
            if a == ancestor {
                return true;
            }
            p = self.node(a).parent;
        }
        false
    }

    /// Depth below the root (children of the root have level 0).
    pub fn level(&self, id: NodeId) -> usize {
        let mut level = 0;
        let mut p = self.node(id).parent;
        while let Some(a) = p {
            if a == NodeId::ROOT {
                break;
            }
            level += 1;
            p = self.node(a).parent;
        }
        level
    }

    /// Finds a live node by relative path (exact match).
    pub fn find(&self, relative_path: &str) -> Option<NodeId> {
        self.descendants(NodeId::ROOT)
            .into_iter()
            .find(|&n| self.node(n).relative_path == relative_path)
    }

    /// Copies the root of `sub` and everything below it into this tree; returns the ID of the copied root.
    pub(crate) fn graft(&mut self, sub: DiffTree) -> NodeId {
        let offset = self.nodes.len() as u32;
        for mut n in sub.nodes {
            n.parent = n.parent.map(|p| NodeId(p.0 + offset));
            for c in &mut n.children {
                c.0 += offset;
            }
            self.nodes.push(n);
        }
        NodeId(offset)
    }

    /// Marks `id` and its subtree as no longer part of the tree.
    pub(crate) fn kill(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            let node = &mut self.nodes[n.index()];
            node.alive = false;
            stack.extend(node.children.iter().copied());
        }
    }
}
