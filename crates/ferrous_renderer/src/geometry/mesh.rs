/// A drawable GPU mesh — a pair of vertex/index buffers plus the index count.
///
/// Meshes are cheaply cloneable because the underlying buffers are `Arc`-
/// wrapped.  Creating a second handle to a mesh does **not** copy GPU memory.
use std::sync::Arc;
use wgpu;
use glam;

#[derive(Clone)]
pub struct Mesh {
    pub vertex_buffer: Arc<wgpu::Buffer>,
    pub index_buffer: Arc<wgpu::Buffer>,
    pub index_count: u32,
    /// Number of vertices in the vertex buffer.
    pub vertex_count: u32,
    /// Index format used when binding this mesh.
    pub index_format: wgpu::IndexFormat,
    /// Local-space axis-aligned bounding box.
    pub aabb: crate::scene::culling::Aabb,
}

impl Mesh {
    /// Convenience constructor — creates a unit cube centred at the origin.
    pub fn cube(device: &wgpu::Device) -> Self {
        super::primitives::cube(device)
    }

    /// Convenience constructor for a UV sphere.  Arguments match the
    /// `primitives::sphere` helper; radius is applied in the mesh itself
    /// (typically callers will use a unit sphere and scale the transform).
    pub fn sphere(device: &wgpu::Device, radius: f32, latitudes: u32, longitudes: u32) -> Self {
        super::primitives::sphere(device, radius, latitudes, longitudes)
    }

    pub fn empty(device: &wgpu::Device) -> Self {
        use wgpu::util::DeviceExt;
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Empty VB"),
            contents: &[0u8; 4],
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Empty IB"),
            contents: &[0u8; 4],
            usage: wgpu::BufferUsages::INDEX,
        });
        Self {
            vertex_buffer: Arc::new(vb),
            index_buffer: Arc::new(ib),
            index_count: 0,
            vertex_count: 0,
            index_format: wgpu::IndexFormat::Uint16,
            aabb: crate::scene::culling::Aabb::new(glam::Vec3::ZERO, glam::Vec3::ZERO),
        }
    }
}
