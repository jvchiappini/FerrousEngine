/// A mesh instance placed in the scene with a full world-space transform.
///
/// ## Dynamic uniform buffer model
///
/// Instead of owning its own `wgpu::Buffer` and `wgpu::BindGroup`, each
/// `RenderObject` stores only its **slot index** in the renderer-wide
/// [`ModelBuffer`].  The `WorldPass` binds the single shared buffer once and
/// supplies `model_buf.offset(slot)` as the dynamic offset for each draw
/// call — reducing N bind-group switches to O(1) per frame.
///
/// ## Frustum culling
///
/// Each object carries a `local_aabb` (object-space bounding box).  Before
/// building `DrawCommand`s, `Renderer::build_base_packet` transforms it to
/// world space and tests it against the camera frustum — objects outside the
/// frustum produce no `DrawCommand` at all.
use glam::Mat4;

use crate::geometry::Mesh;
use crate::scene::culling::Aabb;

pub struct RenderObject {
    /// Stable ID matching `ferrous_core::scene::Handle`.
    pub id: u64,
    pub mesh: Mesh,
    /// Current model matrix (column-major). The world-space position is
    /// always `matrix.w_axis.xyz` — there is no separate `position` field
    /// to avoid storing the same data twice.
    pub matrix: Mat4,
    /// Object-space AABB used for frustum culling.
    pub local_aabb: Aabb,
    /// Slot index in the renderer-wide `ModelBuffer`.
    pub slot: usize,
}

impl RenderObject {
    /// Creates a `RenderObject` assigned to `slot` in the `ModelBuffer`.
    ///
    /// `local_aabb` should tightly enclose the mesh in object space.
    /// Pass [`Aabb::unit_cube()`] for the built-in cube primitive.
    ///
    /// The caller is responsible for writing the initial matrix via
    /// `model_buf.write(queue, slot, &matrix)`.
    pub fn new(
        _device: &wgpu::Device,
        id: u64,
        mesh: Mesh,
        matrix: Mat4,
        slot: usize,
    ) -> Self {
        Self {
            id,
            mesh,
            matrix,
            // Default to a unit cube AABB; callers can override for non-cube meshes.
            local_aabb: Aabb::unit_cube(),
            slot,
        }
    }

    /// Returns the AABB transformed to world space by the current matrix.
    #[inline]
    pub fn world_aabb(&self) -> Aabb {
        self.local_aabb.transform(&self.matrix)
    }

    /// Returns the current matrix (no GPU read — CPU side only).
    #[inline]
    pub fn current_matrix(&self) -> &Mat4 {
        &self.matrix
    }

    /// Updates the CPU-side matrix. The caller must also call
    /// `model_buf.write(queue, self.slot, &matrix)` to push it to the GPU.
    #[inline]
    pub fn set_matrix(&mut self, matrix: Mat4) {
        self.matrix = matrix;
    }
}


