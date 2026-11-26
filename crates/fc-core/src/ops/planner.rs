//! Turns a selection of nodes into a flat list of actions for preview and execution.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::compare::{DiffResult, is_newer};
use crate::model::{DiffStatus, DiffTree, FileSnapshot, NodeId, Side};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CopyDirection {
    LeftToRight,
    RightToLeft,
}

impl CopyDirection {
    pub fn source(self) -> Side {
        match self {
            CopyDirection::LeftToRight => Side::Left,
            CopyDirection::RightToLeft => Side::Right,
        }
    }

    pub fn target(self) -> Side {
        self.source().other()
    }
}

/// Which sides a delete applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sides {
    Left,
    Right,
    Both,
}

impl Sides {
    pub fn contains(self, side: Side) -> bool {
        matches!(
            (self, side),
            (Sides::Both, _) | (Sides::Left, Side::Left) | (Sides::Right, Side::Right)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionKind {
    CopyFile,
    CreateDirectory,
    Delete,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedAction {
    pub kind: ActionKind,
    pub node: NodeId,
    /// Relative path of the node (for messages and refresh).
    pub relative_path: String,
    /// The side that is written to (copy destination or delete side).
    pub target_side: Side,
    pub source_path: Option<PathBuf>,
    pub target_path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub source_time: Option<SystemTime>,
    pub target_time: Option<SystemTime>,
    pub target_exists: bool,
    /// The existing destination is newer than the source (overwriting it may be a mistake).
    pub target_is_newer: bool,
    /// Both files have the same timestamp but different content, so neither side can be called newer.
    /// Overwriting always needs an explicit choice.
    pub direction_unknown: bool,
}

impl PlannedAction {
    pub fn description(&self) -> String {
        let target = self.target_path.display();
        match self.kind {
            ActionKind::CopyFile => format!(
                "Copy {} -> {target}",
                self.source_path
                    .as_deref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            ),
            ActionKind::CreateDirectory => format!("Create folder {target}"),
            ActionKind::Delete => format!("Delete {target}"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OperationPlan {
    pub actions: Vec<PlannedAction>,
    pub notes: Vec<String>,
}

impl OperationPlan {
    pub fn total_bytes(&self) -> u64 {
        self.actions.iter().map(|a| a.size).sum()
    }
}

/// Removes duplicates and nodes that are already covered by a selected ancestor, keeping the original order.
pub fn top_level_only(tree: &DiffTree, nodes: &[NodeId]) -> Vec<NodeId> {
    let mut seen = HashSet::new();
    let list: Vec<NodeId> = nodes.iter().copied().filter(|n| seen.insert(*n)).collect();
    list.iter()
        .copied()
        .filter(|&n| {
            let mut p = tree.node(n).parent;
            while let Some(a) = p {
                if seen.contains(&a) {
                    return false;
                }
                p = tree.node(a).parent;
            }
            true
        })
        .collect()
}

pub fn plan_copy(result: &DiffResult, nodes: &[NodeId], direction: CopyDirection) -> OperationPlan {
    let mut plan = OperationPlan::default();
    for node in top_level_only(&result.tree, nodes) {
        add_copy(result, node, direction, &mut plan, true);
    }
    plan
}

pub fn plan_delete(result: &DiffResult, nodes: &[NodeId], sides: Sides) -> OperationPlan {
    let mut plan = OperationPlan::default();
    for id in top_level_only(&result.tree, nodes) {
        let node = result.tree.node(id);
        let mut any = false;
        for side in [Side::Left, Side::Right] {
            if !sides.contains(side) {
                continue;
            }
            let Some(snap) = node.get(side) else { continue };
            any = true;
            plan.actions.push(PlannedAction {
                kind: ActionKind::Delete,
                node: id,
                relative_path: node.relative_path.clone(),
                target_side: side,
                source_path: None,
                target_path: target_path(result, node.get(side), side, &node.relative_path),
                is_dir: snap.is_dir(),
                size: node.total(side),
                source_time: None,
                target_time: Some(snap.modified),
                target_exists: true,
                target_is_newer: false,
                direction_unknown: false,
            });
        }
        if !any {
            plan.notes.push(format!(
                "{}: does not exist on the chosen side.",
                node.relative_path
            ));
        }
    }
    plan
}

/// The path of a node on one side: the scanned path when it exists there (exact OS name), otherwise built from the
/// root and the scanned path of the other side, so names that are not valid UTF-8 survive.
fn target_path(
    result: &DiffResult,
    existing: Option<&FileSnapshot>,
    side: Side,
    relative_path: &str,
) -> PathBuf {
    if let Some(s) = existing {
        return s.full_path.clone();
    }
    result.root_for(side).join(relative_path)
}

fn mirrored(result: &DiffResult, source: &FileSnapshot, source_side: Side, rel: &str) -> PathBuf {
    let target_root = result.root_for(source_side.other());
    match source.full_path.strip_prefix(result.root_for(source_side)) {
        Ok(tail) if tail != Path::new("") => target_root.join(tail),
        _ => target_root.join(rel),
    }
}

fn add_copy(
    result: &DiffResult,
    id: NodeId,
    direction: CopyDirection,
    plan: &mut OperationPlan,
    top_level: bool,
) {
    let node = result.tree.node(id);
    let (source, target) = (direction.source(), direction.target());
    let rel = &node.relative_path;
    let Some(src) = node.get(source) else {
        if top_level {
            plan.notes
                .push(format!("{rel}: does not exist on the source side."));
        }
        return;
    };
    let dst = node.get(target);
    match node.status {
        DiffStatus::TypeConflict => {
            plan.notes.push(format!(
                "{rel}: skipped, file on one side and folder on the other."
            ));
            return;
        }
        DiffStatus::Error => {
            plan.notes.push(format!(
                "{rel}: skipped, {}",
                node.error.as_deref().unwrap_or_default()
            ));
            return;
        }
        _ => {}
    }

    let target_path = match dst {
        Some(d) => d.full_path.clone(),
        None => mirrored(result, src, source, rel),
    };

    if node.is_dir {
        if dst.is_none() {
            plan.actions.push(PlannedAction {
                kind: ActionKind::CreateDirectory,
                node: id,
                relative_path: rel.clone(),
                target_side: target,
                source_path: Some(src.full_path.clone()),
                target_path,
                is_dir: true,
                size: 0,
                source_time: None,
                target_time: None,
                target_exists: false,
                target_is_newer: false,
                direction_unknown: false,
            });
        }
        for &child in &node.children {
            add_copy(result, child, direction, plan, false);
        }
        return;
    }

    match node.status {
        DiffStatus::Identical => {
            if top_level {
                plan.notes.push(format!("{rel}: already identical."));
            }
            return;
        }
        DiffStatus::SameContentDifferentTime => {
            if top_level {
                plan.notes.push(format!(
                    "{rel}: content identical, only the timestamps differ."
                ));
            }
            return;
        }
        _ => {}
    }

    let direction_unknown = node.status == DiffStatus::SameTimeDifferentContent && dst.is_some();
    let target_is_newer = !direction_unknown
        && dst.is_some_and(|d| is_newer(d.modified, src.modified, result.options.tolerance));
    plan.actions.push(PlannedAction {
        kind: ActionKind::CopyFile,
        node: id,
        relative_path: rel.clone(),
        target_side: target,
        source_path: Some(src.full_path.clone()),
        target_path,
        is_dir: false,
        size: src.size,
        source_time: Some(src.modified),
        target_time: dst.map(|d| d.modified),
        target_exists: dst.is_some(),
        target_is_newer,
        direction_unknown,
    });
}
