//! Chunk management — tracks which 40-m chunks exist in the world and which
//! were modified this frame.
//!
//! A "chunk" is a level-4 DAG cell (≈ 40 m × 40 m × 40 m).  The chunk
//! manager maintains a registry of all live chunks and a dirty set that
//! the GPU upload pass consumes each frame.

use std::collections::HashSet;

/// Axis-aligned bounding box of a chunk in level-0 world coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkAABB {
    /// Minimum corner (inclusive), in level-0 world cells.
    pub min: (i32, i32, i32),
    /// Maximum corner (exclusive), in level-0 world cells.
    pub max: (i32, i32, i32),
}

impl ChunkAABB {
    /// Construct from chunk grid coordinate and the world-cell size of a chunk.
    pub fn from_chunk_coord(cx: i32, cy: i32, cz: i32, chunk_cells: i32) -> Self {
        Self {
            min: (cx * chunk_cells, cy * chunk_cells, cz * chunk_cells),
            max: (
                cx * chunk_cells + chunk_cells,
                cy * chunk_cells + chunk_cells,
                cz * chunk_cells + chunk_cells,
            ),
        }
    }

    /// Returns `true` if the `(wx, wy, wz)` world coordinate is inside the AABB.
    #[inline]
    pub fn contains(&self, wx: i32, wy: i32, wz: i32) -> bool {
        wx >= self.min.0 && wx < self.max.0
            && wy >= self.min.1 && wy < self.max.1
            && wz >= self.min.2 && wz < self.max.2
    }
}

/// Manages live chunks and dirty tracking for GPU sync.
#[derive(Debug, Default)]
pub struct ChunkManager {
    /// All chunks that contain at least one solid voxel.
    live_chunks: HashSet<(i32, i32, i32)>,
    /// Chunks modified this frame.  Cleared by `take_dirty`.
    dirty_chunks: HashSet<(i32, i32, i32)>,
    /// Chunks that were fully emptied this frame (need GPU deallocation).
    removed_chunks: HashSet<(i32, i32, i32)>,
}

impl ChunkManager {
    /// Create an empty chunk manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a chunk as modified this frame.
    ///
    /// Also registers it as live (if not already).
    pub fn mark_dirty(&mut self, coord: (i32, i32, i32)) {
        self.live_chunks.insert(coord);
        self.dirty_chunks.insert(coord);
    }

    /// Mark a chunk as fully removed (all voxels deleted).
    pub fn mark_removed(&mut self, coord: (i32, i32, i32)) {
        self.live_chunks.remove(&coord);
        self.dirty_chunks.remove(&coord);
        self.removed_chunks.insert(coord);
    }

    /// Returns `true` if the chunk is currently live.
    #[inline]
    pub fn is_live(&self, coord: (i32, i32, i32)) -> bool {
        self.live_chunks.contains(&coord)
    }

    /// Total number of live chunks.
    #[inline]
    pub fn live_count(&self) -> usize {
        self.live_chunks.len()
    }

    /// Drain the dirty set and return modified chunk coordinates.
    ///
    /// The internal set is cleared; call once per frame before the GPU upload.
    pub fn take_dirty(&mut self) -> Vec<(i32, i32, i32)> {
        self.dirty_chunks.drain().collect()
    }

    /// Drain the removed set and return removed chunk coordinates.
    pub fn take_removed(&mut self) -> Vec<(i32, i32, i32)> {
        self.removed_chunks.drain().collect()
    }

    /// Iterate over all live chunk coordinates.
    pub fn live_chunks(&self) -> impl Iterator<Item = (i32, i32, i32)> + '_ {
        self.live_chunks.iter().copied()
    }

    /// Returns `true` if any chunks need GPU sync this frame.
    #[inline]
    pub fn has_work(&self) -> bool {
        !self.dirty_chunks.is_empty() || !self.removed_chunks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::hash_dag::LEVEL_SIZES;

    #[test]
    fn mark_and_drain() {
        let mut cm = ChunkManager::new();
        cm.mark_dirty((0, 0, 0));
        cm.mark_dirty((1, 0, 0));
        assert_eq!(cm.live_count(), 2);

        let dirty = cm.take_dirty();
        assert_eq!(dirty.len(), 2);
        // After take, dirty is empty but live is still there.
        assert!(cm.take_dirty().is_empty());
        assert_eq!(cm.live_count(), 2);
    }

    #[test]
    fn remove_chunk() {
        let mut cm = ChunkManager::new();
        cm.mark_dirty((0, 0, 0));
        cm.mark_removed((0, 0, 0));
        assert_eq!(cm.live_count(), 0);
        let removed = cm.take_removed();
        assert_eq!(removed.len(), 1);
    }

    #[test]
    fn aabb_contains() {
        let cs = LEVEL_SIZES[4] as i32;
        let aabb = ChunkAABB::from_chunk_coord(0, 0, 0, cs);
        assert!(aabb.contains(0, 0, 0));
        assert!(aabb.contains(cs - 1, cs - 1, cs - 1));
        assert!(!aabb.contains(cs, 0, 0));
        assert!(!aabb.contains(-1, 0, 0));
    }
}
