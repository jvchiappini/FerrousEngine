//! Voxel and DAG node primitives.
//!
//! # Voxel memory layout (4 bytes)
//!
//! | Byte | Field       | Range  | Description                       |
//! |------|-------------|--------|-----------------------------------|
//! | 0    | material_id | 0-255  | Index into the material table     |
//! | 1    | emissive    | 0-255  | Emission intensity (HDR-scaled)   |
//! | 2    | damage      | 0-255  | Destruction state (0 = pristine)  |
//! | 3    | flags       | bits   | See `VoxelFlags`                  |
//!
//! A voxel with `flags & OCCUPIED == 0` is considered empty (air).

/// Index into the material table. 256 distinct materials.
pub type MaterialId = u8;

bitflags::bitflags! {
    /// Per-voxel status flags (stored in byte 3 of `Voxel`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct VoxelFlags: u8 {
        /// The voxel cell contains solid geometry.
        const OCCUPIED  = 0b0000_0001;
        /// The voxel was modified this frame (dirty bit for GPU sync).
        const MODIFIED  = 0b0000_0010;
        /// The voxel emits light (shortcut — avoids reading `emissive` every traversal).
        const EMISSIVE  = 0b0000_0100;
        /// Reserved for future use.
        const _RESERVED = 0b1111_1000;
    }
}

/// A single voxel cell at the finest resolution (1 cm³ equivalent).
///
/// The struct is intentionally kept at 4 bytes so it packs tightly into
/// arrays and GPU storage buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(C)]
pub struct Voxel {
    /// Material index (0 = air/empty, 1-255 = material).
    pub material_id: MaterialId,
    /// Emission intensity in the range [0, 255].  Mapped to HDR when shading.
    /// 255 ≈ a fully saturated emissive surface at max brightness.
    pub emissive: u8,
    /// Destruction accumulation.  0 = intact, 255 = completely destroyed.
    pub damage: u8,
    /// Status flags (`VoxelFlags`).
    pub flags: VoxelFlags,
}

impl Voxel {
    /// Construct a solid, non-emissive voxel with no damage.
    #[inline]
    pub const fn solid(material_id: MaterialId) -> Self {
        Self {
            material_id,
            emissive: 0,
            damage: 0,
            flags: VoxelFlags::OCCUPIED,
        }
    }

    /// Construct an emissive voxel (e.g. lava, screens, fire).
    #[inline]
    pub const fn emissive(material_id: MaterialId, intensity: u8) -> Self {
        // Cannot use `VoxelFlags::from_bits_retain` in `const` context on older Rust,
        // so compose flags manually.
        Self {
            material_id,
            emissive: intensity,
            damage: 0,
            flags: VoxelFlags::from_bits_truncate(
                VoxelFlags::OCCUPIED.bits() | VoxelFlags::EMISSIVE.bits(),
            ),
        }
    }

    /// Returns `true` if the voxel occupies space (not air).
    #[inline]
    pub fn is_solid(&self) -> bool {
        self.flags.contains(VoxelFlags::OCCUPIED)
    }

    /// Returns `true` if the voxel emits light.
    #[inline]
    pub fn is_emissive(&self) -> bool {
        self.flags.contains(VoxelFlags::EMISSIVE)
    }

    /// Returns `true` if the voxel has been modified this frame.
    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.flags.contains(VoxelFlags::MODIFIED)
    }

    /// Mark the voxel as dirty (modified this frame).
    #[inline]
    pub fn mark_dirty(&mut self) {
        self.flags.insert(VoxelFlags::MODIFIED);
    }

    /// Clear the dirty flag (called after GPU sync).
    #[inline]
    pub fn clear_dirty(&mut self) {
        self.flags.remove(VoxelFlags::MODIFIED);
    }

    /// The canonical "empty / air" voxel.
    pub const AIR: Self = Self {
        material_id: 0,
        emissive: 0,
        damage: 0,
        flags: VoxelFlags::empty(),
    };
}

// ── DAG node ─────────────────────────────────────────────────────────────────

/// Number of children per DAG node (2³ = 8 octants).
pub const CHILDREN_PER_NODE: usize = 8;

/// A node in the HashDAG hierarchy.
///
/// Each node is an octree node covering one cell at a given hierarchy level.
/// Children are either leaf voxels (at level 0) or indices into the node
/// pool at the next-lower level.
///
/// The node's "hash key" used in `HashDAG` is derived from the content of
/// this struct — identical subtrees share the same key and the same
/// allocation, achieving the signature memory compression of a DAG.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DAGNode {
    /// Child slots.  `u32::MAX` means the slot is empty (all-air subtree,
    /// which is never actually stored — just implied by absence).
    ///
    /// At level 0 the children ARE the voxel data; we store the `Voxel` packed
    /// into a `u32` via `Voxel::pack` / `Voxel::unpack`.
    pub children: [u32; CHILDREN_PER_NODE],

    /// Pre-computed occupancy bitmask: bit `i` is set when child `i` is
    /// solid (non-air).  Allows quick skip of empty octants without
    /// dereferencing child nodes.
    pub occupancy_mask: u8,

    /// Pre-computed emissivity bitmask: bit `i` is set when child `i`
    /// contains at least one emissive voxel.
    pub emissive_mask: u8,
}

impl DAGNode {
    /// Create a fully-empty node (all children absent).
    pub const fn empty() -> Self {
        Self {
            children: [u32::MAX; CHILDREN_PER_NODE],
            occupancy_mask: 0,
            emissive_mask: 0,
        }
    }

    /// Returns `true` if no children are solid.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.occupancy_mask == 0
    }
}

impl Voxel {
    /// Pack a `Voxel` into a `u32` for storage in a leaf `DAGNode`.
    #[inline]
    pub fn pack(self) -> u32 {
        (self.material_id as u32)
            | ((self.emissive as u32) << 8)
            | ((self.damage as u32) << 16)
            | ((self.flags.bits() as u32) << 24)
    }

    /// Unpack a `u32` produced by `Voxel::pack`.
    #[inline]
    pub fn unpack(raw: u32) -> Self {
        Self {
            material_id: (raw & 0xFF) as u8,
            emissive: ((raw >> 8) & 0xFF) as u8,
            damage: ((raw >> 16) & 0xFF) as u8,
            flags: VoxelFlags::from_bits_truncate(((raw >> 24) & 0xFF) as u8),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voxel_roundtrip() {
        let v = Voxel::emissive(42, 200);
        let packed = v.pack();
        let v2 = Voxel::unpack(packed);
        assert_eq!(v, v2);
    }

    #[test]
    fn voxel_flags() {
        let mut v = Voxel::solid(1);
        assert!(v.is_solid());
        assert!(!v.is_dirty());
        v.mark_dirty();
        assert!(v.is_dirty());
        v.clear_dirty();
        assert!(!v.is_dirty());
    }

    #[test]
    fn dag_node_empty() {
        let n = DAGNode::empty();
        assert!(n.is_empty());
        assert_eq!(n.occupancy_mask, 0);
    }
}
