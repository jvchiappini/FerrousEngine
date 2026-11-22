//! A uniform-grid spatial index shared by GUIMaker routing and scene picking.
//!
//! The index maps a dense `u32` id to an axis-aligned box. Entries are stored
//! in **paint order** (later entries are on top); a point query walks the grid
//! cell that contains the point from the last entry to the first, so the first
//! candidate that contains the point is the topmost one. A rectangle query
//! collects candidates from every covered cell, de-duplicating them with a
//! reused stamp buffer.
//!
//! Both queries are `O(cell)` for a point and `O(cells + candidates)` for a
//! rectangle, never `O(entries)`, and neither allocates after the index has
//! been built with its final capacity. The index is rebuilt only when geometry
//! changes.
//!
//! This is the one implementation: `guimaker::spatial` wraps it for UI routing
//! and `ferrous_2d::picking` uses it for scene picks. No engine crate may
//! depend on GUIMaker, and GUIMaker may use this GPU-free foundation crate.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)] // reason: grid indices and cell counts are small and bounded

use glam::Vec2;

/// An axis-aligned box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Aabb {
    /// Minimum corner.
    pub min: Vec2,
    /// Maximum corner.
    pub max: Vec2,
}

impl Aabb {
    /// The empty box at the origin.
    pub const ZERO: Self = Self {
        min: Vec2::ZERO,
        max: Vec2::ZERO,
    };

    /// A box from a minimum and a maximum corner (in any order).
    #[must_use]
    pub fn from_min_max(min: Vec2, max: Vec2) -> Self {
        Self {
            min: min.min(max),
            max: min.max(max),
        }
    }

    /// A box from an origin and a size.
    #[must_use]
    pub fn from_origin_size(origin: Vec2, size: Vec2) -> Self {
        Self {
            min: origin,
            max: origin + size,
        }
    }

    /// A box around four points.
    #[must_use]
    pub fn from_points(points: &[Vec2]) -> Self {
        let mut bounds = Self {
            min: Vec2::splat(f32::INFINITY),
            max: Vec2::splat(f32::NEG_INFINITY),
        };
        for point in points {
            bounds.min = bounds.min.min(*point);
            bounds.max = bounds.max.max(*point);
        }
        if !bounds.is_finite() {
            Self::ZERO
        } else {
            bounds
        }
    }

    /// The union of two boxes.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    /// The intersection, or an empty box when they do not overlap.
    #[must_use]
    pub fn intersect(self, other: Self) -> Self {
        let min = self.min.max(other.min);
        let max = self.max.min(other.max);
        if min.x > max.x || min.y > max.y {
            Self::ZERO
        } else {
            Self { min, max }
        }
    }

    /// Whether the box is empty or non-finite.
    #[must_use]
    pub fn is_empty(self) -> bool {
        !self.is_finite() || self.max.x <= self.min.x || self.max.y <= self.min.y
    }

    /// Whether both corners are finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.min.is_finite() && self.max.is_finite()
    }

    /// Whether the point is inside (edges included).
    #[must_use]
    pub fn contains(self, point: Vec2) -> bool {
        point.x >= self.min.x
            && point.y >= self.min.y
            && point.x <= self.max.x
            && point.y <= self.max.y
    }

    /// Whether two boxes overlap (touching edges count).
    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        self.min.x <= other.max.x
            && other.min.x <= self.max.x
            && self.min.y <= other.max.y
            && other.min.y <= self.max.y
    }

    /// The width and height.
    #[must_use]
    pub fn size(self) -> Vec2 {
        self.max - self.min
    }

    /// The centre.
    #[must_use]
    pub fn center(self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    /// The box grown by `amount` on every side (negative shrinks).
    #[must_use]
    pub fn expand(self, amount: f32) -> Self {
        Self {
            min: self.min - Vec2::splat(amount),
            max: self.max + Vec2::splat(amount),
        }
    }
}

/// Trees at or below this size are cheap enough to scan linearly.
const LINEAR_SCAN_THRESHOLD: usize = 16;
/// Smallest grid cell edge, in logical units.
const MIN_CELL_SIZE: f32 = 16.0;
/// Upper bound on grid cells to keep rebuild cost and memory bounded.
const MAX_CELLS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Entry {
    id: u32,
    bounds: Aabb,
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

    fn contains(&self, point: Vec2) -> bool {
        if self.is_empty() {
            return false;
        }
        let width = self.cols as f32 * self.cell_size;
        let height = self.rows as f32 * self.cell_size;
        point.x >= self.origin.x
            && point.y >= self.origin.y
            && point.x <= self.origin.x + width
            && point.y <= self.origin.y + height
    }

    fn cell_coord(&self, x: f32, y: f32) -> (usize, usize) {
        let cx = ((x - self.origin.x) / self.cell_size).floor();
        let cy = ((y - self.origin.y) / self.cell_size).floor();
        (clamp_coord(cx, self.cols), clamp_coord(cy, self.rows))
    }

    fn range_coords(&self, bounds: Aabb) -> ((usize, usize), (usize, usize)) {
        (
            self.cell_coord(bounds.min.x, bounds.min.y),
            self.cell_coord(bounds.max.x, bounds.max.y),
        )
    }
}

fn clamp_coord(value: f32, count: usize) -> usize {
    if !value.is_finite() || count == 0 {
        return 0;
    }
    let max = count.saturating_sub(1) as f32;
    value.max(0.0).min(max) as usize
}

/// The uniform-grid index.
#[derive(Default)]
pub struct SpatialIndex {
    entries: Vec<Entry>,
    grid: Grid,
    last: Option<(u32, Aabb, u64)>,
    rebuilds: u64,
    queries: u64,
    cache_hits: u64,
    visited: Vec<u32>,
    stamp: u32,
    by_id: Vec<u32>,
}

enum CandidateSource<'a> {
    All,
    Cell(&'a [u32]),
}

struct Candidates<'a> {
    point: Vec2,
    entries: &'a [Entry],
    source: CandidateSource<'a>,
    cursor: usize,
}

impl Iterator for Candidates<'_> {
    type Item = u32;

    #[inline]
    fn next(&mut self) -> Option<u32> {
        loop {
            let index = match self.source {
                CandidateSource::All => {
                    if self.cursor >= self.entries.len() {
                        return None;
                    }
                    self.entries.len() - 1 - self.cursor
                }
                CandidateSource::Cell(entries) => {
                    let position = entries.len().checked_sub(1 + self.cursor)?;
                    *entries.get(position)? as usize
                }
            };
            self.cursor += 1;
            if let Some(entry) = self.entries.get(index) {
                if entry.bounds.contains(self.point) {
                    return Some(entry.id);
                }
            }
        }
    }
}

impl SpatialIndex {
    /// Builds an empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds the index from entries in paint order (later = on top).
    ///
    /// `O(entries + covered cells)` and allocates only when the entry or cell
    /// counts grow.
    pub fn rebuild(&mut self, entries: &[(u32, Aabb)]) {
        self.rebuilds += 1;
        self.entries.clear();
        self.entries.reserve(entries.len());
        for (id, bounds) in entries {
            if bounds.is_empty() || !bounds.is_finite() {
                continue;
            }
            self.entries.push(Entry {
                id: *id,
                bounds: *bounds,
            });
        }
        self.build_by_id();
        self.build_grid();
        self.last = None;
    }

    /// The number of indexed entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The bounds stored for `id`, when it is indexed.
    #[must_use]
    pub fn bounds(&self, id: u32) -> Option<Aabb> {
        let index = self.entry_index(id)?;
        self.entries.get(index).map(|entry| entry.bounds)
    }

    /// Sets the generation; a cached hit is reused only within it.
    pub fn set_generation(&mut self, generation: u64) {
        if self.last.is_some_and(|(_, _, cached)| cached != generation) {
            self.last = None;
        }
    }

    /// The topmost indexed id whose box contains `point`.
    ///
    /// A hit that stays inside the last returned box and generation is served
    /// from the cache without touching the grid.
    pub fn pick(&mut self, point: Vec2, generation: u64) -> Option<u32> {
        if let Some((id, bounds, cached)) = self.last {
            if cached == generation && bounds.contains(point) {
                self.cache_hits += 1;
                return Some(id);
            }
        }
        self.queries += 1;
        let id = self.query_point(point).next()?;
        let bounds = self.bounds(id).unwrap_or(Aabb::ZERO);
        self.last = Some((id, bounds, generation));
        Some(id)
    }

    /// Extraction-free iterator over the ids at a point, topmost first.
    #[inline]
    pub fn query_point(&self, point: Vec2) -> impl Iterator<Item = u32> + '_ {
        let source = if self.entries.len() <= LINEAR_SCAN_THRESHOLD || self.grid.is_empty() {
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
            entries: &self.entries,
            source,
            cursor: 0,
        }
    }

    /// Fills `out` with the ids whose boxes intersect `area`.
    ///
    /// The order is unspecified (callers sort by their own z if it matters);
    /// the de-duplication stamps are reused, so a warm query allocates nothing.
    pub fn query_rect_into(&mut self, area: Aabb, out: &mut Vec<u32>) {
        out.clear();
        if area.is_empty() || !area.is_finite() || self.entries.is_empty() {
            return;
        }
        self.queries += 1;
        if self.entries.len() <= LINEAR_SCAN_THRESHOLD || self.grid.is_empty() {
            for entry in self.entries.iter().rev() {
                if entry.bounds.intersects(area) {
                    out.push(entry.id);
                }
            }
            return;
        }
        if self.visited.len() < self.entries.len() {
            self.visited.resize(self.entries.len(), u32::MAX);
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == u32::MAX {
            self.visited.fill(0);
            self.stamp = 1;
        }
        let stamp = self.stamp;
        if area.contains(self.grid.origin)
            && area.max.x >= self.grid.origin.x + self.grid.cols as f32 * self.grid.cell_size
            && area.max.y >= self.grid.origin.y + self.grid.rows as f32 * self.grid.cell_size
        {
            for entry in self.entries.iter().rev() {
                if entry.bounds.intersects(area) {
                    out.push(entry.id);
                }
            }
            return;
        }
        let ((x0, y0), (x1, y1)) = self.grid.range_coords(area.intersect(Aabb {
            min: self.grid.origin,
            max: self.grid.origin
                + Vec2::new(
                    self.grid.cols as f32 * self.grid.cell_size,
                    self.grid.rows as f32 * self.grid.cell_size,
                ),
        }));
        for cy in y0..=y1 {
            let row = cy * self.grid.cols;
            for cx in x0..=x1 {
                let cell = row + cx;
                let start = self.grid.starts[cell] as usize;
                let end = self.grid.starts[cell + 1] as usize;
                for &index in self.grid.entries[start..end].iter().rev() {
                    let index = index as usize;
                    if self.visited[index] == stamp {
                        continue;
                    }
                    self.visited[index] = stamp;
                    if let Some(entry) = self.entries.get(index) {
                        if entry.bounds.intersects(area) {
                            out.push(entry.id);
                        }
                    }
                }
            }
        }
    }

    fn build_by_id(&mut self) {
        self.by_id.clear();
        let max_id = self.entries.iter().map(|entry| entry.id).max();
        let Some(max_id) = max_id else {
            return;
        };
        let max_id = max_id as usize;
        if max_id > (self.entries.len() * 8).max(4096) {
            return;
        }
        self.by_id.resize(max_id + 1, 0);
        for (index, entry) in self.entries.iter().enumerate() {
            self.by_id[entry.id as usize] = index as u32 + 1;
        }
    }

    fn entry_index(&self, id: u32) -> Option<usize> {
        let slot = self.by_id.get(id as usize).copied().unwrap_or(0);
        if slot != 0 {
            return Some(slot as usize - 1);
        }
        self.entries.iter().position(|entry| entry.id == id)
    }

    /// The number of rebuilds performed.
    #[must_use]
    pub fn rebuilds(&self) -> u64 {
        self.rebuilds
    }

    /// The number of point/rect queries performed.
    #[must_use]
    pub fn queries(&self) -> u64 {
        self.queries
    }

    /// The number of point queries answered from the last-hit cache.
    #[must_use]
    pub fn cache_hits(&self) -> u64 {
        self.cache_hits
    }

    /// Clears the last-hit cache (geometry changed).
    pub fn invalidate(&mut self) {
        self.last = None;
    }

    fn build_grid(&mut self) {
        let previous_cell_size = self.grid.cell_size;
        self.grid = Grid::default();
        let entry_count = self.entries.len();
        if entry_count <= LINEAR_SCAN_THRESHOLD {
            return;
        }

        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for entry in &self.entries {
            min = min.min(entry.bounds.min);
            max = max.max(entry.bounds.max);
        }
        let width = (max.x - min.x).max(1.0);
        let height = (max.y - min.y).max(1.0);
        if !width.is_finite() || !height.is_finite() {
            return;
        }

        let target = (width * height / entry_count as f32)
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

        for entry in &self.entries {
            let ((x0, y0), (x1, y1)) = self.grid.range_coords(entry.bounds);
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
        for (index, entry) in self.entries.iter().enumerate() {
            let ((x0, y0), (x1, y1)) = self.grid.range_coords(entry.bounds);
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

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // reason: exact grid arithmetic in tests

    use super::{Aabb, SpatialIndex};
    use glam::Vec2;

    fn boxed(x: f32, y: f32, w: f32, h: f32) -> Aabb {
        Aabb::from_origin_size(Vec2::new(x, y), Vec2::new(w, h))
    }

    #[test]
    fn aabb_union_intersection_and_containment() {
        let a = boxed(0.0, 0.0, 10.0, 10.0);
        let b = boxed(5.0, 5.0, 10.0, 10.0);
        assert!(a.intersects(b));
        assert!(a.contains(Vec2::new(5.0, 5.0)));
        assert_eq!(a.union(b), boxed(0.0, 0.0, 15.0, 15.0));
        assert_eq!(a.intersect(b), boxed(5.0, 5.0, 5.0, 5.0));
        assert!(a.intersect(boxed(20.0, 20.0, 1.0, 1.0)).is_empty());
        assert_eq!(
            Aabb::from_points(&[Vec2::new(2.0, 3.0), Vec2::new(-1.0, 9.0)]),
            boxed(-1.0, 3.0, 3.0, 6.0)
        );
    }

    #[test]
    fn picks_the_topmost_entry() {
        let mut index = SpatialIndex::new();
        index.rebuild(&[
            (1, boxed(0.0, 0.0, 100.0, 100.0)),
            (2, boxed(10.0, 10.0, 20.0, 20.0)),
        ]);
        assert_eq!(index.pick(Vec2::new(15.0, 15.0), 0), Some(2));
        assert_eq!(index.pick(Vec2::new(90.0, 90.0), 0), Some(1));
        assert_eq!(index.pick(Vec2::new(300.0, 300.0), 0), None);
    }

    #[test]
    fn the_last_hit_is_cached_within_a_generation() {
        let mut index = SpatialIndex::new();
        index.rebuild(&[(7, boxed(0.0, 0.0, 50.0, 50.0))]);
        assert_eq!(index.pick(Vec2::new(5.0, 5.0), 3), Some(7));
        let queries = index.queries();
        assert_eq!(index.pick(Vec2::new(6.0, 6.0), 3), Some(7));
        assert_eq!(index.queries(), queries);
        assert_eq!(index.cache_hits(), 1);
        index.invalidate();
        assert_eq!(index.pick(Vec2::new(6.0, 6.0), 3), Some(7));
        assert!(index.queries() > queries);
    }

    #[test]
    fn a_rect_query_returns_candidates_once() {
        let mut index = SpatialIndex::new();
        let mut items: Vec<(u32, Aabb)> = Vec::new();
        for i in 0..1000u32 {
            items.push((
                i,
                boxed((i % 40) as f32 * 20.0, (i / 40) as f32 * 20.0, 20.0, 20.0),
            ));
        }
        index.rebuild(&items);
        let mut out = Vec::new();
        index.query_rect_into(boxed(0.0, 0.0, 200.0, 100.0), &mut out);
        assert!(!out.is_empty());
        let mut sorted = out.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), out.len(), "no id may appear twice");
        let capacity = out.capacity();
        index.query_rect_into(boxed(400.0, 200.0, 100.0, 100.0), &mut out);
        assert_eq!(out.capacity(), capacity, "the buffer is reused");
    }

    #[test]
    fn a_full_extent_query_matches_the_linear_scan() {
        let mut index = SpatialIndex::new();
        let mut items: Vec<(u32, Aabb)> = Vec::new();
        for i in 0..500u32 {
            items.push((
                i,
                boxed((i % 25) as f32 * 10.0, (i / 25) as f32 * 10.0, 10.0, 10.0),
            ));
        }
        index.rebuild(&items);
        let mut out = Vec::new();
        index.query_rect_into(
            Aabb {
                min: Vec2::splat(-1000.0),
                max: Vec2::splat(1000.0),
            },
            &mut out,
        );
        assert_eq!(out.len(), 500);
    }

    #[test]
    fn empty_and_tiny_indexes_are_safe() {
        let mut index = SpatialIndex::new();
        index.rebuild(&[]);
        assert!(index.is_empty());
        assert_eq!(index.pick(Vec2::ZERO, 0), None);
        let mut out = Vec::new();
        index.query_rect_into(boxed(0.0, 0.0, 10.0, 10.0), &mut out);
        assert!(out.is_empty());
        index.rebuild(&[(1, Aabb::ZERO)]);
        assert!(index.is_empty(), "an empty box is not indexed");
    }

    #[test]
    fn a_huge_index_stays_bounded() {
        let mut index = SpatialIndex::new();
        let mut items: Vec<(u32, Aabb)> = Vec::with_capacity(100_000);
        for i in 0..100_000u32 {
            items.push((
                i,
                boxed((i % 400) as f32 * 8.0, (i / 400) as f32 * 8.0, 8.0, 8.0),
            ));
        }
        index.rebuild(&items);
        assert_eq!(index.len(), 100_000);
        assert_eq!(index.pick(Vec2::new(4.0, 4.0), 0), Some(0));
    }
}
