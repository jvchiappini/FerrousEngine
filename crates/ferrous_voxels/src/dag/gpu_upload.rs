//! CPU → GPU synchronisation of the HashDAG.
//!
//! `DagGpuSync` is a helper (not a `RenderPass` itself) that reads the dirty
//! node sets from a [`HashDAG`], converts the dirty `DAGNode`s into
//! `GpuDagNode` bytes, and writes them into the staging buffers owned by
//! [`PersistentBuffers`].
//!
//! The actual GPU copy (`staging → node_buf`) is recorded by
//! [`VoxelGpuUploadPass`](crate::passes::gpu_upload_pass::VoxelGpuUploadPass)
//! into the frame's `CommandEncoder`.
//!
//! # Node address scheme
//!
//! All levels share a single `node_buf` SSBO.  Each level is allocated a
//! contiguous slice:
//!
//! ```text
//! node_buf layout:
//! [ level-0 nodes (0 .. base[1]) | level-1 nodes (base[1] .. base[2]) | … ]
//! ```
//!
//! `LevelOffsets::base[L]` is the starting index in the flat array for level
//! `L`.  The shader receives these offsets via a small uniform buffer
//! (added in Phase 3).
//!
//! For Phase 2 we simply concatenate all levels and track the per-level bases
//! so Phase 3 can add the uniform in one place.

use bytemuck::cast_slice;

use crate::buffers::PersistentBuffers;
use crate::dag::{
    gpu_types::{GpuChunkRoot, GpuDagNode},
    hash_dag::{HashDAG, DAG_LEVELS},
    node::DAGNode,
};

// ── Level base offsets ────────────────────────────────────────────────────────

/// Starting index in the flat `node_buf` for each hierarchy level.
///
/// Recomputed each time a level overflows its previous slice.  For Phase 2
/// the layout is fixed to the capacities of `PersistentBuffers`; a dynamic
/// allocator is added in Phase 3 when pool sizes are tracked separately.
#[derive(Clone, Copy, Debug, Default)]
pub struct LevelOffsets {
    /// `base[L]` = first `GpuDagNode` index in `node_buf` for level `L`.
    pub base: [u32; DAG_LEVELS],
    /// Total nodes across all levels (= end of the last level's slice).
    pub total_nodes: u32,
}

impl LevelOffsets {
    /// Compute offsets from the actual per-level node counts.
    pub fn compute(counts: &[u32; DAG_LEVELS]) -> Self {
        let mut base = [0u32; DAG_LEVELS];
        let mut cursor = 0u32;
        for (level, &count) in counts.iter().enumerate() {
            base[level] = cursor;
            cursor += count;
        }
        Self {
            base,
            total_nodes: cursor,
        }
    }
}

// ── DagGpuSync ────────────────────────────────────────────────────────────────

/// Pending GPU write produced by [`DagGpuSync::prepare`].
///
/// Contains all the bytes that should be written to the staging buffers before
/// the `CommandEncoder` records the `copy_buffer_to_buffer` commands.
pub struct DagUploadBatch {
    /// Flat byte slice for the node staging buffer.
    ///
    /// Written to `staging_node` at byte offset
    /// `offsets.base[level] * size_of::<GpuDagNode>()`.
    ///
    /// For simplicity Phase 2 uploads the **entire** flat array every dirty
    /// frame.  Phase 3 will switch to per-node sparse updates.
    pub node_bytes: Vec<u8>,
    /// Flat byte slice for the root staging buffer.
    pub root_bytes: Vec<u8>,
    /// Level offsets valid for this frame (needed by the shader uniform).
    pub offsets: LevelOffsets,
    /// Whether the node buffer needs to grow (`PersistentBuffers::ensure_node_capacity`).
    pub needs_node_realloc: bool,
    /// Whether the root buffer needs to grow.
    pub needs_root_realloc: bool,
}

/// Converts CPU `HashDAG` state into GPU-ready bytes.
///
/// Call [`DagGpuSync::prepare`] in `VoxelGpuUploadPass::prepare` and store the
/// resulting [`DagUploadBatch`] until `execute` records the copy commands.
pub struct DagGpuSync;

impl DagGpuSync {
    /// Snapshot the current `HashDAG` into upload-ready byte slices.
    ///
    /// This is a **full upload** for Phase 2: all nodes at all levels are
    /// serialised every time anything is dirty.  Phase 3 will refine this to
    /// only re-upload changed nodes.
    pub fn prepare(dag: &HashDAG, buffers: &PersistentBuffers) -> Option<DagUploadBatch> {
        // Nothing to upload if the DAG is clean.
        if !dag.has_dirty_nodes() && dag.chunk_count() == 0 {
            return None;
        }

        // ── 1. Build per-level GpuDagNode vectors ─────────────────────────────
        let mut per_level: [Vec<GpuDagNode>; DAG_LEVELS] = Default::default();

        for level in 0..DAG_LEVELS {
            per_level[level] = dag
                .level_nodes(level)
                .iter()
                .map(|cpu_node| Self::convert_node(cpu_node))
                .collect();
        }

        // ── 2. Compute level offsets ──────────────────────────────────────────
        let counts: [u32; DAG_LEVELS] = std::array::from_fn(|l| per_level[l].len() as u32);
        let offsets = LevelOffsets::compute(&counts);
        let total_nodes = offsets.total_nodes as u64;

        // ── 3. Concatenate all levels into a single byte slice ─────────────────
        let mut node_bytes: Vec<u8> =
            Vec::with_capacity(total_nodes as usize * std::mem::size_of::<GpuDagNode>());
        for level_nodes in &per_level {
            node_bytes.extend_from_slice(cast_slice(level_nodes.as_slice()));
        }

        // ── 4. Build the root table ───────────────────────────────────────────
        let mut roots: Vec<GpuChunkRoot> = dag
            .roots()
            .iter()
            .map(|(&(cx, cy, cz), &root_idx)| {
                /*
                log::debug!(
                    "Root at {}, {}, {} is idx {}, base[12] is {}",
                    cx,
                    cy,
                    cz,
                    root_idx,
                    offsets.base[12]
                );
                */
                GpuChunkRoot {
                    cx,
                    cy,
                    cz,
                    root_idx,
                }
            })
            .collect();
        // Sort for O(log n) binary search in the shader.
        roots.sort_by_key(|r| (r.cx, r.cy, r.cz));

        let root_bytes: Vec<u8> = cast_slice(roots.as_slice()).to_vec();

        let needs_node_realloc = total_nodes > buffers.node_capacity;
        let needs_root_realloc = roots.len() as u64 > buffers.root_capacity;

        Some(DagUploadBatch {
            node_bytes,
            root_bytes,
            offsets,
            needs_node_realloc,
            needs_root_realloc,
        })
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Convert one CPU `DAGNode` → `GpuDagNode`.
    #[inline]
    fn convert_node(cpu: &DAGNode) -> GpuDagNode {
        GpuDagNode {
            children: cpu.children,
            occupancy_mask: cpu.occupancy_mask as u32,
            emissive_mask: cpu.emissive_mask as u32,
        }
    }
}
