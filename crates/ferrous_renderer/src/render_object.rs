// internal helper representing a mesh instance along with its GPU state
// (model matrix buffer and bind group).  Not exported outside of this
// crate -- it's purely an implementation detail of `Renderer`.

use crate::mesh;
use glam;
use wgpu::util::DeviceExt;

pub(crate) struct RenderObject {
    pub mesh: mesh::Mesh,
    /// world-space translation only for now
    pub position: glam::Vec3,
    pub(crate) model_buffer: wgpu::Buffer,
    pub(crate) model_bind_group: wgpu::BindGroup,
}

impl RenderObject {
    /// create a new render object from a mesh and initial position.
    pub fn new(
        mesh: mesh::Mesh,
        position: glam::Vec3,
        device: &wgpu::Device,
        model_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        // create buffer holding the model matrix
        let mat: glam::Mat4 = glam::Mat4::from_translation(position);
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Model Matrix Buffer"),
            contents: bytemuck::cast_slice(&[mat.to_cols_array()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Model Bind Group"),
            layout: model_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        Self {
            mesh,
            position,
            model_buffer: buffer,
            model_bind_group: bind_group,
        }
    }

    /// update the position and write new matrix to GPU buffer
    pub fn set_position(&mut self, queue: &wgpu::Queue, pos: glam::Vec3) {
        self.position = pos;
        let mat: glam::Mat4 = glam::Mat4::from_translation(pos);
        queue.write_buffer(
            &self.model_buffer,
            0,
            bytemuck::cast_slice(&[mat.to_cols_array()]),
        );
    }
}
