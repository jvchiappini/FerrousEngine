//! Focus order: a precomputed array of focusable nodes, rebuilt only when the
//! tree structure changes.
//!
//! `Tab`/`Shift+Tab` walk this array instead of the tree, so a keystroke is
//! `O(1)` and never allocates. The order is the tree's pre-order (parents
//! before children, siblings left to right), with `tab_index` overriding the
//! position the way HTML does: positive indices first (ascending), then the
//! natural order of the nodes without one, then negative indices (descending),
//! because a negative index removes a node from the natural sequence.
//!
//! A focus scope (a dialog) restricts the array to one subtree; a trapping
//! scope wraps at its ends instead of leaving.

use std::collections::HashMap;

use crate::view::{NodeId, Tree};

/// One focus scope: a subtree that owns the tab order while it is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusScope {
    /// The subtree root.
    pub root: NodeId,
    /// Whether Tab wraps inside the scope instead of leaving it.
    pub trap: bool,
}

/// The precomputed focus order.
#[derive(Clone, Debug, Default)]
pub struct FocusOrder {
    order: Vec<NodeId>,
    index: HashMap<NodeId, usize>,
    scope: Option<NodeId>,
    rebuilt: u64,
}

impl FocusOrder {
    /// Builds an empty order.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds the order for the current tree and scope.
    ///
    /// `O(n log n)` in the number of focusable nodes (the `tab_index` sort);
    /// the common case (no explicit index) is already in tree order.
    pub fn rebuild<Msg>(&mut self, tree: &Tree<Msg>, scope: Option<FocusScope>) {
        self.rebuilt = self.rebuilt.wrapping_add(1);
        self.order.clear();
        self.index.clear();
        self.scope = scope.map(|scope| scope.root);

        let in_scope = |id: NodeId| scope.is_none_or(|scope| is_descendant(tree, id, scope.root));
        let mut natural: Vec<NodeId> = Vec::with_capacity(tree.nodes.len());
        for node in &tree.nodes {
            if !node.interactive
                || node.style.disabled
                || node.style.hidden
                || node.style.opacity <= 0.0
            {
                continue;
            }
            if !in_scope(node.id) {
                continue;
            }
            natural.push(node.id);
        }
        if natural
            .iter()
            .any(|id| tree.nodes[id.index()].style.tab_index.is_some())
        {
            let mut keyed: Vec<(u8, i32, usize, NodeId)> = natural
                .iter()
                .enumerate()
                .map(|(order, id)| {
                    let declared = tree.nodes[id.index()].style.tab_index;
                    let (group, value) = match declared {
                        Some(value) if value > 0 => (0, value),
                        Some(value) if value < 0 => (2, -value),
                        _ => (1, 0),
                    };
                    (group, value, order, *id)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
            self.order = keyed.into_iter().map(|(_, _, _, id)| id).collect();
        } else {
            self.order = natural;
        }
        self.index.reserve(self.order.len());
        for (position, id) in self.order.iter().enumerate() {
            self.index.insert(*id, position);
        }
    }

    /// The focusable nodes, in tab order.
    #[must_use]
    pub fn nodes(&self) -> &[NodeId] {
        &self.order
    }

    /// The number of focusable nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Whether nothing is focusable.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// The position of a node in the order.
    #[must_use]
    pub fn position(&self, id: NodeId) -> Option<usize> {
        self.index.get(&id).copied()
    }

    /// The first focusable node.
    #[must_use]
    pub fn first(&self) -> Option<NodeId> {
        self.order.first().copied()
    }

    /// The last focusable node.
    #[must_use]
    pub fn last(&self) -> Option<NodeId> {
        self.order.last().copied()
    }

    /// The next focusable node after `current`, or the first when moving
    /// forward from the end (or the last when moving backward from the start).
    ///
    /// Inside a trapping scope the walk wraps; otherwise it clamps so focus
    /// cannot escape the scope by keyboard.
    #[must_use]
    pub fn step(&self, current: Option<NodeId>, forward: bool) -> Option<NodeId> {
        if self.order.is_empty() {
            return None;
        }
        let position = current.and_then(|id| self.position(id));
        let next = match (position, forward) {
            (Some(position), true) => Some((position + 1) % self.order.len()),
            (Some(position), false) => Some(if position == 0 {
                self.order.len() - 1
            } else {
                position - 1
            }),
            (None, true) => Some(0),
            (None, false) => Some(self.order.len() - 1),
        };
        next.map(|position| self.order[position])
    }

    /// The rebuilt generation.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.rebuilt
    }

    /// The active scope root.
    #[must_use]
    pub fn scope(&self) -> Option<NodeId> {
        self.scope
    }
}

fn is_descendant<Msg>(tree: &Tree<Msg>, id: NodeId, root: NodeId) -> bool {
    let mut current = Some(id);
    while let Some(candidate) = current {
        if candidate == root {
            return true;
        }
        current = tree.node(candidate).and_then(|node| node.parent);
    }
    false
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // reason: exact token arithmetic in tests
    use super::{FocusOrder, FocusScope};
    use crate::view::{Element, NodeId, Tree};

    #[derive(Clone, Debug, PartialEq)]
    enum Msg {}

    fn tree() -> Tree<Msg> {
        Tree::build(
            &Element::<Msg>::column()
                .key("root")
                .child(Element::canvas().key("a").interactive())
                .child(
                    Element::row()
                        .key("group")
                        .child(Element::canvas().key("b").interactive())
                        .child(Element::canvas().key("c").interactive().disabled()),
                )
                .child(Element::canvas().key("d").interactive()),
        )
    }

    #[test]
    fn tabs_in_tree_order_and_skips_disabled() {
        let tree = tree();
        let mut order = FocusOrder::new();
        order.rebuild(&tree, None);
        let ids: Vec<NodeId> = order.nodes().to_vec();
        assert_eq!(ids.len(), 3);
        let a = tree.find_key("a").unwrap();
        let b = tree.find_key("b").unwrap();
        let d = tree.find_key("d").unwrap();
        assert_eq!(ids, vec![a, b, d]);
        assert_eq!(order.step(Some(a), true), Some(b));
        assert_eq!(order.step(Some(d), true), Some(a));
        assert_eq!(order.step(Some(a), false), Some(d));
    }

    #[test]
    fn positive_tab_index_comes_first() {
        let view: Element<Msg> = Element::column()
            .child(Element::canvas().key("a").interactive())
            .child(Element::canvas().key("b").interactive().tab_index(2))
            .child(Element::canvas().key("c").interactive().tab_index(1));
        let tree = Tree::build(&view);
        let mut order = FocusOrder::new();
        order.rebuild(&tree, None);
        let keys: Vec<&str> = order
            .nodes()
            .iter()
            .map(|id| match &tree.nodes[id.index()].key {
                Some(crate::view::Key::Static(name)) => *name,
                _ => "",
            })
            .collect();
        assert_eq!(keys, vec!["c", "b", "a"]);
    }

    #[test]
    fn a_scope_restricts_and_a_trap_wraps() {
        let tree = tree();
        let group = tree.find_key("group").unwrap();
        let mut order = FocusOrder::new();
        order.rebuild(
            &tree,
            Some(FocusScope {
                root: group,
                trap: true,
            }),
        );
        assert_eq!(order.len(), 1);
        let b = tree.find_key("b").unwrap();
        assert_eq!(order.step(Some(b), true), Some(b));
        assert_eq!(order.step(Some(b), false), Some(b));
    }

    #[test]
    fn empty_order_steps_to_nothing() {
        let tree = Tree::<Msg>::build(&Element::<Msg>::column());
        let mut order = FocusOrder::new();
        order.rebuild(&tree, None);
        assert!(order.is_empty());
        assert_eq!(order.step(None, true), None);
    }
}
