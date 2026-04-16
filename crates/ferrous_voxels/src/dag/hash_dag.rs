//! HashDAG — Hierarchical, hash-deduplicated Directed Acyclic Graph for voxel worlds.
//!
//! # Hierarchy
//!
//! The world is subdivided into 13 resolution levels using a standard binary
//! octree (2×2×2 split per axis, 8 children per node):
//!
//! | Level | Cell size | Description                     |
//! |-------|-----------|---------------------------------|
//! | 0     | 1 cm      | Leaf voxels (finest detail)     |
//! | 1     | 2 cm      | 2 cm group                      |
//! | …     | …         | …                               |
//! | 12    | 4096 cm   | ~40 m streaming chunk (root)    |
//!
//! `2^12 = 4096` — 13 levels span 1 cm leaf → ~40 m streaming chunk,
//! matching the roadmap target for GPU HDDA traversal.
//!
//! # Deduplication
//!
//! Identical subtrees share the same `DAGNode` instance via a content-addressed
//! `HashMap<u64, Vec<u32>>` (hash → node pool indices).  A typical voxel world
//! achieves 10–20× memory reduction compared to a naïve SVO.
//!
//! # Dirty tracking
//!
//! Every mutation (insert / remove) marks the modified path in the `dirty_nodes`
//! `HashSet`.  The GPU upload pass reads this set each frame to transfer only
//! changed data.
//!
//! # Coordinate system
//!
//! All coordinates are in integer grid units at the given level.  Level-0 world
//! coordinates correspond to 1 cm voxels starting at (0, 0, 0).

use std::collections::{HashMap, HashSet};

use super::{
    bit_grid::LevelGrids,
    node::{DAGNode, Voxel},
};

/// Number of hierarchy levels in the HashDAG (0 = finest, 12 = coarsest).
pub const DAG_LEVELS: usize = 13;

/// The HashDAG uses a standard binary octree where each node divides its
/// cell into 8 equal octants (2×2×2 split per axis).
///
/// `LEVEL_SIZES[L]` is the cell size in voxels at level L.  Each level's
/// cell is exactly twice the size of the level below it:
///
/// | Level | Cell size (voxels) | Approx size @ 1 cm/voxel  |
/// |-------|--------------------|----------------------------|
/// | 0     | 1                  | 1 cm  (leaf voxel)         |
/// | 1     | 2                  | 2 cm                       |
/// | 2     | 4                  | 4 cm                       |
/// | 3     | 8                  | 8 cm                       |
/// | 4     | 16                 | 16 cm                      |
/// | 5     | 32                 | 32 cm                      |
/// | 6     | 64                 | 64 cm (~room scale)        |
/// | 7     | 128                | 1.3 m                      |
/// | 8     | 256                | 2.6 m                      |
/// | 9     | 512                | 5.1 m                      |
/// | 10    | 1024               | 10 m                       |
/// | 11    | 2048               | 20 m                       |
/// | 12    | 4096               | 41 m  (streaming chunk)    |
///
/// 13 levels gives `2^12 = 4096 cm ≈ 40 m` per streaming chunk.
pub const LEVEL_SIZES: [u32; DAG_LEVELS] =
    [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096];

/// Compute an inexpensive hash for a `DAGNode` used as its deduplication key.
///
/// We use a simple XOR-fold of children + masks; collisions are benign (they
/// just prevent deduplication for those two nodes).
fn hash_node(node: &DAGNode) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325; // FNV offset basis
    for &child in &node.children {
        h ^= child as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3); // FNV prime
    }
    h ^= node.occupancy_mask as u64;
    h ^= (node.emissive_mask as u64) << 8;
    h
}

// ── Node pool ─────────────────────────────────────────────────────────────────

/// A level-specific pool of `DAGNode`s with content-addressed deduplication.
#[derive(Debug, Default)]
struct NodePool {
    /// All unique nodes allocated at this level.
    nodes: Vec<DAGNode>,
    /// Maps `hash_node(node)` → list of indices into `nodes` that share this hash.
    /// Using a Vec per bucket handles collisions correctly.
    hash_to_indices: HashMap<u64, Vec<u32>>,
}

impl NodePool {
    /// Insert `node` into the pool and return its stable index.
    ///
    /// If an identical node already exists its index is returned without
    /// allocating a new entry (DAG deduplication).
    /// Hash collisions are handled by linear scan of the bucket.
    fn intern(&mut self, node: DAGNode) -> u32 {
        let h = hash_node(&node);
        // Check existing bucket for an exact match.
        if let Some(bucket) = self.hash_to_indices.get(&h) {
            for &idx in bucket {
                if self.nodes[idx as usize] == node {
                    return idx; // exact match — deduplicate
                }
            }
        }
        // New node.
        let idx = self.nodes.len() as u32;
        self.nodes.push(node);
        self.hash_to_indices.entry(h).or_default().push(idx);
        idx
    }

    /// Retrieve a reference to a node by its pool index.
    #[inline]
    fn get(&self, idx: u32) -> Option<&DAGNode> {
        self.nodes.get(idx as usize)
    }
}

// ── HashDAG ───────────────────────────────────────────────────────────────────

/// The complete HashDAG world representation.
///
/// Create one `HashDAG` per level of detail you want to support.  For a full
/// game world create a single `HashDAG` that spans the playable area.
pub struct HashDAG {
    /// Per-level node pools (index 0 = finest / leaf level).
    pools: [NodePool; DAG_LEVELS],

    /// Level-12 root nodes keyed by their chunk coordinate `(cx, cy, cz)`.
    /// A chunk covers `LEVEL_SIZES[12]` = 4096 level-0 cells ≈ 40 m on a side.
    roots: HashMap<(i32, i32, i32), u32>,

    /// Bit-grids for occupancy and emissivity at each level.
    ///
    /// Indexed as `grids[level]`.  The coarsest grid (level 12) spans a
    /// fixed 256³ cells (= 256 × 40 m ≈ 10 km³ playable volume).
    pub grids: [LevelGrids; DAG_LEVELS],

    /// Pool indices of nodes modified since the last `take_dirty` call.
    /// Tuple: `(level, node_index)`.
    dirty_nodes: HashSet<(usize, u32)>,

    /// Chunk coordinates that were modified this frame.
    dirty_chunks: HashSet<(i32, i32, i32)>,
}

impl HashDAG {
    /// Grid dimensions (in cells) for each level's `LevelGrids`.
    ///
    /// Coarse levels (10-12) use larger grids for world-scale queries.
    /// Fine levels (0-4) use small grids — the HashDAG pools handle fine detail.
    ///
    /// | Level | Grid dim | Coverage @ 1 cm/voxel   |
    /// |-------|----------|-------------------------|
    /// | 12    | 256³     | 256 chunks × 40 m       |
    /// | 11    | 256³     | 256 × 20 m cells        |
    /// | 10    | 256³     | 256 × 10 m cells        |
    /// | 9     | 128³     | 128 × 5 m cells         |
    /// | 8     | 64³      | 64 × 2.6 m cells        |
    /// | 7     | 32³      | 32 × 1.3 m cells        |
    /// | 6     | 32³      | 32 × 64 cm cells        |
    /// | 0-5   | 16³      | fine-detail (DAG-only)  |
    const GRID_DIM: [u32; DAG_LEVELS] = [16, 16, 16, 16, 16, 16, 32, 32, 64, 128, 256, 256, 256];

    /// Create a new, empty `HashDAG`.
    pub fn new() -> Self {
        Self {
            pools: Default::default(),
            roots: HashMap::new(),
            grids: std::array::from_fn(|l| {
                LevelGrids::new(Self::GRID_DIM[l], Self::GRID_DIM[l], Self::GRID_DIM[l])
            }),
            dirty_nodes: HashSet::new(),
            dirty_chunks: HashSet::new(),
        }
    }

    // ── Coordinate helpers ────────────────────────────────────────────────────

    /// Convert a level-0 world coordinate to the containing chunk coordinate.
    #[inline]
    pub fn world_to_chunk(x: i32, y: i32, z: i32) -> (i32, i32, i32) {
        let s = LEVEL_SIZES[DAG_LEVELS - 1] as i32;
        (x.div_euclid(s), y.div_euclid(s), z.div_euclid(s))
    }

    /// Compute the child index (0-7) in an octree node for coordinates `(lx, ly, lz)`
    /// inside a cell of size `cell_size`.
    ///
    /// The cell is divided into 8 equal octants by splitting each axis at
    /// `half = cell_size / 2`.  The returned index encodes which octant:
    ///   bit 0 = lx >= half, bit 1 = ly >= half, bit 2 = lz >= half.
    #[inline]
    fn octant(lx: i32, ly: i32, lz: i32, half: i32) -> usize {
        debug_assert!(
            half > 0,
            "octant: half must be > 0 (cell_size must be >= 2)"
        );
        ((if lx >= half { 1 } else { 0 })
            | (if ly >= half { 2 } else { 0 })
            | (if lz >= half { 4 } else { 0 })) as usize
    }

    /// Clamp child-local coordinates into [0, half) for the next level.
    #[inline]
    fn child_local(lx: i32, ly: i32, lz: i32, half: i32) -> (i32, i32, i32) {
        (
            if lx >= half { lx - half } else { lx },
            if ly >= half { ly - half } else { ly },
            if lz >= half { lz - half } else { lz },
        )
    }

    // ── Voxel insertion (recursive) ───────────────────────────────────────────

    /// Descend the tree read-only from `node_idx` at `current_level` down to
    /// `target_level`, following the path for the coordinate `(lx, ly, lz)`
    /// inside a cell of size `cell_size`.
    ///
    /// Returns the node index at `target_level` if it exists, or `None`.
    ///
    /// Currently unused — reserved for Phase 3 (HDDA shader node streaming).
    #[allow(dead_code)]
    fn find_node_at_level(
        &self,
        node_idx: u32,
        lx: i32,
        ly: i32,
        lz: i32,
        cell_size: i32,
        current_level: usize,
        target_level: usize,
    ) -> Option<u32> {
        if current_level == target_level {
            return Some(node_idx);
        }
        let node = self.pools[current_level].get(node_idx)?;
        let half = cell_size / 2;
        let octant = Self::octant(lx, ly, lz, half);
        if node.occupancy_mask & (1 << octant) == 0 {
            return None;
        }
        let child_idx = node.children[octant];
        if child_idx == u32::MAX {
            return None;
        }
        let clx = if lx >= half { lx - half } else { lx };
        let cly = if ly >= half { ly - half } else { ly };
        let clz = if lz >= half { lz - half } else { lz };
        self.find_node_at_level(
            child_idx,
            clx,
            cly,
            clz,
            half,
            current_level - 1,
            target_level,
        )
    }

    /// Insert `voxel` at level-0 world coordinates `(wx, wy, wz)`.
    ///
    /// If `voxel` is `Voxel::AIR` the call is equivalent to `remove_voxel`.
    pub fn set_voxel(&mut self, wx: i32, wy: i32, wz: i32, voxel: Voxel) {
        let chunk = Self::world_to_chunk(wx, wy, wz);
        let root_level = DAG_LEVELS - 1;
        let cs = LEVEL_SIZES[root_level] as i32;

        // Local coords within the chunk (level-root cell).
        let lx = wx.rem_euclid(cs);
        let ly = wy.rem_euclid(cs);
        let lz = wz.rem_euclid(cs);

        let current_root = self.roots.get(&chunk).copied();
        let new_root = self.set_recursive(lx, ly, lz, cs, root_level, current_root, voxel);

        match new_root {
            Some(idx) => {
                self.roots.insert(chunk, idx);
            }
            None => {
                self.roots.remove(&chunk);
            }
        }

        self.dirty_chunks.insert(chunk);

        // Update coarsest grid occupancy.
        let (cx, cy, cz) = chunk;
        let is_solid = voxel.is_solid();
        let is_emissive = voxel.is_emissive();
        self.grids[root_level].occupancy.write(cx, cy, cz, is_solid);
        if is_emissive {
            self.grids[root_level].emissivity.set(cx, cy, cz);
        }
    }

    /// Recursive helper — descends the DAG from `level` down to 0, rebuilding
    /// the path from root to the modified leaf.
    ///
    /// `current_idx` is the pool index of the existing node at `level` for this
    /// coordinate path (if any).  Passing it avoids a second tree traversal to
    /// find sibling children that must be preserved.
    ///
    /// Returns `Some(new_index)` if the subtree is non-empty after the edit,
    /// `None` if the entire subtree became empty (all air).
    fn set_recursive(
        &mut self,
        lx: i32,
        ly: i32,
        lz: i32,
        cell_size: i32,
        level: usize,
        current_idx: Option<u32>,
        voxel: Voxel,
    ) -> Option<u32> {
        // Level 0 is the leaf: this node IS a single voxel.
        // No subdivision — just store the packed voxel.
        if level == 0 {
            if !voxel.is_solid() {
                return None; // air → prune
            }
            let mut node = DAGNode::empty();
            node.children[0] = voxel.pack();
            node.occupancy_mask = 1;
            if voxel.is_emissive() {
                node.emissive_mask = 1;
            }
            let idx = self.pools[0].intern(node);
            self.dirty_nodes.insert((0, idx));
            return Some(idx);
        }

        let half = cell_size / 2;
        let octant = Self::octant(lx, ly, lz, half);

        let (clx, cly, clz) = Self::child_local(lx, ly, lz, half);

        // Look up the child at `octant` in the current node (if it exists),
        // so we can pass it to the recursive call as context.
        let child_current_idx: Option<u32> = current_idx
            .and_then(|idx| self.pools[level].get(idx))
            .and_then(|node| {
                if node.occupancy_mask & (1 << octant) != 0 {
                    let c = node.children[octant];
                    if c != u32::MAX {
                        Some(c)
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

        // Recurse into the child.
        let child_result =
            self.set_recursive(clx, cly, clz, half, level - 1, child_current_idx, voxel);

        // Start from the existing node at this level (clone to preserve siblings),
        // then update only the affected octant.
        let mut node: DAGNode = current_idx
            .and_then(|idx| self.pools[level].get(idx).cloned())
            .unwrap_or_else(DAGNode::empty);

        match child_result {
            Some(child_idx) => {
                node.children[octant] = child_idx;
                node.occupancy_mask |= 1 << octant;
                // Propagate emissive flag from child.
                let emissive = self.pools[level - 1]
                    .get(child_idx)
                    .is_some_and(|n| n.emissive_mask != 0);
                if emissive {
                    node.emissive_mask |= 1 << octant;
                } else {
                    node.emissive_mask &= !(1u8 << octant);
                }
            }
            None => {
                node.children[octant] = u32::MAX;
                node.occupancy_mask &= !(1u8 << octant);
                node.emissive_mask &= !(1u8 << octant);
            }
        }

        if node.occupancy_mask == 0 {
            return None; // empty subtree, prune
        }

        let idx = self.pools[level].intern(node);
        self.dirty_nodes.insert((level, idx));
        Some(idx)
    }

    // ── Voxel query ───────────────────────────────────────────────────────────

    /// Retrieve the voxel at level-0 world coordinates.
    ///
    /// Returns `Voxel::AIR` if the coordinate is empty or out of bounds.
    pub fn get_voxel(&self, wx: i32, wy: i32, wz: i32) -> Voxel {
        let chunk = Self::world_to_chunk(wx, wy, wz);
        let root_idx = match self.roots.get(&chunk) {
            Some(&idx) => idx,
            None => return Voxel::AIR,
        };

        let root_level = DAG_LEVELS - 1;
        let cs = LEVEL_SIZES[root_level] as i32;
        let lx = wx.rem_euclid(cs);
        let ly = wy.rem_euclid(cs);
        let lz = wz.rem_euclid(cs);

        self.get_recursive(root_idx, lx, ly, lz, cs, root_level)
    }

    fn get_recursive(
        &self,
        node_idx: u32,
        lx: i32,
        ly: i32,
        lz: i32,
        cell_size: i32,
        level: usize,
    ) -> Voxel {
        let node = match self.pools[level].get(node_idx) {
            Some(n) => n,
            None => return Voxel::AIR,
        };

        // Level 0 is the leaf node — it stores exactly one voxel in children[0].
        if level == 0 {
            let packed = node.children[0];
            return if packed == u32::MAX || node.occupancy_mask == 0 {
                Voxel::AIR
            } else {
                Voxel::unpack(packed)
            };
        }

        let half = cell_size / 2;
        let octant = Self::octant(lx, ly, lz, half);

        if node.occupancy_mask & (1 << octant) == 0 {
            return Voxel::AIR; // octant is empty
        }

        let child_idx = node.children[octant];
        if child_idx == u32::MAX {
            return Voxel::AIR;
        }

        let (clx, cly, clz) = Self::child_local(lx, ly, lz, half);

        self.get_recursive(child_idx, clx, cly, clz, half, level - 1)
    }

    // ── Occupancy query ───────────────────────────────────────────────────────

    /// Returns `true` if any voxel exists within the level-12 chunk at `(cx, cy, cz)`.
    #[inline]
    pub fn chunk_has_voxels(&self, cx: i32, cy: i32, cz: i32) -> bool {
        self.grids[DAG_LEVELS - 1].occupancy.get(cx, cy, cz)
    }

    /// Returns `true` if any emissive voxel exists within the level-12 chunk.
    #[inline]
    pub fn chunk_has_emissive(&self, cx: i32, cy: i32, cz: i32) -> bool {
        self.grids[DAG_LEVELS - 1].emissivity.get(cx, cy, cz)
    }

    // ── Dirty tracking ────────────────────────────────────────────────────────

    /// Drain and return all dirty `(level, node_index)` pairs since the last call.
    ///
    /// The internal dirty set is cleared after this call.
    pub fn take_dirty_nodes(&mut self) -> Vec<(usize, u32)> {
        self.dirty_nodes.drain().collect()
    }

    /// Drain and return all dirty chunk coordinates since the last call.
    pub fn take_dirty_chunks(&mut self) -> Vec<(i32, i32, i32)> {
        self.dirty_chunks.drain().collect()
    }

    /// Returns `true` if any nodes have been modified since the last `take_dirty_nodes`.
    #[inline]
    pub fn has_dirty_nodes(&self) -> bool {
        !self.dirty_nodes.is_empty()
    }

    // ── Statistics ────────────────────────────────────────────────────────────

    /// Total number of unique nodes across all levels.
    pub fn node_count(&self) -> usize {
        self.pools.iter().map(|p| p.nodes.len()).sum()
    }

    /// Number of live (non-empty) root chunks.
    pub fn chunk_count(&self) -> usize {
        self.roots.len()
    }

    /// Read-only access to the flat node slice at a given hierarchy level.
    ///
    /// Used by the GPU upload pass to serialise node data into staging buffers.
    /// Returns an empty slice for levels that have no nodes.
    #[inline]
    pub fn level_nodes(&self, level: usize) -> &[DAGNode] {
        self.pools
            .get(level)
            .map(|p| p.nodes.as_slice())
            .unwrap_or(&[])
    }

    /// Read-only access to the root chunk map.
    ///
    /// Returns a map from chunk coordinates `(cx, cy, cz)` to the level-4
    /// root node index in the level-4 pool.
    #[inline]
    pub fn roots(&self) -> &HashMap<(i32, i32, i32), u32> {
        &self.roots
    }
}

impl Default for HashDAG {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::node::Voxel;

    #[test]
    fn two_voxels_same_chunk() {
        let mut dag = HashDAG::new();
        dag.set_voxel(0, 0, 0, Voxel::solid(1));
        dag.set_voxel(1, 0, 0, Voxel::solid(1));
        assert!(dag.get_voxel(0, 0, 0).is_solid(), "(0,0,0) should be solid");
        assert!(dag.get_voxel(1, 0, 0).is_solid(), "(1,0,0) should be solid");
        assert!(!dag.get_voxel(2, 0, 0).is_solid(), "(2,0,0) should be air");
    }

    #[test]
    fn insert_and_query_single_voxel() {
        let mut dag = HashDAG::new();
        let v = Voxel::solid(7);
        dag.set_voxel(0, 0, 0, v);
        let got = dag.get_voxel(0, 0, 0);
        assert_eq!(got.material_id, 7);
        assert!(got.is_solid());
    }

    #[test]
    fn air_voxel_returns_default() {
        let dag = HashDAG::new();
        let v = dag.get_voxel(10, 20, 30);
        assert!(!v.is_solid());
        assert_eq!(v, Voxel::AIR);
    }

    #[test]
    fn dirty_chunks_populated_after_edit() {
        let mut dag = HashDAG::new();
        dag.set_voxel(5, 5, 5, Voxel::solid(1));
        assert!(!dag.dirty_chunks.is_empty());
        let chunks = dag.take_dirty_chunks();
        assert_eq!(chunks.len(), 1);
        assert!(dag.dirty_chunks.is_empty());
    }

    #[test]
    fn multiple_inserts_deduplicate_nodes() {
        let mut dag = HashDAG::new();
        // Insert the same voxel type at many positions — DAG should deduplicate leaf nodes.
        for i in 0..8 {
            dag.set_voxel(i, 0, 0, Voxel::solid(3));
        }
        // We can only assert that the total count is less than 8 unique inserts
        // would produce without deduplication; exact count depends on structure.
        let n = dag.node_count();
        assert!(n > 0);
        assert!(n < 8 * DAG_LEVELS); // sanity upper bound
    }

    #[test]
    fn chunk_occupancy_updated() {
        let mut dag = HashDAG::new();
        let (cx, cy, cz) = HashDAG::world_to_chunk(0, 0, 0);
        assert!(!dag.chunk_has_voxels(cx, cy, cz));
        dag.set_voxel(0, 0, 0, Voxel::solid(1));
        assert!(dag.chunk_has_voxels(cx, cy, cz));
    }
}
