//! The tree controller: expansion, selection, keyboard navigation and
//! type-ahead over a nested model.
//!
//! The nested [`TreeNode`] a caller builds is flattened once into an arena
//! (`O(nodes)`), so a row's label and children are an array index away. The
//! visible-row list is rebuilt only when the shape or an expansion changes,
//! never per frame; `flatten_into` copies it into a caller buffer and allocates
//! nothing when that buffer has capacity.
//!
//! Navigation is `O(1)` for Up/Down/Home/End because the selected row index is
//! kept; Left/Right touch one node. Type-ahead scans the visible rows and is
//! `O(visible)` in the worst case, with no allocation.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // reason: row, column and pixel values are bounded UI quantities

/// One input node of the tree.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeNode<T> {
    /// The row label.
    pub label: String,
    /// Whether the node starts expanded.
    pub expanded: bool,
    /// Application data.
    pub data: T,
    /// Child nodes, in display order.
    pub children: Vec<TreeNode<T>>,
}

impl<T> TreeNode<T> {
    /// A leaf node with a label.
    #[must_use]
    pub fn new(label: impl Into<String>, data: T) -> Self {
        Self {
            label: label.into(),
            expanded: false,
            data,
            children: Vec::new(),
        }
    }

    /// The same node, expanded initially.
    #[must_use]
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    /// Appends a child.
    #[must_use]
    pub fn add_child(mut self, child: Self) -> Self {
        self.children.push(child);
        self
    }
}

/// One visible row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Row {
    /// Arena index of the node.
    pub node: usize,
    /// Nesting depth, zero at the root.
    pub depth: u16,
    /// Position in the visible list.
    pub flat_index: usize,
    /// Whether the node has children.
    pub has_children: bool,
    /// Whether the node is expanded.
    pub expanded: bool,
    /// Whether the node is selected.
    pub selected: bool,
}

/// A navigation key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeKey {
    /// One row up.
    Up,
    /// One row down.
    Down,
    /// Collapse, or move to the parent.
    Left,
    /// Expand, or move to the first child.
    Right,
    /// First row.
    Home,
    /// Last row.
    End,
    /// One page up.
    PageUp,
    /// One page down.
    PageDown,
    /// Toggle the selected row.
    Enter,
}

struct Node<T> {
    label: String,
    expanded: bool,
    data: T,
    parent: Option<usize>,
    children: Vec<usize>,
    depth: u16,
}

/// A nested tree flattened into an arena.
pub struct Tree<T> {
    nodes: Vec<Node<T>>,
    root: Option<usize>,
    visible: Vec<usize>,
    selected: Option<usize>,
    row_height: f32,
    type_ahead: String,
    type_ahead_at: f64,
    type_ahead_ms: f64,
}

impl<T> Default for Tree<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Tree<T> {
    /// Builds an empty tree.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            root: None,
            visible: Vec::new(),
            selected: None,
            row_height: 24.0,
            type_ahead: String::new(),
            type_ahead_at: f64::NEG_INFINITY,
            type_ahead_ms: 800.0,
        }
    }

    /// Builds a tree from a root node.
    #[must_use]
    pub fn with_root(root: TreeNode<T>) -> Self {
        let mut tree = Self::new();
        tree.set_root(root);
        tree
    }

    /// Replaces the tree. `O(nodes)`; the nested model is moved into the arena
    /// with an explicit stack, so a 10 000-level chain cannot overflow.
    pub fn set_root(&mut self, root: TreeNode<T>) {
        self.nodes.clear();
        self.root = None;
        self.selected = None;
        let mut stack: Vec<(TreeNode<T>, Option<usize>, u16)> = vec![(root, None, 0)];
        let root_index = self.nodes.len();
        self.root = Some(root_index);
        while let Some((node, parent, depth)) = stack.pop() {
            let index = self.nodes.len();
            self.nodes.push(Node {
                label: node.label,
                expanded: node.expanded,
                data: node.data,
                parent,
                children: Vec::new(),
                depth,
            });
            if let Some(parent) = parent {
                self.nodes[parent].children.push(index);
            }
            for child in node.children.into_iter().rev() {
                stack.push((child, Some(index), depth.saturating_add(1)));
            }
        }
        self.rebuild_visible();
    }

    /// The number of nodes in the arena.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree has no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// A node's label by arena index.
    #[must_use]
    pub fn label(&self, node: usize) -> Option<&str> {
        self.nodes.get(node).map(|node| node.label.as_str())
    }

    /// A node's data by arena index.
    #[must_use]
    pub fn data(&self, node: usize) -> Option<&T> {
        self.nodes.get(node).map(|node| &node.data)
    }

    /// A node's data, mutably.
    pub fn data_mut(&mut self, node: usize) -> Option<&mut T> {
        self.nodes.get_mut(node).map(|node| &mut node.data)
    }

    /// Whether a node has children.
    #[must_use]
    pub fn has_children(&self, node: usize) -> bool {
        self.nodes
            .get(node)
            .is_some_and(|node| !node.children.is_empty())
    }

    /// Whether a node is expanded.
    #[must_use]
    pub fn is_expanded(&self, node: usize) -> bool {
        self.nodes.get(node).is_some_and(|node| node.expanded)
    }

    /// Sets the row height used by [`Self::row_at`].
    pub fn set_row_height(&mut self, row_height: f32) {
        self.row_height = row_height.max(1.0);
    }

    /// The number of visible rows.
    #[must_use]
    pub fn visible_len(&self) -> usize {
        self.visible.len()
    }

    /// The visible rows, as arena indices.
    #[must_use]
    pub fn visible(&self) -> &[usize] {
        &self.visible
    }

    /// Copies the visible rows into `out`, reusing its capacity.
    ///
    /// `O(visible)` and allocation-free when `out` has room.
    pub fn flatten_into(&self, out: &mut Vec<Row>) {
        out.clear();
        out.reserve(self.visible.len());
        for (flat_index, &node) in self.visible.iter().enumerate() {
            let Some(entry) = self.nodes.get(node) else {
                continue;
            };
            out.push(Row {
                node,
                depth: self.depth(node),
                flat_index,
                has_children: !entry.children.is_empty(),
                expanded: entry.expanded,
                selected: self.selected == Some(flat_index),
            });
        }
    }

    /// The row under `y` in the viewport at `offset`.
    #[must_use]
    pub fn row_at(&self, y: f32, offset: f32) -> Option<usize> {
        if !y.is_finite() || y < 0.0 {
            return None;
        }
        let index = ((y + offset.max(0.0)) / self.row_height).floor() as usize;
        (index < self.visible.len()).then_some(index)
    }

    /// The scroll offset that reveals the selected row.
    #[must_use]
    pub fn scroll_to_selection(&self, viewport_height: f32) -> f32 {
        let Some(index) = self.selected else {
            return 0.0;
        };
        let y0 = index as f32 * self.row_height;
        let y1 = y0 + self.row_height;
        if y1 > viewport_height {
            (y1 - viewport_height).max(0.0)
        } else {
            0.0
        }
    }

    /// The selected row, as a flat index.
    #[must_use]
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// The selected arena node.
    #[must_use]
    pub fn selected_node(&self) -> Option<usize> {
        self.selected
            .and_then(|index| self.visible.get(index).copied())
    }

    /// Selects a visible row.
    pub fn select(&mut self, flat_index: usize) -> bool {
        if flat_index >= self.visible.len() || self.selected == Some(flat_index) {
            return false;
        }
        self.selected = Some(flat_index);
        true
    }

    /// Expands or collapses a node; returns whether the state changed.
    pub fn toggle(&mut self, node: usize) -> bool {
        if !self.has_children(node) {
            return false;
        }
        let selected_node = self.selected_node();
        let Some(entry) = self.nodes.get_mut(node) else {
            return false;
        };
        entry.expanded = !entry.expanded;
        self.rebuild_visible();
        if let Some(selected) = selected_node {
            self.selected = self
                .visible
                .iter()
                .position(|candidate| *candidate == selected);
        }
        true
    }

    /// Selects the first row after a fresh tree.
    pub fn select_first(&mut self) -> bool {
        if self.visible.is_empty() {
            self.selected = None;
            return false;
        }
        self.selected = Some(0);
        true
    }

    /// Applies a navigation key; returns whether the selection changed.
    ///
    /// Up/Down/Home/End are `O(1)` (the flat index is kept); Left/Right touch
    /// one node; paging is `O(1)`.
    pub fn move_selection(&mut self, key: TreeKey) -> bool {
        if self.visible.is_empty() {
            self.selected = None;
            return false;
        }
        let current = self.selected.unwrap_or(0);
        let next = match key {
            TreeKey::Up => current.saturating_sub(1),
            TreeKey::Down => (current + 1).min(self.visible.len() - 1),
            TreeKey::Home => 0,
            TreeKey::End => self.visible.len() - 1,
            TreeKey::PageUp => current.saturating_sub(10),
            TreeKey::PageDown => (current + 10).min(self.visible.len() - 1),
            TreeKey::Enter => {
                let node = self.visible[current];
                let changed = self.toggle(node);
                if changed {
                    self.selected = self
                        .visible
                        .iter()
                        .position(|candidate| *candidate == node)
                        .or(Some(current));
                }
                return changed;
            }
            TreeKey::Left => {
                let node = self.visible[current];
                if self.has_children(node) && self.is_expanded(node) {
                    self.toggle(node);
                    return true;
                }
                let parent = self.nodes[node].parent;
                match parent.and_then(|parent| self.visible.iter().position(|id| *id == parent)) {
                    Some(parent_row) => parent_row,
                    None => current,
                }
            }
            TreeKey::Right => {
                let node = self.visible[current];
                if self.has_children(node) && !self.is_expanded(node) {
                    self.toggle(node);
                    return true;
                }
                if self.has_children(node) {
                    current + 1
                } else {
                    current
                }
            }
        };
        let changed = next != current;
        self.selected = Some(next);
        changed
    }

    /// Moves the selection to the next visible row starting with `ch`.
    ///
    /// The buffer resets after `type_ahead_ms`; a case-insensitive prefix
    /// match. `O(visible)` worst case, no allocation (the buffer is reused).
    pub fn type_ahead(&mut self, ch: char, now_ms: f64) -> bool {
        if self.visible.is_empty() {
            return false;
        }
        if now_ms - self.type_ahead_at > self.type_ahead_ms {
            self.type_ahead.clear();
        }
        self.type_ahead_at = now_ms;
        self.type_ahead.push(ch);
        let needle = self.type_ahead.as_str();
        let start = self.selected.map_or(0, |row| row + 1);
        for step in 0..self.visible.len() {
            let row = (start + step) % self.visible.len();
            let node = self.visible[row];
            if label_starts_with(&self.nodes[node].label, needle) {
                let changed = self.selected != Some(row);
                self.selected = Some(row);
                return changed;
            }
        }
        false
    }

    /// The type-ahead buffer (for the debug overlay).
    #[must_use]
    pub fn type_ahead_buffer(&self) -> &str {
        &self.type_ahead
    }

    fn depth(&self, node: usize) -> u16 {
        self.nodes[node].depth
    }

    fn rebuild_visible(&mut self) {
        self.visible.clear();
        let Some(root) = self.root else {
            self.selected = None;
            return;
        };
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            self.visible.push(node);
            if self.nodes[node].expanded {
                for child in self.nodes[node].children.iter().rev() {
                    stack.push(*child);
                }
            }
        }
        if self
            .selected
            .is_some_and(|index| index >= self.visible.len())
        {
            self.selected = None;
        }
    }
}

fn label_starts_with(label: &str, needle: &str) -> bool {
    label
        .get(..needle.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // reason: exact token arithmetic in tests
    use super::{Row, Tree, TreeKey, TreeNode};

    fn sample() -> Tree<u32> {
        Tree::with_root(
            TreeNode::new("scene", 0)
                .expanded(true)
                .add_child(
                    TreeNode::new("group", 1)
                        .expanded(true)
                        .add_child(TreeNode::new("cube", 2))
                        .add_child(TreeNode::new("sphere", 3)),
                )
                .add_child(TreeNode::new("camera", 4)),
        )
    }

    fn rows(tree: &Tree<u32>) -> Vec<Row> {
        let mut out = Vec::new();
        tree.flatten_into(&mut out);
        out
    }

    #[test]
    fn flatten_follows_expansion() {
        let mut tree = sample();
        assert_eq!(tree.visible_len(), 5);
        let group = tree.visible()[1];
        assert!(tree.toggle(group));
        assert_eq!(tree.visible_len(), 3);
        assert!(tree.toggle(group));
        assert_eq!(tree.visible_len(), 5);
        let rows = rows(&tree);
        assert_eq!(rows[2].depth, 2);
        assert_eq!(rows[2].node, tree.visible()[2]);
    }

    #[test]
    fn navigation_moves_one_row_and_pages() {
        let mut tree = sample();
        tree.select_first();
        assert!(tree.move_selection(TreeKey::Down));
        assert_eq!(tree.selected(), Some(1));
        assert!(tree.move_selection(TreeKey::End));
        assert_eq!(tree.selected(), Some(4));
        assert!(!tree.move_selection(TreeKey::Down));
        assert!(tree.move_selection(TreeKey::PageUp));
        assert_eq!(tree.selected(), Some(0));
        assert!(!tree.move_selection(TreeKey::Up));
    }

    #[test]
    fn left_collapses_then_goes_to_parent() {
        let mut tree = sample();
        tree.select_first();
        tree.move_selection(TreeKey::Down);
        // group is expanded: Left collapses it.
        assert!(tree.move_selection(TreeKey::Left));
        assert_eq!(tree.visible_len(), 3);
        // Left again moves to the parent (scene).
        assert!(tree.move_selection(TreeKey::Left));
        assert_eq!(tree.selected(), Some(0));
        // Root has no parent row; Left keeps the selection.
        tree.select(0);
        tree.move_selection(TreeKey::Left);
        assert_eq!(tree.selected(), Some(0));
    }

    #[test]
    fn right_expands_then_moves_to_the_child() {
        let mut tree = sample();
        let group = tree.visible()[1];
        tree.toggle(group);
        tree.select(1);
        assert!(tree.move_selection(TreeKey::Right));
        assert_eq!(tree.visible_len(), 5);
        assert!(tree.move_selection(TreeKey::Right));
        assert_eq!(tree.selected(), Some(2));
        assert!(!tree.move_selection(TreeKey::Right));
    }

    #[test]
    fn enter_toggles_a_branch() {
        let mut tree = sample();
        tree.select(1);
        assert!(tree.move_selection(TreeKey::Enter));
        assert_eq!(tree.visible_len(), 3);
        assert_eq!(tree.selected(), Some(1));
    }

    #[test]
    fn type_ahead_matches_the_next_row_and_expires() {
        let mut tree = sample();
        tree.select_first();
        assert!(tree.type_ahead('s', 0.0));
        assert_eq!(tree.selected(), Some(3));
        assert!(!tree.type_ahead('s', 100.0));
        assert!(tree.type_ahead('c', 1000.0));
        assert_eq!(tree.type_ahead_buffer(), "c");
        assert_eq!(tree.selected(), Some(4));
        assert!(tree.type_ahead('g', 1810.0));
        assert_eq!(tree.type_ahead_buffer(), "g");
        assert_eq!(tree.label(tree.selected_node().unwrap()), Some("group"));
    }

    #[test]
    fn row_at_and_reveal() {
        let mut tree = sample();
        tree.set_row_height(20.0);
        assert_eq!(tree.row_at(0.0, 0.0), Some(0));
        assert_eq!(tree.row_at(45.0, 0.0), Some(2));
        assert_eq!(tree.row_at(0.0, 100.0), None);
        tree.select(3);
        assert_eq!(tree.scroll_to_selection(40.0), 40.0);
        tree.select(1);
        assert_eq!(tree.scroll_to_selection(40.0), 0.0);
    }

    #[test]
    fn a_buffer_is_reused() {
        let tree = sample();
        let mut out: Vec<Row> = Vec::with_capacity(8);
        let capacity = out.capacity();
        tree.flatten_into(&mut out);
        tree.flatten_into(&mut out);
        assert_eq!(out.capacity(), capacity);
        assert_eq!(out.len(), 5);
    }

    #[test]
    fn an_empty_tree_is_safe() {
        let mut tree = Tree::<u32>::new();
        assert!(tree.is_empty());
        assert!(!tree.select_first());
        assert!(!tree.move_selection(TreeKey::Down));
        assert!(!tree.type_ahead('a', 0.0));
        assert_eq!(tree.scroll_to_selection(100.0), 0.0);
        assert_eq!(tree.selected_node(), None);
    }

    #[test]
    fn a_deep_chain_does_not_recurse() {
        let mut node = TreeNode::new("leaf", 0u32).expanded(true);
        for depth in 1..10_000u32 {
            node = TreeNode::new(format!("node {depth}"), depth)
                .expanded(true)
                .add_child(node);
        }
        let tree = Tree::with_root(node);
        assert_eq!(tree.visible_len(), 10_000);
        let mut out = Vec::new();
        tree.flatten_into(&mut out);
        assert_eq!(out[9_999].depth, 9_999);
    }
}
