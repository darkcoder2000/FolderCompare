//! View model of the main window, kept free of egui so it can be unit-tested: filtering, the visible row list,
//! sorting, expansion, selection, counts and the enable rules of the commands.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use fc_core::compare::DiffResult;
use fc_core::model::{DiffNode, DiffStatus, DiffTree, NodeId};
use fc_core::ops::top_level_only;

use crate::settings::FilterSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortKey {
    Name,
    LeftSize,
    LeftDate,
    Status,
    RightSize,
    RightDate,
}

/// The status filter toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Filters {
    pub show_only_left: bool,
    pub show_only_right: bool,
    pub show_newer_left: bool,
    pub show_newer_right: bool,
    pub show_identical: bool,
    pub show_same_time_different: bool,
    pub show_time_only: bool,
    pub show_errors: bool,
    pub differences_only: bool,
}

impl Default for Filters {
    fn default() -> Self {
        Self::from(&FilterSettings::default())
    }
}

impl From<&FilterSettings> for Filters {
    fn from(f: &FilterSettings) -> Self {
        Self {
            show_only_left: f.show_only_left,
            show_only_right: f.show_only_right,
            show_newer_left: f.show_newer_left,
            show_newer_right: f.show_newer_right,
            show_identical: f.show_identical,
            show_same_time_different: f.show_same_time_different,
            show_time_only: f.show_time_only,
            show_errors: f.show_errors,
            differences_only: f.differences_only,
        }
    }
}

impl Filters {
    pub fn write_to(&self, f: &mut FilterSettings) {
        f.show_only_left = self.show_only_left;
        f.show_only_right = self.show_only_right;
        f.show_newer_left = self.show_newer_left;
        f.show_newer_right = self.show_newer_right;
        f.show_identical = self.show_identical;
        f.show_same_time_different = self.show_same_time_different;
        f.show_time_only = self.show_time_only;
        f.show_errors = self.show_errors;
        f.differences_only = self.differences_only;
    }

    pub fn to_settings(self) -> FilterSettings {
        let mut f = FilterSettings::default();
        self.write_to(&mut f);
        f
    }

    pub fn passes(&self, status: DiffStatus) -> bool {
        match status {
            DiffStatus::OnlyLeft => self.show_only_left,
            DiffStatus::OnlyRight => self.show_only_right,
            DiffStatus::NewerLeft => self.show_newer_left,
            DiffStatus::NewerRight => self.show_newer_right,
            DiffStatus::Identical => self.show_identical && !self.differences_only,
            DiffStatus::SameTimeDifferentContent => self.show_same_time_different,
            DiffStatus::SameContentDifferentTime => self.show_time_only && !self.differences_only,
            DiffStatus::Differs => true,
            DiffStatus::Error | DiffStatus::TypeConflict => self.show_errors,
        }
    }
}

/// One visible row of the side-by-side tree grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub node: NodeId,
    pub level: usize,
}

/// How one side of a row is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideState {
    Missing,
    Normal,
    Newer,
    Older,
    Only,
    Error,
    /// Same timestamp but different content: neither side can be called newer.
    Conflict,
}

pub fn side_state(node: &DiffNode, left: bool) -> SideState {
    if node.get(side(left)).is_none() {
        return SideState::Missing;
    }
    match node.status {
        DiffStatus::Error | DiffStatus::TypeConflict => SideState::Error,
        DiffStatus::OnlyLeft | DiffStatus::OnlyRight => SideState::Only,
        DiffStatus::NewerLeft if left => SideState::Newer,
        DiffStatus::NewerLeft => SideState::Older,
        DiffStatus::NewerRight if !left => SideState::Newer,
        DiffStatus::NewerRight => SideState::Older,
        DiffStatus::SameTimeDifferentContent => SideState::Conflict,
        _ => SideState::Normal,
    }
}

fn side(left: bool) -> fc_core::model::Side {
    if left {
        fc_core::model::Side::Left
    } else {
        fc_core::model::Side::Right
    }
}

pub fn status_glyph(status: DiffStatus) -> &'static str {
    match status {
        DiffStatus::NewerLeft | DiffStatus::OnlyLeft => "→",
        DiffStatus::NewerRight | DiffStatus::OnlyRight => "←",
        DiffStatus::Identical | DiffStatus::SameContentDifferentTime => "=",
        DiffStatus::Differs => "≠",
        _ => "⚠",
    }
}

pub fn row_tooltip(node: &DiffNode) -> Option<&str> {
    if let Some(e) = &node.error {
        return Some(e);
    }
    match node.status {
        DiffStatus::SameTimeDifferentContent => {
            Some("Same timestamp but different size or content - can't tell which side is newer")
        }
        DiffStatus::SameContentDifferentTime => Some("Content identical, timestamps differ"),
        _ => None,
    }
}

/// `1,234` (thousands grouped).
pub fn format_count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `{n:N0} B` below 1024, otherwise KB/MB/GB/TB with one decimal.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", format_count(bytes))
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn format_date(t: SystemTime) -> String {
    fc_core::report::format_date(t)
}

/// Main window state that does not depend on egui.
#[derive(Default)]
pub struct MainState {
    pub result: Option<DiffResult>,
    /// Incremented for every new comparison; background results for an older one are dropped.
    pub generation: u64,
    pub filters: Filters,
    /// The applied (debounced) search text.
    pub search: String,
    pub folders_first: bool,
    pub sort_key: Option<SortKey>,
    pub sort_ascending: bool,
    expanded: HashSet<String>,
    visible: Vec<bool>,
    pub rows: Vec<Row>,
    row_index: HashMap<NodeId, usize>,
    selected: HashSet<NodeId>,
    /// Row the keyboard works on (the last clicked or moved-to row).
    pub focus: Option<NodeId>,
    anchor: Option<NodeId>,
}

impl MainState {
    pub fn new(filters: Filters, folders_first: bool) -> Self {
        Self {
            filters,
            folders_first,
            sort_key: Some(SortKey::Name),
            sort_ascending: true,
            ..Self::default()
        }
    }

    pub fn tree(&self) -> Option<&DiffTree> {
        self.result.as_ref().map(|r| &r.tree)
    }

    pub fn node(&self, id: NodeId) -> &DiffNode {
        self.result.as_ref().expect("a result").tree.node(id)
    }

    fn search_active(&self) -> bool {
        !self.search.trim().is_empty()
    }

    fn key(&self, rel: &str) -> String {
        match &self.result {
            Some(r) => r.paths().key(rel).into_owned(),
            None => rel.to_string(),
        }
    }

    /// Replaces the result after a comparison; keeps expanded folders, clears the selection.
    pub fn set_result(&mut self, result: DiffResult) {
        self.result = Some(result);
        self.generation += 1;
        self.selected.clear();
        self.focus = None;
        self.anchor = None;
        self.sort_whole_tree();
        self.rebuild_rows(false);
    }

    // ---- Visibility and rows ----------------------------------------------------------------

    /// Recomputes filter visibility over the whole tree and regenerates the visible row list.
    pub fn rebuild_rows(&mut self, preserve_selection: bool) {
        let selected_paths: HashSet<String> = if preserve_selection {
            self.selected
                .iter()
                .filter_map(|&id| self.tree().map(|t| self.key(&t.node(id).relative_path)))
                .collect()
        } else {
            HashSet::new()
        };
        let focus_path = self
            .focus
            .and_then(|id| self.tree().map(|t| self.key(&t.node(id).relative_path)));

        self.visible.clear();
        if let Some(tree) = self.tree() {
            let mut visible = vec![false; tree.len()];
            self.compute_visible(tree, NodeId::ROOT, &mut visible);
            self.visible = visible;
        }
        self.emit_rows();

        self.selected.clear();
        self.focus = None;
        if let Some(tree) = self.result.as_ref().map(|r| &r.tree) {
            for row in &self.rows {
                let key = self.key(&tree.node(row.node).relative_path);
                if selected_paths.contains(&key) {
                    self.selected.insert(row.node);
                }
                if focus_path.as_ref() == Some(&key) {
                    self.focus = Some(row.node);
                }
            }
        }
        if self
            .anchor
            .is_some_and(|a| !self.row_index.contains_key(&a))
        {
            self.anchor = self.focus;
        }
    }

    fn compute_visible(&self, tree: &DiffTree, id: NodeId, visible: &mut [bool]) -> bool {
        let node = tree.node(id);
        let mut any_child = false;
        for &c in &node.children {
            any_child |= self.compute_visible(tree, c, visible);
        }
        let v = if node.is_dir && !node.children.is_empty() {
            any_child
                || (self.search_active()
                    && self.name_matches(node)
                    && self.filters.passes(node.status))
        } else {
            self.filters.passes(node.status) && self.name_matches(node)
        };
        visible[id.index()] = v;
        v
    }

    fn name_matches(&self, node: &DiffNode) -> bool {
        if !self.search_active() {
            return true;
        }
        node.name
            .to_lowercase()
            .contains(&self.search.trim().to_lowercase())
    }

    pub fn is_visible(&self, id: NodeId) -> bool {
        self.visible.get(id.index()).copied().unwrap_or(false)
    }

    /// Regenerates the row list from the current visibility (after expanding or collapsing).
    fn emit_rows(&mut self) {
        let mut rows = Vec::new();
        if let Some(tree) = self.tree() {
            let mut stack: Vec<Row> = tree
                .root()
                .children
                .iter()
                .rev()
                .filter(|&&c| self.is_visible(c))
                .map(|&c| Row { node: c, level: 0 })
                .collect();
            while let Some(row) = stack.pop() {
                rows.push(row);
                let node = tree.node(row.node);
                if node.is_dir && (self.search_active() || self.is_expanded(row.node)) {
                    stack.extend(
                        node.children
                            .iter()
                            .rev()
                            .filter(|&&c| self.is_visible(c))
                            .map(|&c| Row {
                                node: c,
                                level: row.level + 1,
                            }),
                    );
                }
            }
        }
        self.row_index = rows.iter().enumerate().map(|(i, r)| (r.node, i)).collect();
        self.rows = rows;
        let index = &self.row_index;
        self.selected.retain(|id| index.contains_key(id));
        if self.focus.is_some_and(|f| !index.contains_key(&f)) {
            self.focus = None;
        }
    }

    pub fn row_of(&self, id: NodeId) -> Option<usize> {
        self.row_index.get(&id).copied()
    }

    // ---- Expansion ---------------------------------------------------------------------------

    pub fn is_expanded(&self, id: NodeId) -> bool {
        self.tree()
            .is_some_and(|t| self.expanded.contains(&self.key(&t.node(id).relative_path)))
    }

    pub fn is_expandable(&self, id: NodeId) -> bool {
        self.tree().is_some_and(|t| {
            let n = t.node(id);
            n.is_dir && !n.children.is_empty()
        })
    }

    pub fn set_expanded(&mut self, id: NodeId, expand: bool) {
        let Some(tree) = self.tree() else { return };
        let key = self.key(&tree.node(id).relative_path);
        if expand {
            self.expanded.insert(key);
        } else {
            self.expanded.remove(&key);
        }
        if !self.search_active() {
            self.emit_rows();
        }
    }

    pub fn toggle_expanded(&mut self, id: NodeId) {
        let e = self.is_expanded(id);
        self.set_expanded(id, !e);
    }

    pub fn expand_all(&mut self) {
        let Some(tree) = self.tree() else { return };
        let keys: Vec<String> = tree
            .descendants(NodeId::ROOT)
            .into_iter()
            .filter(|&n| tree.node(n).is_dir && !tree.node(n).children.is_empty())
            .map(|n| self.key(&tree.node(n).relative_path))
            .collect();
        self.expanded.extend(keys);
        self.rebuild_rows(true);
    }

    pub fn collapse_all(&mut self) {
        self.expanded.clear();
        self.rebuild_rows(true);
    }

    pub fn expand_subtree(&mut self) {
        let Some(tree) = self.tree() else { return };
        let mut keys = Vec::new();
        for &id in &self.selected {
            if tree.node(id).is_dir {
                keys.push(self.key(&tree.node(id).relative_path));
            }
            for n in tree.descendants(id) {
                if tree.node(n).is_dir && !tree.node(n).children.is_empty() {
                    keys.push(self.key(&tree.node(n).relative_path));
                }
            }
        }
        self.expanded.extend(keys);
        self.rebuild_rows(true);
    }

    pub fn collapse_subtree(&mut self) {
        let Some(tree) = self.tree() else { return };
        let mut keys = Vec::new();
        for &id in &self.selected {
            keys.push(self.key(&tree.node(id).relative_path));
            for n in tree.descendants(id) {
                keys.push(self.key(&tree.node(n).relative_path));
            }
        }
        for k in keys {
            self.expanded.remove(&k);
        }
        self.rebuild_rows(true);
    }

    // ---- Sorting -----------------------------------------------------------------------------

    /// Clicking the same key again toggles the direction.
    pub fn sort_by(&mut self, key: SortKey) {
        if self.sort_key == Some(key) {
            self.sort_ascending = !self.sort_ascending;
        } else {
            self.sort_key = Some(key);
            self.sort_ascending = true;
        }
        self.apply_sort();
    }

    pub fn set_folders_first(&mut self, value: bool) {
        self.folders_first = value;
        self.apply_sort();
    }

    pub fn apply_sort(&mut self) {
        if self.result.is_none() {
            return;
        }
        self.sort_whole_tree();
        self.rebuild_rows(true);
    }

    fn sort_whole_tree(&mut self) {
        self.sort_subtree(NodeId::ROOT);
    }

    /// Sorts the children of `id` and of every folder below it.
    pub fn sort_subtree(&mut self, id: NodeId) {
        let (key, ascending, folders_first) = (
            self.sort_key.unwrap_or(SortKey::Name),
            self.sort_ascending,
            self.folders_first,
        );
        let Some(result) = self.result.as_mut() else {
            return;
        };
        let tree = &mut result.tree;
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            let mut children = std::mem::take(&mut tree.node_mut(n).children);
            children.sort_by(|&a, &b| {
                compare_nodes(tree.node(a), tree.node(b), key, ascending, folders_first)
            });
            stack.extend(children.iter().copied().filter(|&c| tree.node(c).is_dir));
            tree.node_mut(n).children = children;
        }
    }

    // ---- Selection ---------------------------------------------------------------------------

    pub fn is_selected(&self, id: NodeId) -> bool {
        self.selected.contains(&id)
    }

    pub fn selection_count(&self) -> usize {
        self.selected.len()
    }

    /// Selected nodes in display order.
    pub fn selected_nodes(&self) -> Vec<NodeId> {
        self.rows
            .iter()
            .map(|r| r.node)
            .filter(|n| self.selected.contains(n))
            .collect()
    }

    pub fn first_selected(&self) -> Option<NodeId> {
        self.selected_nodes().first().copied()
    }

    /// A click on a row: plain click selects only it, Ctrl toggles, Shift selects the range from the anchor.
    pub fn click(&mut self, id: NodeId, ctrl: bool, shift: bool) {
        if shift {
            let anchor = self
                .anchor
                .filter(|a| self.row_index.contains_key(a))
                .unwrap_or(id);
            self.select_range(anchor, id, ctrl);
        } else if ctrl {
            if !self.selected.remove(&id) {
                self.selected.insert(id);
            }
            self.anchor = Some(id);
        } else {
            self.selected.clear();
            self.selected.insert(id);
            self.anchor = Some(id);
        }
        self.focus = Some(id);
    }

    fn select_range(&mut self, from: NodeId, to: NodeId, keep: bool) {
        let (Some(a), Some(b)) = (self.row_of(from), self.row_of(to)) else {
            return;
        };
        if !keep {
            self.selected.clear();
        }
        let (lo, hi) = (a.min(b), a.max(b));
        for r in &self.rows[lo..=hi] {
            self.selected.insert(r.node);
        }
    }

    /// Right-click on a row that isn't selected selects it first.
    pub fn context_click(&mut self, id: NodeId) {
        if !self.selected.contains(&id) {
            self.click(id, false, false);
        }
    }

    /// Space: toggles the focused row.
    pub fn toggle_focused(&mut self) {
        if let Some(f) = self.focus
            && !self.selected.remove(&f)
        {
            self.selected.insert(f);
        }
    }

    /// Moves the keyboard focus by `delta` rows (clamped); Shift extends the selection, Ctrl only moves the focus.
    pub fn move_focus(&mut self, delta: isize, shift: bool, ctrl: bool) {
        if self.rows.is_empty() {
            return;
        }
        let current = self.focus.and_then(|f| self.row_of(f));
        let target = match current {
            None => {
                if delta >= 0 {
                    0
                } else {
                    self.rows.len() - 1
                }
            }
            Some(c) => (c as isize + delta).clamp(0, self.rows.len() as isize - 1) as usize,
        };
        let id = self.rows[target].node;
        if ctrl && !shift {
            self.focus = Some(id);
        } else {
            self.click(id, false, shift);
        }
    }

    pub fn select_rows(&mut self, ids: impl IntoIterator<Item = NodeId>) {
        self.selected = ids.into_iter().collect();
        self.focus = self.selected_nodes().first().copied();
        self.anchor = self.focus;
    }

    pub fn select_all(&mut self) {
        let all: Vec<NodeId> = self.rows.iter().map(|r| r.node).collect();
        let focus = self.focus;
        self.select_rows(all);
        if focus.is_some() {
            self.focus = focus;
        }
    }

    pub fn invert_selection(&mut self) {
        let ids: Vec<NodeId> = self
            .rows
            .iter()
            .map(|r| r.node)
            .filter(|n| !self.selected.contains(n))
            .collect();
        self.select_rows(ids);
    }

    pub fn select_by_status(&mut self, status: DiffStatus) {
        let Some(tree) = self.tree() else { return };
        let ids: Vec<NodeId> = self
            .rows
            .iter()
            .map(|r| r.node)
            .filter(|&n| tree.node(n).status == status)
            .collect();
        self.select_rows(ids);
    }

    /// Selects the parent row of the focused row (Left key on a collapsed row).
    pub fn focus_parent(&mut self) {
        let Some(f) = self.focus else { return };
        if let Some(parent) = self.node(f).parent
            && parent != NodeId::ROOT
            && self.row_of(parent).is_some()
        {
            self.click(parent, false, false);
        }
    }

    pub fn selection_text(&self) -> String {
        let Some(tree) = self.tree() else {
            return String::new();
        };
        if self.selected.is_empty() {
            return String::new();
        }
        let ids: Vec<NodeId> = self.selected.iter().copied().collect();
        let size: u64 = top_level_only(tree, &ids)
            .into_iter()
            .map(|n| tree.node(n).left_total.max(tree.node(n).right_total))
            .sum();
        format!(
            "{} selected ({})",
            format_count(self.selected.len() as u64),
            format_size(size)
        )
    }

    // ---- Counts ------------------------------------------------------------------------------

    pub fn counts_text(&self) -> String {
        let Some(tree) = self.tree() else {
            return String::new();
        };
        let mut counts: HashMap<DiffStatus, u64> = HashMap::new();
        for id in tree.descendants(NodeId::ROOT) {
            let n = tree.node(id);
            if n.is_dir && !matches!(n.status, DiffStatus::Error | DiffStatus::TypeConflict) {
                continue;
            }
            *counts.entry(n.status).or_insert(0) += 1;
        }
        let parts: Vec<String> = [
            (DiffStatus::OnlyLeft, "only left"),
            (DiffStatus::NewerLeft, "newer left"),
            (DiffStatus::NewerRight, "newer right"),
            (DiffStatus::OnlyRight, "only right"),
            (
                DiffStatus::SameTimeDifferentContent,
                "same time, different content",
            ),
            (DiffStatus::TypeConflict, "conflicts"),
            (DiffStatus::Error, "errors"),
            (DiffStatus::Identical, "identical"),
            (DiffStatus::SameContentDifferentTime, "timestamp only"),
        ]
        .iter()
        .filter_map(|(s, label)| {
            counts
                .get(s)
                .filter(|&&c| c > 0)
                .map(|c| format!("{} {label}", format_count(*c)))
        })
        .collect();
        if parts.is_empty() {
            "No files".to_string()
        } else {
            parts.join(" · ")
        }
    }

    /// All nodes that pass the current filter, regardless of expansion (for the report).
    pub fn visible_nodes(&self) -> Vec<NodeId> {
        let Some(tree) = self.tree() else {
            return Vec::new();
        };
        tree.descendants(NodeId::ROOT)
            .into_iter()
            .filter(|&n| self.is_visible(n))
            .collect()
    }

    // ---- Enable rules ------------------------------------------------------------------------

    fn any_selected(&self, f: impl Fn(&DiffNode) -> bool) -> bool {
        self.tree()
            .is_some_and(|t| self.selected.iter().any(|&id| f(t.node(id))))
    }

    pub fn can_copy_to_right(&self, busy: bool) -> bool {
        !busy && self.any_selected(|n| n.left.is_some())
    }

    pub fn can_copy_to_left(&self, busy: bool) -> bool {
        !busy && self.any_selected(|n| n.right.is_some())
    }

    pub fn can_delete_left(&self, busy: bool) -> bool {
        self.can_copy_to_right(busy)
    }

    pub fn can_delete_right(&self, busy: bool) -> bool {
        self.can_copy_to_left(busy)
    }

    pub fn can_delete_any(&self, busy: bool) -> bool {
        !busy && !self.selected.is_empty()
    }

    pub fn can_rename(&self, busy: bool) -> bool {
        !busy && self.selected.len() == 1
    }

    pub fn can_refresh_selected(&self, busy: bool) -> bool {
        !busy && !self.selected.is_empty() && self.result.is_some()
    }

    pub fn can_export(&self, busy: bool) -> bool {
        !busy && self.result.is_some()
    }

    pub fn can_expand_subtree(&self) -> bool {
        !self.selected.is_empty()
    }

    /// Exactly one file, existing on at least one side.
    pub fn can_compare_contents(&self, busy: bool) -> bool {
        !busy
            && self.selected.len() == 1
            && self.any_selected(|n| !n.is_dir && (n.left.is_some() || n.right.is_some()))
    }
}

fn nanos(t: SystemTime) -> i128 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128,
        Err(e) => -(e.duration().as_nanos() as i128),
    }
}

/// Folders first (if enabled), then the key, then the name ignoring case; descending reverses everything.
pub fn compare_nodes(
    a: &DiffNode,
    b: &DiffNode,
    key: SortKey,
    ascending: bool,
    folders_first: bool,
) -> Ordering {
    let size = |n: &DiffNode, left: bool| -> i128 {
        match (left, &n.left, &n.right) {
            (true, Some(_), _) => n.left_total as i128,
            (false, _, Some(_)) => n.right_total as i128,
            _ => -1,
        }
    };
    let date = |n: &DiffNode, left: bool| -> i128 {
        let s = if left { &n.left } else { &n.right };
        s.as_ref().map_or(i128::MIN, |s| nanos(s.modified))
    };
    let mut c = if folders_first && a.is_dir != b.is_dir {
        if a.is_dir {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    } else {
        Ordering::Equal
    };
    if c == Ordering::Equal {
        c = match key {
            SortKey::LeftSize => size(a, true).cmp(&size(b, true)),
            SortKey::RightSize => size(a, false).cmp(&size(b, false)),
            SortKey::LeftDate => date(a, true).cmp(&date(b, true)),
            SortKey::RightDate => date(a, false).cmp(&date(b, false)),
            SortKey::Status => a.status.cmp(&b.status),
            SortKey::Name => Ordering::Equal,
        };
    }
    if c == Ordering::Equal {
        c = compare_ignore_case(&a.name, &b.name);
    }
    if ascending { c } else { c.reverse() }
}

fn compare_ignore_case(a: &str, b: &str) -> Ordering {
    let up = fc_core::compare::upper_char;
    a.chars().map(up).cmp(b.chars().map(up))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use fc_core::compare::PathComparison;
    use fc_core::model::{
        Attributes, CompareOptions, DiffNode, DiffTree, FileSnapshot, join_relative,
    };

    use super::*;

    fn snap(size: u64, secs: u64, dir: bool) -> FileSnapshot {
        FileSnapshot {
            size,
            modified: UNIX_EPOCH + Duration::from_secs(secs),
            attributes: Attributes(if dir { Attributes::DIRECTORY } else { 0 }),
            unix_mode: None,
            full_path: PathBuf::new(),
        }
    }

    struct Builder {
        tree: DiffTree,
    }

    impl Builder {
        fn new() -> Self {
            let mut root = DiffNode::new(String::new(), String::new(), true);
            root.left = Some(snap(0, 0, true));
            root.right = Some(snap(0, 0, true));
            Self {
                tree: DiffTree::new(root),
            }
        }

        fn add(&mut self, parent: NodeId, name: &str, dir: bool, status: DiffStatus) -> NodeId {
            let rel = join_relative(&self.tree.node(parent).relative_path, name);
            let mut n = DiffNode::new(rel, name.to_string(), dir);
            n.status = status;
            if status != DiffStatus::OnlyRight {
                n.left = Some(snap(10, 100, dir));
                n.left_total = 10;
            }
            if status != DiffStatus::OnlyLeft {
                n.right = Some(snap(20, 200, dir));
                n.right_total = 20;
            }
            let id = self.tree.push(n);
            self.tree.add_child(parent, id);
            id
        }

        fn result(self) -> DiffResult {
            DiffResult {
                left_root: PathBuf::from("L"),
                right_root: PathBuf::from("R"),
                options: CompareOptions {
                    path_comparison: Some(PathComparison::IgnoreCase),
                    ..CompareOptions::default()
                },
                tree: self.tree,
            }
        }
    }

    /// root: b.txt (identical), dir/ (differs: x.txt newer left, y.txt only right), A.txt (only left)
    fn sample() -> (MainState, [NodeId; 5]) {
        let mut b = Builder::new();
        let r = NodeId::ROOT;
        let bt = b.add(r, "b.txt", false, DiffStatus::Identical);
        let dir = b.add(r, "dir", true, DiffStatus::Differs);
        let x = b.add(dir, "x.txt", false, DiffStatus::NewerLeft);
        let y = b.add(dir, "y.txt", false, DiffStatus::OnlyRight);
        let a = b.add(r, "A.txt", false, DiffStatus::OnlyLeft);
        let mut s = MainState::new(Filters::default(), true);
        s.set_result(b.result());
        (s, [bt, dir, x, y, a])
    }

    fn names(s: &MainState) -> Vec<String> {
        s.rows
            .iter()
            .map(|r| format!("{}{}", "  ".repeat(r.level), s.node(r.node).name))
            .collect()
    }

    #[test]
    fn sorts_folders_first_then_name_ignoring_case() {
        let (s, _) = sample();
        assert_eq!(vec!["dir", "A.txt", "b.txt"], names(&s));
    }

    #[test]
    fn sort_toggles_direction_and_descending_reverses_folders_first() {
        let (mut s, _) = sample();
        s.sort_by(SortKey::Name);
        assert_eq!(vec!["b.txt", "A.txt", "dir"], names(&s));
        s.sort_by(SortKey::Status);
        // Identical < OnlyLeft (enum order), folder first.
        assert_eq!(vec!["dir", "b.txt", "A.txt"], names(&s));
        s.set_folders_first(false);
        assert_eq!(vec!["b.txt", "A.txt", "dir"], names(&s));
    }

    #[test]
    fn missing_side_sorts_as_smallest_size() {
        let (mut s, [_, dir, ..]) = sample();
        s.set_expanded(dir, true);
        s.sort_by(SortKey::LeftSize);
        assert_eq!(
            vec!["dir", "  y.txt", "  x.txt", "A.txt", "b.txt"],
            names(&s)
        );
    }

    #[test]
    fn expand_and_collapse() {
        let (mut s, [_, dir, x, ..]) = sample();
        assert!(s.is_expandable(dir) && !s.is_expandable(x));
        s.toggle_expanded(dir);
        assert_eq!(
            vec!["dir", "  x.txt", "  y.txt", "A.txt", "b.txt"],
            names(&s)
        );
        s.collapse_all();
        assert_eq!(3, s.rows.len());
        s.expand_all();
        assert_eq!(5, s.rows.len());
    }

    #[test]
    fn filters_hide_rows_and_empty_folders() {
        let (mut s, [_, dir, ..]) = sample();
        s.set_expanded(dir, true);
        s.filters.differences_only = true;
        s.rebuild_rows(true);
        assert_eq!(vec!["dir", "  x.txt", "  y.txt", "A.txt"], names(&s));
        s.filters.show_newer_left = false;
        s.filters.show_only_right = false;
        s.rebuild_rows(true);
        assert_eq!(vec!["A.txt"], names(&s));
    }

    #[test]
    fn search_expands_all_matches() {
        let (mut s, _) = sample();
        s.search = "Y.T".into();
        s.rebuild_rows(true);
        assert_eq!(vec!["dir", "  y.txt"], names(&s));
        s.search = "dir".into();
        s.rebuild_rows(true);
        assert_eq!(vec!["dir"], names(&s));
    }

    #[test]
    fn filter_rules() {
        let mut f = Filters::default();
        assert!(f.passes(DiffStatus::Identical) && f.passes(DiffStatus::SameContentDifferentTime));
        f.differences_only = true;
        assert!(
            !f.passes(DiffStatus::Identical) && !f.passes(DiffStatus::SameContentDifferentTime)
        );
        assert!(f.passes(DiffStatus::Differs) && f.passes(DiffStatus::TypeConflict));
        f.show_errors = false;
        assert!(!f.passes(DiffStatus::Error) && !f.passes(DiffStatus::TypeConflict));
    }

    #[test]
    fn selection_click_ctrl_shift_and_text() {
        let (mut s, [bt, dir, _, _, a]) = sample();
        s.click(dir, false, false);
        s.click(bt, false, true);
        assert_eq!(vec![dir, a, bt], s.selected_nodes());
        s.click(a, true, false);
        assert_eq!(vec![dir, bt], s.selected_nodes());
        assert_eq!("2 selected (40 B)", s.selection_text());
        s.invert_selection();
        assert_eq!(vec![a], s.selected_nodes());
        s.select_all();
        assert_eq!(3, s.selection_count());
        s.select_by_status(DiffStatus::OnlyLeft);
        assert_eq!(vec![a], s.selected_nodes());
    }

    #[test]
    fn selection_survives_rebuild_and_drops_hidden_rows() {
        let (mut s, [_, dir, x, ..]) = sample();
        s.set_expanded(dir, true);
        s.click(x, false, false);
        s.sort_by(SortKey::Name);
        assert_eq!(vec![x], s.selected_nodes());
        s.set_expanded(dir, false);
        assert!(s.selected_nodes().is_empty());
    }

    #[test]
    fn keyboard_focus_moves_and_parent() {
        let (mut s, [_, dir, x, ..]) = sample();
        s.set_expanded(dir, true);
        s.move_focus(1, false, false);
        assert_eq!(Some(dir), s.focus);
        s.move_focus(1, false, false);
        assert_eq!(vec![x], s.selected_nodes());
        s.move_focus(1, true, false);
        assert_eq!(2, s.selection_count());
        s.move_focus(100, false, false);
        assert_eq!("b.txt", s.node(s.focus.unwrap()).name);
        s.click(x, false, false);
        s.focus_parent();
        assert_eq!(vec![dir], s.selected_nodes());
    }

    #[test]
    fn counts_skip_folders() {
        let (s, _) = sample();
        assert_eq!(
            "1 only left · 1 newer left · 1 only right · 1 identical",
            s.counts_text()
        );
        assert_eq!("", MainState::new(Filters::default(), true).counts_text());
        let mut empty = MainState::new(Filters::default(), true);
        empty.set_result(Builder::new().result());
        assert_eq!("No files", empty.counts_text());
    }

    #[test]
    fn enable_rules() {
        let (mut s, [bt, dir, x, y, a]) = sample();
        assert!(!s.can_copy_to_right(false) && !s.can_delete_any(false));
        assert!(s.can_export(false) && !s.can_export(true));
        s.click(a, false, false);
        assert!(s.can_copy_to_right(false) && !s.can_copy_to_left(false));
        assert!(s.can_compare_contents(false) && s.can_rename(false));
        assert!(!s.can_compare_contents(true));
        s.click(dir, false, false);
        assert!(!s.can_compare_contents(false));
        s.set_expanded(dir, true);
        s.click(y, false, false);
        assert!(s.can_copy_to_left(false) && !s.can_delete_left(false));
        s.click(x, true, false);
        assert!(!s.can_rename(false) && s.can_delete_any(false));
        let _ = bt;
    }

    #[test]
    fn formats() {
        assert_eq!("0 B", format_size(0));
        assert_eq!("1,023 B", format_size(1023));
        assert_eq!("1.0 KB", format_size(1024));
        assert_eq!("1.5 MB", format_size(1024 * 1024 * 3 / 2));
        assert_eq!("1,234,567", format_count(1_234_567));
        assert_eq!("≠", status_glyph(DiffStatus::Differs));
        assert_eq!("⚠", status_glyph(DiffStatus::TypeConflict));
    }

    #[test]
    fn side_states() {
        let (s, [_, _, x, y, _]) = sample();
        assert_eq!(SideState::Newer, side_state(s.node(x), true));
        assert_eq!(SideState::Older, side_state(s.node(x), false));
        assert_eq!(SideState::Missing, side_state(s.node(y), true));
        assert_eq!(SideState::Only, side_state(s.node(y), false));
    }
}
