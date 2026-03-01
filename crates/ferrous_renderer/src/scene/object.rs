/// A mesh instance placed in the scene at a given world-space position.
///
/// Owns the model-matrix GPU buffer and the corresponding bind group so that
/// each object can be drawn with a unique transform without CPU↔GPU round
/// trips on every draw call.
use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::geometry::Mesh;
use crate::resources::buffer;

pub struct RenderObject {
    pub mesh: Mesh,
    pub position: glam::Vec3,
    /// GPU buffer containing the 4×4 model matrix (column-major `f32`).
    model_buffer: wgpu::Buffer,
    /// Bind group referencing `model_buffer` at binding 0 (group 1).
    pub model_bind_group: Arc<wgpu::BindGroup>,
}

impl RenderObject {
    /// Allocates GPU resources and places the object at `position`.
    pub fn new(
        device: &wgpu::Device,
        mesh: Mesh,
        position: glam::Vec3,
        model_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let mat = glam::Mat4::from_translation(position);
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Model Matrix Buffer"),
            contents: bytemuck::cast_slice(&[mat.to_cols_array()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Model Bind Group"),
            layout: model_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        }));
        Self { mesh, position, model_buffer: buf, model_bind_group: bind_group }
    }

    /// Moves the object to `pos` and uploads the new matrix to the GPU.
    pub fn set_position(&mut self, queue: &wgpu::Queue, pos: glam::Vec3) {
        self.position = pos;
        let mat = glam::Mat4::from_translation(pos);
        buffer::update_uniform(queue, &self.model_buffer, &mat.to_cols_array());
    }
}
