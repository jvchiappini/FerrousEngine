/// A drawable GPU mesh — a pair of vertex/index buffers plus the index count.
///
/// Meshes are cheaply cloneable because the underlying buffers are `Arc`-
/// wrapped.  Creating a second handle to a mesh does **not** copy GPU memory.
use std::sync::Arc;

#[derive(Clone)]
pub struct Mesh {
    pub vertex_buffer: Arc<wgpu::Buffer>,
    pub index_buffer: Arc<wgpu::Buffer>,
    pub index_count: u32,
    /// Index format used when binding this mesh.
    pub index_format: wgpu::IndexFormat,
}

impl Mesh {
    /// Convenience constructor — creates a unit cube centred at the origin.
    pub fn cube(device: &wgpu::Device) -> Self {
        super::primitives::cube(device)
    }
}
