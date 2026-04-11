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
    /// Unit cube centred at the origin.
    pub fn cube(device: &wgpu::Device) -> Self {
        super::primitives::cube(device)
    }

    /// UV sphere with explicit latitudes / longitudes.
    pub fn sphere(device: &wgpu::Device, radius: f32, latitudes: u32, longitudes: u32) -> Self {
        super::primitives::sphere(device, radius, latitudes, longitudes)
    }

    /// Cylinder / cone / frustum.
    ///
    /// Set `radius_top = 0` for a cone, `open_ended = true` to skip caps.
    pub fn cylinder(
        device: &wgpu::Device,
        radius_top: f32,
        radius_bottom: f32,
        height: f32,
        segments: u32,
        rings: u32,
        open_ended: bool,
    ) -> Self {
        super::primitives::cylinder(device, radius_top, radius_bottom, height, segments, rings, open_ended)
    }

    /// Torus (donut) — full circle arc.
    pub fn torus(
        device: &wgpu::Device,
        radius: f32,
        tube: f32,
        radial_segments: u32,
        tubular_segments: u32,
    ) -> Self {
        super::primitives::torus(device, radius, tube, radial_segments, tubular_segments, std::f32::consts::TAU)
    }

    /// Subdivided flat plane in the XZ plane (Y = 0).
    pub fn plane(device: &wgpu::Device, width: f32, height: f32, width_segs: u32, height_segs: u32) -> Self {
        super::primitives::plane(device, width, height, width_segs, height_segs)
    }

    /// Capsule — cylinder body + hemispherical caps.
    pub fn capsule(device: &wgpu::Device, radius: f32, height: f32, radial: u32, cap: u32) -> Self {
        super::primitives::capsule(device, radius, height, radial, cap)
    }

    /// Flat filled circle disc in the XZ plane.
    pub fn circle(device: &wgpu::Device, radius: f32, segments: u32) -> Self {
        super::primitives::circle(device, radius, segments)
    }

    /// Ring (annulus) in the XZ plane.
    pub fn ring(device: &wgpu::Device, inner_radius: f32, outer_radius: f32, segments: u32, rings: u32) -> Self {
        super::primitives::ring(device, inner_radius, outer_radius, segments, rings)
    }

    /// A zero-vertex placeholder mesh — useful before real geometry is ready.
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
