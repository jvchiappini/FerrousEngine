/// GPU-side camera resources: the uniform buffer and its bind group.
///
/// `GpuCamera` owns the `wgpu::Buffer` that holds the view-projection matrix
/// and keeps the bind group that shaders can bind at group(0).  It is the
/// bridge between the CPU `Camera` (in `ferrous_core`) and the GPU pipeline.
use std::sync::Arc;

use ferrous_core::scene::{Camera, CameraUniform};

use crate::resources::buffer;

pub struct GpuCamera {
    pub uniform: CameraUniform,
    pub buffer: Arc<wgpu::Buffer>,
    pub bind_group: Arc<wgpu::BindGroup>,
}

impl GpuCamera {
    /// Allocates the GPU uniform buffer and creates a bind group using the
    /// provided layout.  The layout must have a single `UNIFORM` buffer entry
    /// at binding 0.
    pub fn new(
        device: &wgpu::Device,
        camera: &Camera,
        layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let mut uniform = CameraUniform::new();
        uniform.update_view_proj(camera);

        let buf = buffer::create_uniform(device, "Camera Uniform Buffer", &uniform);

        let bind_group = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        }));

        Self {
            uniform,
            buffer: buf,
            bind_group,
        }
    }

    /// Syncs the CPU `Camera` state to the GPU buffer.  Call once per frame
    /// before any render passes execute.
    pub fn sync(&mut self, queue: &wgpu::Queue, camera: &Camera) {
        self.uniform.update_view_proj(camera);
        buffer::update_uniform(queue, &self.buffer, &self.uniform);
    }
}
