//! Hit testing: a uniform grid over the laid-out interactive rectangles.
//!
//! The grid itself is [`ferrous_core::spatial::SpatialIndex`], the one spatial
//! implementation the engine and `GUIMaker` share (prompt 15 phase 4B). This
//! module is the UI-specific adapter: it decides which nodes are candidates
//! (interactive, non-empty, clipped by every scroll/clip ancestor) and maps
//! the index's dense ids back to [`NodeId`].
//!
//! The index is rebuilt only when the layout changes, and a query reads the
//! single grid cell that contains the point, in front-to-back order, so a
//! click on a 10 000-node screen does not walk the tree. Clips are baked into
//! the stored rectangles: a child scrolled out of its container is not a
//! candidate. The last hit and its rect are cached, so a pointer move that
//! stays inside the same element does no query at all.

use glam::Vec2;

use crate::geom::Rect;
use crate::view::{NodeId, Tree};

/// The spatial index of interactive rectangles.
#[derive(Default)]
pub struct HitIndex {
    /// The UI rectangles, front-to-back (the first entry is on top).
    pub nodes: Vec<(NodeId, Rect)>,
    index: ferrous_core::spatial::SpatialIndex,
    entries: Vec<(u32, ferrous_core::spatial::Aabb)>,
    rebuilds: u64,
}

impl HitIndex {
    /// Builds an empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds the index from the laid-out tree.
    ///
    /// `O(nodes + covered cells)`; only this rebuild allocates.
    pub fn rebuild<Msg>(&mut self, tree: &Tree<Msg>, rects: &[Rect]) {
        self.rebuilds += 1;
        self.nodes.clear();
        for node in &tree.nodes {
            if !node.interactive {
                continue;
            }
            let Some(rect) = rects.get(node.id.index()).copied() else {
                continue;
            };
            if rect.is_empty() {
                continue;
            }
            if let Some(visible) = visible_rect(tree, rects, node.id) {
                self.nodes.push((node.id, visible));
            }
        }
        // Pre-order visits parents before children; hit testing wants the
        // last painted (topmost) first.
        self.nodes.reverse();
        self.entries.clear();
        self.entries.reserve(self.nodes.len());
        // The core index stores paint order (later = on top), so feed it back
        // to front. `nodes` stays front-to-back for the debug view.
        for (id, rect) in self.nodes.iter().rev() {
            self.entries.push((
                id.0,
                ferrous_core::spatial::Aabb::from_origin_size(
                    Vec2::new(rect.x, rect.y),
                    Vec2::new(rect.width, rect.height),
                ),
            ));
        }
        self.index.rebuild(&self.entries);
    }

    /// Sets the layout generation; a cached hit is only reused within it.
    pub fn set_generation(&mut self, generation: u64) {
        self.index.set_generation(generation);
    }

    /// The topmost interactive node containing `point`.
    pub fn pick(&mut self, point: Vec2, generation: u64) -> Option<NodeId> {
        self.index.pick(point, generation).map(NodeId)
    }

    /// Allocation-free iterator over the candidates at a point.
    #[inline]
    pub fn query(&self, point: Vec2) -> impl Iterator<Item = NodeId> + '_ {
        self.index.query_point(point).map(NodeId)
    }

    /// Rebuilds this frame's index.
    #[must_use]
    pub fn rebuilds(&self) -> u64 {
        self.rebuilds
    }

    /// The number of point queries performed.
    #[must_use]
    pub fn queries(&self) -> u64 {
        self.index.queries()
    }

    /// The number of point queries answered from the last-hit cache.
    #[must_use]
    pub fn cache_hits(&self) -> u64 {
        self.index.cache_hits()
    }

    /// Clears the last-hit cache (a layout change).
    pub fn invalidate(&mut self) {
        self.index.invalidate();
    }
}

/// A node's rect intersected with every clipping ancestor.
fn visible_rect<Msg>(tree: &Tree<Msg>, rects: &[Rect], id: NodeId) -> Option<Rect> {
    let node = tree.node(id)?;
    let mut visible = *rects.get(id.index())?;
    let mut parent = node.parent;
    while let Some(parent_id) = parent {
        let parent_node = tree.node(parent_id)?;
        if parent_node.style.clip || parent_node.style.scroll.any(|axes| axes.clips()) {
            let parent_rect = *rects.get(parent_id.index())?;
            visible = visible.intersect(parent_rect);
            if visible.is_empty() {
                return None;
            }
        }
        parent = parent_node.parent;
    }
    (!visible.is_empty()).then_some(visible)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // reason: exact layout rectangles in tests

    use super::HitIndex;
    use crate::geom::Rect;
    use crate::style::ScrollAxes;
    use crate::view::{Element, Tree};
    use glam::Vec2;

    #[derive(Clone, Debug, PartialEq)]
    enum Msg {}

    fn rects(tree: &Tree<Msg>) -> Vec<Rect> {
        // A simple layout stand-in: every node gets the same rect, which keeps
        // this test about the index, not taffy.
        tree.nodes
            .iter()
            .map(|_node| Rect::new(0.0, 0.0, 200.0, 200.0))
            .collect()
    }

    #[test]
    fn picks_topmost_and_respects_clips() {
        let view: Element<Msg> = Element::column()
            .key("root")
            .child(Element::scroll(ScrollAxes::Vertical).key("scroll"))
            .child(Element::canvas().key("top"));
        let tree = Tree::build(&view);
        let rects = rects(&tree);
        let mut index = HitIndex::new();
        index.rebuild(&tree, &rects);
        let top = tree.find_key("top").expect("top");
        assert_eq!(index.pick(Vec2::new(10.0, 10.0), 0), Some(top));
    }

    #[test]
    fn last_hit_is_cached_within_a_generation() {
        let view: Element<Msg> = Element::column()
            .key("root")
            .child(Element::canvas().key("button"));
        let tree = Tree::build(&view);
        let rects = rects(&tree);
        let mut index = HitIndex::new();
        index.rebuild(&tree, &rects);
        let button = tree.find_key("button").expect("button");
        assert_eq!(index.pick(Vec2::new(10.0, 10.0), 1), Some(button));
        let before = index.queries();
        assert_eq!(index.pick(Vec2::new(11.0, 11.0), 1), Some(button));
        assert_eq!(index.queries(), before);
        assert_eq!(index.cache_hits(), 1);
    }
}
