//! Hit testing: a uniform grid over the laid-out interactive rectangles.
//!
//! The index is rebuilt only when the layout changes, and a query reads the
//! single grid cell that contains the point, in front-to-back order, so a
//! click on a 10 000-node screen does not walk the tree. Clips are baked into
//! the stored rectangles: a child scrolled out of its container is not a
//! candidate. The last hit and its rect are cached, so a pointer move that
//! stays inside the same element does no query at all.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)] // reason: grid indices and cell counts are small and bounded
use glam::Vec2;

use crate::geom::Rect;
use crate::view::{NodeId, Tree};

/// Trees at or below this size are cheap enough to scan linearly.
const LINEAR_SCAN_THRESHOLD: usize = 16;
/// Smallest grid cell edge, in logical pixels.
const MIN_CELL_SIZE: f32 = 16.0;
/// Upper bound on grid cells to keep rebuild cost and memory bounded.
const MAX_CELLS: usize = 4096;

/// The spatial index of interactive rectangles.
#[derive(Default)]
pub struct HitIndex {
    /// The candidate rectangles, front-to-back.
    pub nodes: Vec<(NodeId, Rect)>,
    grid: Grid,
    last: Option<(NodeId, Rect, u64)>,
    rebuilds: u64,
    queries: u64,
    cache_hits: u64,
}

#[derive(Default)]
struct Grid {
    origin: Vec2,
    cell_size: f32,
    cols: usize,
    rows: usize,
    starts: Vec<u32>,
    entries: Vec<u32>,
}

impl Grid {
    fn is_empty(&self) -> bool {
        self.cols == 0 || self.rows == 0 || self.cell_size <= 0.0
    }

    fn contains(&self, pos: Vec2) -> bool {
        if self.is_empty() {
            return false;
        }
        let width = self.cols as f32 * self.cell_size;
        let height = self.rows as f32 * self.cell_size;
        pos.x >= self.origin.x
            && pos.y >= self.origin.y
            && pos.x <= self.origin.x + width
            && pos.y <= self.origin.y + height
    }

    fn cell_coord(&self, x: f32, y: f32) -> (usize, usize) {
        let cx = ((x - self.origin.x) / self.cell_size).floor();
        let cy = ((y - self.origin.y) / self.cell_size).floor();
        (clamp_coord(cx, self.cols), clamp_coord(cy, self.rows))
    }
}

enum CandidateSource<'a> {
    All,
    Cell(&'a [u32]),
}

struct Candidates<'a> {
    point: Vec2,
    nodes: &'a [(NodeId, Rect)],
    source: CandidateSource<'a>,
    cursor: usize,
}

impl Iterator for Candidates<'_> {
    type Item = NodeId;

    #[inline]
    fn next(&mut self) -> Option<NodeId> {
        loop {
            let index = match self.source {
                CandidateSource::All => {
                    if self.cursor >= self.nodes.len() {
                        return None;
                    }
                    self.cursor
                }
                CandidateSource::Cell(entries) => *entries.get(self.cursor)? as usize,
            };
            self.cursor += 1;
            if let Some((id, rect)) = self.nodes.get(index) {
                if rect.contains(self.point) {
                    return Some(*id);
                }
            }
        }
    }
}

fn clamp_coord(value: f32, count: usize) -> usize {
    if !value.is_finite() || count == 0 {
        return 0;
    }
    let max = count.saturating_sub(1) as f32;
    value.max(0.0).min(max) as usize
}

impl HitIndex {
    /// Builds an empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds the index from the laid-out tree.
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
        self.build_grid();
        self.last = None;
    }

    /// Sets the layout generation; a cached hit is only reused within it.
    pub fn set_generation(&mut self, generation: u64) {
        if self.last.is_some_and(|(_, _, cached)| cached != generation) {
            self.last = None;
        }
    }

    /// The topmost interactive node containing `point`.
    pub fn pick(&mut self, point: Vec2, generation: u64) -> Option<NodeId> {
        if let Some((id, rect, cached)) = self.last {
            if cached == generation && rect.contains(point) {
                self.cache_hits += 1;
                return Some(id);
            }
        }
        self.queries += 1;
        let id = self.query(point).next()?;
        let rect = self
            .nodes
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .map_or(Rect::ZERO, |(_, rect)| *rect);
        self.last = Some((id, rect, generation));
        Some(id)
    }

    /// Allocation-free iterator over the candidates at a point.
    #[inline]
    pub fn query(&self, point: Vec2) -> impl Iterator<Item = NodeId> + '_ {
        let source = if self.nodes.len() <= LINEAR_SCAN_THRESHOLD || self.grid.is_empty() {
            CandidateSource::All
        } else if !self.grid.contains(point) {
            CandidateSource::Cell(&[])
        } else {
            let (cx, cy) = self.grid.cell_coord(point.x, point.y);
            let cell = cy * self.grid.cols + cx;
            let start = self.grid.starts[cell] as usize;
            let end = self.grid.starts[cell + 1] as usize;
            CandidateSource::Cell(&self.grid.entries[start..end])
        };
        Candidates {
            point,
            nodes: &self.nodes,
            source,
            cursor: 0,
        }
    }

    /// Rebuilds this frame's index.
    #[must_use]
    pub fn rebuilds(&self) -> u64 {
        self.rebuilds
    }

    /// The number of point queries performed.
    #[must_use]
    pub fn queries(&self) -> u64 {
        self.queries
    }

    /// The number of point queries answered from the last-hit cache.
    #[must_use]
    pub fn cache_hits(&self) -> u64 {
        self.cache_hits
    }

    /// Clears the last-hit cache (a layout change).
    pub fn invalidate(&mut self) {
        self.last = None;
    }

    fn build_grid(&mut self) {
        let previous_cell_size = self.grid.cell_size;
        self.grid = Grid::default();
        let node_count = self.nodes.len();
        if node_count <= LINEAR_SCAN_THRESHOLD {
            return;
        }

        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for &(_, rect) in &self.nodes {
            min.x = min.x.min(rect.x);
            min.y = min.y.min(rect.y);
            max.x = max.x.max(rect.right());
            max.y = max.y.max(rect.bottom());
        }
        let width = (max.x - min.x).max(1.0);
        let height = (max.y - min.y).max(1.0);
        if !width.is_finite() || !height.is_finite() {
            return;
        }

        let target = (width * height / node_count as f32)
            .sqrt()
            .max(MIN_CELL_SIZE);
        let cell_size = if previous_cell_size > 0.0 {
            let ratio = (target * target) / (previous_cell_size * previous_cell_size);
            if (0.5..=2.0).contains(&ratio) {
                previous_cell_size
            } else {
                target
            }
        } else {
            target
        };
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return;
        }

        let mut cell_size = cell_size;
        let mut cols = ((width / cell_size).ceil() as usize).max(1);
        let mut rows = ((height / cell_size).ceil() as usize).max(1);
        while cols.saturating_mul(rows) > MAX_CELLS {
            let scale = (cols as f32 * rows as f32 / MAX_CELLS as f32)
                .sqrt()
                .max(1.01);
            cell_size *= scale;
            cols = ((width / cell_size).ceil() as usize).max(1);
            rows = ((height / cell_size).ceil() as usize).max(1);
        }
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return;
        }

        let cell_count = cols * rows;
        let mut counts = vec![0u32; cell_count];
        self.grid.origin = min;
        self.grid.cell_size = cell_size;
        self.grid.cols = cols;
        self.grid.rows = rows;

        for &(_, rect) in &self.nodes {
            let (x0, y0) = self.grid.cell_coord(rect.x, rect.y);
            let (x1, y1) = self.grid.cell_coord(rect.right(), rect.bottom());
            for cy in y0..=y1 {
                let row = cy * cols;
                for cx in x0..=x1 {
                    counts[row + cx] += 1;
                }
            }
        }

        let mut starts = vec![0u32; cell_count + 1];
        let mut total = 0u32;
        for (index, count) in counts.iter().enumerate() {
            starts[index] = total;
            total += *count;
        }
        starts[cell_count] = total;

        let mut cursor = starts.clone();
        let mut entries = vec![0u32; total as usize];
        for (index, &(_, rect)) in self.nodes.iter().enumerate() {
            let (x0, y0) = self.grid.cell_coord(rect.x, rect.y);
            let (x1, y1) = self.grid.cell_coord(rect.right(), rect.bottom());
            for cy in y0..=y1 {
                let row = cy * cols;
                for cx in x0..=x1 {
                    let cell = row + cx;
                    let slot = cursor[cell] as usize;
                    entries[slot] = index as u32;
                    cursor[cell] += 1;
                }
            }
        }
        self.grid.starts = starts;
        self.grid.entries = entries;
    }
}

/// A node's rect intersected with every clipping ancestor.
fn visible_rect<Msg>(tree: &Tree<Msg>, rects: &[Rect], id: NodeId) -> Option<Rect> {
    let node = tree.node(id)?;
    let mut visible = *rects.get(id.index())?;
    let mut parent = node.parent;
    while let Some(parent_id) = parent {
        let parent_node = tree.node(parent_id)?;
        if parent_node.style.clip || parent_node.style.scroll.clips() {
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
    use super::HitIndex;
    use crate::geom::Rect;
    use crate::style::ScrollAxes;
    use crate::view::{Element, Tree};
    use glam::Vec2;

    #[derive(Clone, Debug, PartialEq)]
    enum Msg {}

    fn rects(tree: &Tree<Msg>) -> Vec<Rect> {
        // A simple layout stand-in: every node gets the rect given by its
        // sibling index, which keeps this test about the index, not taffy.
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
