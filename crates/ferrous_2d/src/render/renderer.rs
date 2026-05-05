use std::sync::Arc;
use crate::render::{SpriteBatcher, SpritePipeline, SpriteInstance, ShapePipeline, ShapeBatcher};
use glam::Mat4;

pub struct Renderer2d {
    device: Arc<wgpu::Device>,
    pub pipeline: SpritePipeline,
    pub shape_pipeline: ShapePipeline,
    
    // Buffers
    instance_buffer: wgpu::Buffer,
    shape_instance_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    
    camera_bind_group: wgpu::BindGroup,
    shape_camera_bind_group: wgpu::BindGroup,
    
    // Capacity
    max_instances: u32,
    max_shape_instances: u32,
}

impl Renderer2d {
    pub fn new(device: Arc<wgpu::Device>, output_format: wgpu::TextureFormat, sample_count: u32, initial_capacity: u32) -> Self {
        let pipeline = SpritePipeline::new(device.clone(), output_format, sample_count);
        let shape_pipeline = ShapePipeline::new(device.clone(), output_format, sample_count);
        
        let max_instances = initial_capacity.max(128);
        
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sprite Instance Buffer"),
            size: (max_instances as usize * std::mem::size_of::<SpriteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shape_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shape Instance Buffer"),
            size: (max_instances as usize * std::mem::size_of::<crate::render::types::ShapeInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sprite Camera Uniform Buffer"),
            size: std::mem::size_of::<crate::render::types::Uniform2d>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &pipeline.camera_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
            ],
            label: Some("Sprite Camera Bind Group"),
        });

        let shape_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &shape_pipeline.camera_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
            ],
            label: Some("Shape Camera Bind Group"),
        });

        Self {
            device,
            pipeline,
            shape_pipeline,
            instance_buffer,
            shape_instance_buffer,
            camera_buffer,
            camera_bind_group,
            shape_camera_bind_group,
            max_instances,
            max_shape_instances: max_instances,
        }
    }

    /// Update the Orthographic Camera Matrix and Resolution
    pub fn update_camera(&self, queue: &wgpu::Queue, proj_view: Mat4, resolution: glam::Vec2) {
        let uniform = crate::render::types::Uniform2d {
            view_proj: proj_view.to_cols_array(),
            resolution: resolution.to_array(),
            padding: [0.0; 2],
        };
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// Write all batched instances to the GPU Buffer (Resizes if needed)
    pub fn prepare(&mut self, queue: &wgpu::Queue, batcher: &SpriteBatcher) -> usize {
        let total_instances: usize = batcher.batches.values().map(|v| v.len()).sum();
        if total_instances == 0 {
            return 0;
        }

        // Check if we need to resize the instance buffer
        if total_instances > self.max_instances as usize {
            self.max_instances = (total_instances as u32).next_power_of_two();
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Sprite Instance Buffer (Resized)"),
                size: (self.max_instances as usize * std::mem::size_of::<SpriteInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        // Upload data consecutively
        let mut offset = 0;
        for instances in batcher.batches.values() {
            if instances.is_empty() { continue; }
            let bytes = bytemuck::cast_slice(instances.as_slice());
            queue.write_buffer(&self.instance_buffer, offset, bytes);
            offset += bytes.len() as u64;
        }

        total_instances
    }

    pub fn prepare_shapes(&mut self, queue: &wgpu::Queue, batcher: &crate::render::ShapeBatcher) -> usize {
        let total = batcher.instances.len();
        if total == 0 { return 0; }

        if total > self.max_shape_instances as usize {
            self.max_shape_instances = (total as u32).next_power_of_two();
            self.shape_instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Shape Instance Buffer (Resized)"),
                size: (self.max_shape_instances as usize * std::mem::size_of::<crate::render::types::ShapeInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        queue.write_buffer(&self.shape_instance_buffer, 0, bytemuck::cast_slice(&batcher.instances));
        total
    }

    /// Renders the prepared sprite instances.
    pub fn render<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        batcher: &'a SpriteBatcher,
        mut get_texture_bind_group: impl FnMut(u32) -> Option<&'a wgpu::BindGroup>,
    ) {
        render_pass.set_pipeline(&self.pipeline.wgpu_pipeline);
        render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));

        let mut current_offset = 0;
        
        // Draw each batch (grouped by texture ID)
        for (&tex_id, instances) in &batcher.batches {
            let count = instances.len() as u32;
            if count == 0 { continue; }

            // Bind the unique texture for this batch
            if let Some(bg) = get_texture_bind_group(tex_id) {
                render_pass.set_bind_group(1, bg, &[]);
                // We draw 4 vertices (quad) per instance
                render_pass.draw(0..4, current_offset..(current_offset + count));
            }
            
            current_offset += count;
        }
    }

    pub fn render_shapes<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        batcher: &'a crate::render::ShapeBatcher,
    ) {
        if batcher.instances.is_empty() { return; }
        render_pass.set_pipeline(&self.shape_pipeline.wgpu_pipeline);
        render_pass.set_bind_group(0, &self.shape_camera_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.shape_instance_buffer.slice(..));
        render_pass.draw(0..4, 0..batcher.instances.len() as u32);
    }
}
