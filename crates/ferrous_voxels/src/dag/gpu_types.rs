//! GPU-side type definitions for the HashDAG storage buffers.
//!
//! These types are the **exact byte layout** of the SSBO entries read by the
//! WGSL shaders.  They must be `#[repr(C)]`, implement `bytemuck::Pod` /
//! `Zeroable`, and their field order and padding must match the WGSL struct
//! definitions.
//!
//! ## Buffer layout
//!
//! ```text
//! @group(0) @binding(0)  var<storage, read>  dag_nodes  : array<GpuDagNode>;
//! @group(0) @binding(1)  var<storage, read>  roots      : array<GpuChunkRoot>;
//! @group(0) @binding(2)  var<storage, read>  occupancy  : array<u32>;
//! ```
//!
//! See `assets/shaders/voxels/` for the matching WGSL declarations (added in
//! Phase 3).

use bytemuck::{Pod, Zeroable};

// ── Node ──────────────────────────────────────────────────────────────────────

/// GPU representation of one `DAGNode`.
///
/// Matches the WGSL struct:
/// ```wgsl
/// struct GpuDagNode {
///     children       : array<u32, 8>,
///     occupancy_mask : u32,   // only low 8 bits used
///     emissive_mask  : u32,   // only low 8 bits used
/// }
/// ```
/// Total: 10 × u32 = 40 bytes per node.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct GpuDagNode {
    /// Child node indices (pool-relative).  `u32::MAX` means empty.
    pub children: [u32; 8],
    /// Bit-set of occupied octants (low 8 bits).
    pub occupancy_mask: u32,
    /// Bit-set of emissive octants (low 8 bits).
    pub emissive_mask: u32,
}

impl GpuDagNode {
    /// The "empty / null" sentinel: all children absent, all masks zero.
    pub const EMPTY: Self = Self {
        children: [u32::MAX; 8],
        occupancy_mask: 0,
        emissive_mask: 0,
    };
}

// ── Chunk root table ─────────────────────────────────────────────────────────

/// One entry in the flat root-chunk lookup table on the GPU.
///
/// The shader resolves a world-space position to a chunk, binary-searches
/// (or hash-probes) this table for a matching entry, and uses `root_idx` as
/// the starting node in `dag_nodes`.
///
/// Matches the WGSL struct:
/// ```wgsl
/// struct GpuChunkRoot {
///     cx       : i32,
///     cy       : i32,
///     cz       : i32,
///     root_idx : u32,   // index into dag_nodes[level-4 pool], u32::MAX = absent
/// }
/// ```
/// Total: 4 × 4 = 16 bytes per entry.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct GpuChunkRoot {
    /// Chunk X coordinate (level-4 grid).
    pub cx: i32,
    /// Chunk Y coordinate.
    pub cy: i32,
    /// Chunk Z coordinate.
    pub cz: i32,
    /// Index of the level-4 root node in the `dag_nodes` pool for this level,
    /// or `u32::MAX` if the chunk is empty.
    pub root_idx: u32,
}

// ── Voxel material / colour ───────────────────────────────────────────────────

/// Compact GPU voxel payload that travels alongside the node tree.
///
/// The leaf node (`level == 0`) stores `voxel_packed` in `children[0]`.
/// The shader unpacks it using the same bit layout as `Voxel::pack()`:
///
/// | Bits  | Field       |
/// |-------|-------------|
/// | 7:0   | material_id |
/// | 15:8  | emissive    |
/// | 23:16 | damage      |
/// | 31:24 | flags       |
pub type GpuPackedVoxel = u32;

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    #[test]
    fn gpu_dag_node_size() {
        // 8 children + 2 masks = 10 u32s = 40 bytes.
        assert_eq!(mem::size_of::<GpuDagNode>(), 40);
    }

    #[test]
    fn gpu_chunk_root_size() {
        // 3 i32 + 1 u32 = 16 bytes.
        assert_eq!(mem::size_of::<GpuChunkRoot>(), 16);
    }

    #[test]
    fn gpu_dag_node_empty_sentinel() {
        let e = GpuDagNode::EMPTY;
        assert_eq!(e.occupancy_mask, 0);
        assert!(e.children.iter().all(|&c| c == u32::MAX));
    }
}
