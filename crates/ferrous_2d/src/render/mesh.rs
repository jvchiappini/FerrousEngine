//! Generic textured triangle meshes for the Pure2D path.
//!
//! A mesh is an arbitrary indexed triangle list sampled from one registered
//! sprite texture.  It shares the sprite texture registry, the premultiplied
//! texture flag, the camera uniform, layers, masks and blend modes, so a mesh
//! interleaves with sprites in painter order.
//!
//! This module owns the CPU-side types and the per-frame batcher.  The GPU
//! pipeline lives in [`crate::render::pipeline::TexturedMeshPipeline`] and the
//! grow-only vertex/index buffers live in
//! [`crate::render::renderer::Renderer2d`].

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec4};

use crate::render::BlendMode2d;

/// One mesh vertex: position, texture coordinate and coverage.
///
/// Positions are in the mesh's own local space (the same space a sprite's
/// unit quad lives in before its model transform); the draw call's transform
/// maps them to world space.  `alpha` multiplies the sampled texel's coverage
/// and may vary per vertex.  `premultiplied` is filled by the renderer from
/// the texture registry (1 = the texels are already premultiplied) and must
/// be left at `0` by callers.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub struct MeshVertex2d {
    /// Local position; `.z` is usually `0.0` (the transform carries depth).
    pub position: [f32; 3],
    /// Texture coordinate in `[0, 1]` (or the sprite atlas convention).
    pub uv: [f32; 2],
    /// Straight coverage multiplied with the texel alpha.
    pub alpha: f32,
    /// `1.0` when the sampled texels are premultiplied (renderer-filled).
    pub premultiplied: f32,
}

impl MeshVertex2d {
    /// A fully opaque vertex.
    #[must_use]
    pub const fn new(position: [f32; 2], uv: [f32; 2]) -> Self {
        Self {
            position: [position[0], position[1], 0.0],
            uv,
            alpha: 1.0,
            premultiplied: 0.0,
        }
    }

    /// A vertex with explicit coverage.
    #[must_use]
    pub const fn with_alpha(position: [f32; 2], uv: [f32; 2], alpha: f32) -> Self {
        Self {
            position: [position[0], position[1], 0.0],
            uv,
            alpha,
            premultiplied: 0.0,
        }
    }

    /// Vertex-buffer layout describing this type to wgpu.
    ///
    /// The stride is 28 bytes: `Float32x3` position, `Float32x2` UV and two
    /// scalar attributes (coverage, premultiplied flag).
    pub const fn descriptor() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 20,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}

/// One queued mesh draw: a range in the batcher's vertex and index arenas.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct MeshDraw2d {
    /// Registered sprite texture id.
    pub texture_id: u32,
    /// Painter-order depth taken from the draw transform's z translation.
    pub depth: f32,
    /// Per-draw blend mode.
    pub blend: BlendMode2d,
    /// First arena vertex of this draw.
    pub first_vertex: u32,
    /// Number of arena vertices of this draw.
    pub vertex_count: u32,
    /// First arena index of this draw.
    pub first_index: u32,
    /// Number of arena indices (a multiple of three).
    pub index_count: u32,
}

/// Per-frame CPU arena plus one [`MeshDraw2d`] per accepted submission.
///
/// The arena is cleared per frame without releasing its capacity, so a steady
/// frame performs no heap allocation.  Indices are stored absolute in the
/// arena, so the renderer can copy a draw's range into an upload with a single
/// base-vertex offset.
#[derive(Default)]
pub struct MeshBatcher2d {
    /// Transformed vertices (world space).
    pub vertices: Vec<MeshVertex2d>,
    /// Absolute arena indices.
    pub indices: Vec<u32>,
    /// Accepted draws in submission order.
    pub draws: Vec<MeshDraw2d>,
}

impl MeshBatcher2d {
    /// Clears the arena and the draw list, keeping their capacity.
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.draws.clear();
    }

    /// Clears only the draw list, keeping the arena (used when the frame
    /// switches to the layered command log mid-frame).
    pub fn clear_draws(&mut self) {
        self.draws.clear();
    }

    /// True when no draw was accepted this frame.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.draws.is_empty()
    }

    /// Appends one textured mesh draw.
    ///
    /// Returns `false` (and changes nothing) when the mesh is empty, its index
    /// count is not a multiple of three, an index is out of range, or the
    /// arena would overflow its `u32` offsets.  Degenerate triangles are
    /// accepted: they rasterise no pixels.
    pub fn push(
        &mut self,
        texture_id: u32,
        vertices: &[MeshVertex2d],
        indices: &[u32],
        transform: Mat4,
        blend: BlendMode2d,
        premultiplied: bool,
    ) -> bool {
        if vertices.is_empty() || indices.is_empty() || !indices.len().is_multiple_of(3) {
            return false;
        }
        let vertex_count = vertices.len();
        if indices
            .iter()
            .any(|&index| usize::try_from(index).map_or(true, |value| value >= vertex_count))
        {
            return false;
        }
        let Some(first_vertex) = u32::try_from(self.vertices.len()).ok() else {
            return false;
        };
        let Some(first_index) = u32::try_from(self.indices.len()).ok() else {
            return false;
        };
        let Some(vertex_count_u32) = u32::try_from(vertex_count).ok() else {
            return false;
        };
        let Some(index_count_u32) = u32::try_from(indices.len()).ok() else {
            return false;
        };
        if first_vertex.checked_add(vertex_count_u32).is_none()
            || first_index.checked_add(index_count_u32).is_none()
        {
            return false;
        }
        let flag = if premultiplied { 1.0 } else { 0.0 };
        self.vertices.reserve(vertex_count);
        for vertex in vertices {
            let world = transform
                * Vec4::new(
                    vertex.position[0],
                    vertex.position[1],
                    vertex.position[2],
                    1.0,
                );
            let inverse_w = if world.w != 0.0 { 1.0 / world.w } else { 1.0 };
            self.vertices.push(MeshVertex2d {
                position: [
                    world.x * inverse_w,
                    world.y * inverse_w,
                    world.z * inverse_w,
                ],
                uv: vertex.uv,
                alpha: vertex.alpha,
                premultiplied: flag,
            });
        }
        self.indices
            .extend(indices.iter().map(|&index| index + first_vertex));
        self.draws.push(MeshDraw2d {
            texture_id,
            depth: transform.w_axis.z,
            blend,
            first_vertex,
            vertex_count: vertex_count_u32,
            first_index,
            index_count: index_count_u32,
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad() -> (Vec<MeshVertex2d>, Vec<u32>) {
        let vertices = vec![
            MeshVertex2d::new([-0.5, -0.5], [0.0, 0.0]),
            MeshVertex2d::new([0.5, -0.5], [1.0, 0.0]),
            MeshVertex2d::new([-0.5, 0.5], [0.0, 1.0]),
            MeshVertex2d::new([0.5, 0.5], [1.0, 1.0]),
        ];
        let indices = vec![0, 1, 2, 2, 1, 3];
        (vertices, indices)
    }

    #[test]
    fn a_quad_is_transformed_and_offset_into_the_arena() {
        let mut batch = MeshBatcher2d::default();
        let (vertices, indices) = quad();
        assert!(batch.push(
            7,
            &vertices,
            &indices,
            Mat4::from_translation(glam::Vec3::new(10.0, 20.0, 3.0)),
            BlendMode2d::Normal,
            true,
        ));
        assert_eq!(batch.draws.len(), 1);
        assert_eq!(batch.vertices.len(), 4);
        assert_eq!(batch.vertices[0].position, [9.5, 19.5, 3.0]);
        assert_eq!(batch.vertices[0].premultiplied, 1.0);
        assert_eq!(batch.draws[0].depth, 3.0);

        // A second draw offsets its indices by the existing vertex base.
        assert!(batch.push(
            7,
            &vertices,
            &indices,
            Mat4::IDENTITY,
            BlendMode2d::Normal,
            false,
        ));
        assert_eq!(batch.vertices.len(), 8);
        assert_eq!(&batch.indices[6..], &[4, 5, 6, 6, 5, 7]);
        assert_eq!(batch.draws[1].first_vertex, 4);
        assert_eq!(batch.draws[1].first_index, 6);
    }

    #[test]
    fn empty_degenerate_and_out_of_range_meshes_are_skipped() {
        let mut batch = MeshBatcher2d::default();
        assert!(!batch.push(
            1,
            &[],
            &[0, 1, 2],
            Mat4::IDENTITY,
            BlendMode2d::Normal,
            false
        ));
        let (vertices, _) = quad();
        assert!(!batch.push(
            1,
            &vertices,
            &[],
            Mat4::IDENTITY,
            BlendMode2d::Normal,
            false
        ));
        assert!(!batch.push(
            1,
            &vertices,
            &[0, 1],
            Mat4::IDENTITY,
            BlendMode2d::Normal,
            false
        ));
        assert!(!batch.push(
            1,
            &vertices,
            &[0, 1, 9],
            Mat4::IDENTITY,
            BlendMode2d::Normal,
            false
        ));
        assert!(batch.is_empty());
        assert!(batch.indices.is_empty());

        // A degenerate (zero-area) triangle is accepted and draws nothing.
        assert!(batch.push(
            1,
            &vertices,
            &[0, 0, 0],
            Mat4::IDENTITY,
            BlendMode2d::Normal,
            false
        ));
        assert_eq!(batch.draws.len(), 1);
    }
}
