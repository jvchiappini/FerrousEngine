//! High-level voxel edit API.
//!
//! `VoxelWorld` is the central entry point for gameplay code.  It wraps the
//! `HashDAG` and `ChunkManager` and exposes a simple interface for:
//!
//! - Placing voxels (`set_voxel`)
//! - Destroying voxels (`destroy_voxel`)
//! - Querying (`get_voxel`, `is_solid`)
//! - Batch operations (`fill_box`, `destroy_sphere`)
//!
//! All edits automatically:
//! 1. Update the `HashDAG` (CPU side).
//! 2. Mark the containing chunk dirty for GPU sync.
//! 3. Update the level-4 occupancy/emissivity bit grids.

use crate::dag::{HashDAG, Voxel, MaterialId};
use super::chunk_manager::ChunkManager;

/// The complete CPU-side voxel world.
///
/// Pass a mutable reference to the per-frame `update` to apply gameplay edits,
/// then call `take_dirty_*` to feed the GPU upload pass.
pub struct VoxelWorld {
    /// The hash-deduplicated voxel representation.
    pub dag: HashDAG,
    /// Chunk lifecycle and dirty tracking.
    pub chunks: ChunkManager,
    /// Running count of solid voxels (updated on every edit).
    voxel_count: u64,
}

impl VoxelWorld {
    /// Create an empty world.
    pub fn new() -> Self {
        Self {
            dag: HashDAG::new(),
            chunks: ChunkManager::new(),
            voxel_count: 0,
        }
    }

    // ── Single voxel edits ────────────────────────────────────────────────────

    /// Place a solid voxel at `(wx, wy, wz)` with the given `material_id`.
    ///
    /// Overwrites any existing voxel at that position.
    pub fn set_voxel(&mut self, wx: i32, wy: i32, wz: i32, material_id: MaterialId) {
        let was_solid = self.dag.get_voxel(wx, wy, wz).is_solid();
        let v = Voxel::solid(material_id);
        self.dag.set_voxel(wx, wy, wz, v);
        let chunk = HashDAG::world_to_chunk(wx, wy, wz);
        self.chunks.mark_dirty(chunk);
        if !was_solid {
            self.voxel_count += 1;
        }
    }

    /// Place an emissive voxel (e.g. lava, screens, fire) at `(wx, wy, wz)`.
    pub fn set_emissive_voxel(
        &mut self,
        wx: i32, wy: i32, wz: i32,
        material_id: MaterialId,
        intensity: u8,
    ) {
        let was_solid = self.dag.get_voxel(wx, wy, wz).is_solid();
        let v = Voxel::emissive(material_id, intensity);
        self.dag.set_voxel(wx, wy, wz, v);
        let chunk = HashDAG::world_to_chunk(wx, wy, wz);
        self.chunks.mark_dirty(chunk);
        if !was_solid {
            self.voxel_count += 1;
        }
    }

    /// Remove (destroy) the voxel at `(wx, wy, wz)`.
    ///
    /// If the position is already empty this is a no-op.
    pub fn destroy_voxel(&mut self, wx: i32, wy: i32, wz: i32) {
        let existing = self.dag.get_voxel(wx, wy, wz);
        if !existing.is_solid() {
            return; // already empty
        }
        self.dag.set_voxel(wx, wy, wz, Voxel::AIR);
        let chunk = HashDAG::world_to_chunk(wx, wy, wz);
        self.chunks.mark_dirty(chunk);
        self.voxel_count = self.voxel_count.saturating_sub(1);
    }

    /// Query the voxel at `(wx, wy, wz)`.
    ///
    /// Returns `Voxel::AIR` for empty or out-of-bounds coordinates.
    #[inline]
    pub fn get_voxel(&self, wx: i32, wy: i32, wz: i32) -> Voxel {
        self.dag.get_voxel(wx, wy, wz)
    }

    /// Returns `true` if the position contains a solid voxel.
    #[inline]
    pub fn is_solid(&self, wx: i32, wy: i32, wz: i32) -> bool {
        self.dag.get_voxel(wx, wy, wz).is_solid()
    }

    // ── Batch edits ───────────────────────────────────────────────────────────

    /// Fill an axis-aligned box with the given material.
    ///
    /// `min` and `max` are inclusive bounds in level-0 world coordinates.
    ///
    /// Large fills may generate many dirty chunks; the GPU upload pass handles
    /// them in bulk.
    pub fn fill_box(
        &mut self,
        min: (i32, i32, i32),
        max: (i32, i32, i32),
        material_id: MaterialId,
    ) {
        for z in min.2..=max.2 {
            for y in min.1..=max.1 {
                for x in min.0..=max.0 {
                    self.set_voxel(x, y, z, material_id);
                }
            }
        }
    }

    /// Destroy all voxels within an axis-aligned box.
    pub fn destroy_box(&mut self, min: (i32, i32, i32), max: (i32, i32, i32)) {
        for z in min.2..=max.2 {
            for y in min.1..=max.1 {
                for x in min.0..=max.0 {
                    self.destroy_voxel(x, y, z);
                }
            }
        }
    }

    /// Destroy all voxels within a sphere of `radius` centred at `(cx, cy, cz)`.
    ///
    /// All coordinates in level-0 world units.
    pub fn destroy_sphere(&mut self, cx: i32, cy: i32, cz: i32, radius: i32) {
        let r2 = (radius * radius) as i64;
        let min = (cx - radius, cy - radius, cz - radius);
        let max = (cx + radius, cy + radius, cz + radius);
        for z in min.2..=max.2 {
            for y in min.1..=max.1 {
                for x in min.0..=max.0 {
                    let dx = (x - cx) as i64;
                    let dy = (y - cy) as i64;
                    let dz = (z - cz) as i64;
                    if dx * dx + dy * dy + dz * dz <= r2 {
                        self.destroy_voxel(x, y, z);
                    }
                }
            }
        }
    }

    // ── Statistics ────────────────────────────────────────────────────────────

    /// Total number of solid voxels currently in the world.
    #[inline]
    pub fn voxel_count(&self) -> u64 {
        self.voxel_count
    }

    /// Number of live (non-empty) level-4 chunks.
    #[inline]
    pub fn chunk_count(&self) -> usize {
        self.chunks.live_count()
    }
}

impl Default for VoxelWorld {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_destroy() {
        let mut world = VoxelWorld::new();
        world.set_voxel(0, 0, 0, 1);
        assert!(world.is_solid(0, 0, 0));
        assert_eq!(world.voxel_count(), 1);

        world.destroy_voxel(0, 0, 0);
        assert!(!world.is_solid(0, 0, 0));
        assert_eq!(world.voxel_count(), 0);
    }

    #[test]
    fn destroy_nonexistent_is_noop() {
        let mut world = VoxelWorld::new();
        world.destroy_voxel(99, 99, 99); // must not panic
        assert_eq!(world.voxel_count(), 0);
    }

    #[test]
    fn fill_box_count() {
        let mut world = VoxelWorld::new();
        world.fill_box((0, 0, 0), (3, 3, 3), 2); // 4×4×4 = 64 voxels

        // Verify all 64 positions are solid.
        let mut solid_count = 0u64;
        for z in 0..=3i32 {
            for y in 0..=3i32 {
                for x in 0..=3i32 {
                    if world.is_solid(x, y, z) {
                        solid_count += 1;
                    }
                }
            }
        }
        assert_eq!(solid_count, 64, "solid_count mismatch");
        assert_eq!(world.voxel_count(), 64);
    }

    #[test]
    fn destroy_sphere() {
        let mut world = VoxelWorld::new();
        world.fill_box((-10, -10, -10), (10, 10, 10), 1);
        let before = world.voxel_count();
        world.destroy_sphere(0, 0, 0, 3);
        let after = world.voxel_count();
        assert!(after < before);
    }

    #[test]
    fn dirty_chunks_after_edit() {
        let mut world = VoxelWorld::new();
        world.set_voxel(0, 0, 0, 1);
        assert!(world.chunks.has_work());
    }
}
