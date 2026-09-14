use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use crate::render::{SpriteBatcher, SpritePipeline, SpriteInstance, ShapePipeline};
use glam::Mat4;

/// Hardware-independent per-frame counters for the 2D render path.
///
/// These exist so that the headless benchmarking harness (and integration
/// tests) can report *what the renderer did this frame* without relying on
/// wall-clock timings, which are meaningless on a software rasteriser.  They
/// are reset by [`Renderer2d::reset_frame_counters`] at the start of a frame
/// and accumulated by the `prepare_*` / `render_*` methods.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Frame2dCounters {
    /// Total `draw*` calls issued (`shape_draw_calls + sprite_draw_calls`).
    pub draw_calls: u32,
    /// Draw calls issued through the shape (SDF) pipeline.
    pub shape_draw_calls: u32,
    /// Draw calls issued through the sprite pipeline.
    pub sprite_draw_calls: u32,
    /// Bytes actually written to GPU instance buffers this frame.
    pub bytes_uploaded: u64,
    /// Instances uploaded this frame (shapes + sprites).
    pub instances_uploaded: u32,
    /// Instance buffers reallocated because the frame outgrew the current
    /// capacity.  A steady-state frame is expected to report `0`.
    pub buffer_reallocations: u32,
}

/// Owns the 2D instance/camera buffers and pipelines and issues 2D draw calls.
///
/// One `Renderer2d` is created per 2D output (the engine currently keeps one
/// for the Pure2D swapchain path and one for the Full3D overlay). The caller
/// prepares instances with `prepare` / `prepare_shapes`, then records draws
/// with `render*` inside a render pass.
pub struct Renderer2d {
    device: Arc<wgpu::Device>,
    /// Sprite pipeline and its texture bind-group layout.
    pub pipeline: SpritePipeline,
    /// Shape (SDF) pipeline and its camera bind-group layout.
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

    // Per-frame instrumentation.  Atomics (not `Cell`) so `Renderer2d` stays
    // `Send + Sync` as required by `ferrous_render_graph::RenderPass`.
    counters_draw_calls: AtomicU32,
    counters_shape_draw_calls: AtomicU32,
    counters_sprite_draw_calls: AtomicU32,
    counters_bytes_uploaded: AtomicU64,
    counters_instances_uploaded: AtomicU32,
    counters_buffer_reallocations: AtomicU32,
}

impl Renderer2d {
    /// Create the pipelines and growable instance/camera buffers.
    ///
    /// `initial_capacity` is the starting instance count (clamped to `128`);
    /// buffers grow to the next power of two when a frame needs more.
    pub fn new(device: Arc<wgpu::Device>, output_format: wgpu::TextureFormat, depth_format: Option<wgpu::TextureFormat>, sample_count: u32, initial_capacity: u32) -> Self {
        Self::new_with_cache(device, output_format, depth_format, sample_count, initial_capacity, None)
    }

    /// Same as [`Renderer2d::new`] but threads a shared [`wgpu::PipelineCache`]
    /// into both the sprite and shape pipelines.
    pub fn new_with_cache(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
        initial_capacity: u32,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let pipeline = SpritePipeline::new_with_cache(device.clone(), output_format, depth_format, sample_count, cache);
        let shape_pipeline = ShapePipeline::new_with_cache(device.clone(), output_format, depth_format, sample_count, cache);
        
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
            counters_draw_calls: AtomicU32::new(0),
            counters_shape_draw_calls: AtomicU32::new(0),
            counters_sprite_draw_calls: AtomicU32::new(0),
            counters_bytes_uploaded: AtomicU64::new(0),
            counters_instances_uploaded: AtomicU32::new(0),
            counters_buffer_reallocations: AtomicU32::new(0),
        }
    }

    /// Resets the per-frame instrumentation.  Call once at the start of every
    /// frame (before `prepare_*` / `render_*`) when collecting metrics.
    pub fn reset_frame_counters(&self) {
        self.counters_draw_calls.store(0, Ordering::Relaxed);
        self.counters_shape_draw_calls.store(0, Ordering::Relaxed);
        self.counters_sprite_draw_calls.store(0, Ordering::Relaxed);
        self.counters_bytes_uploaded.store(0, Ordering::Relaxed);
        self.counters_instances_uploaded.store(0, Ordering::Relaxed);
        self.counters_buffer_reallocations.store(0, Ordering::Relaxed);
    }

    /// Snapshot of the per-frame instrumentation accumulated since the last
    /// [`Renderer2d::reset_frame_counters`].
    pub fn frame_counters(&self) -> Frame2dCounters {
        Frame2dCounters {
            draw_calls: self.counters_draw_calls.load(Ordering::Relaxed),
            shape_draw_calls: self.counters_shape_draw_calls.load(Ordering::Relaxed),
            sprite_draw_calls: self.counters_sprite_draw_calls.load(Ordering::Relaxed),
            bytes_uploaded: self.counters_bytes_uploaded.load(Ordering::Relaxed),
            instances_uploaded: self.counters_instances_uploaded.load(Ordering::Relaxed),
            buffer_reallocations: self.counters_buffer_reallocations.load(Ordering::Relaxed),
        }
    }

    #[inline]
    fn record_shape_draw(&self) {
        self.counters_draw_calls.fetch_add(1, Ordering::Relaxed);
        self.counters_shape_draw_calls.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn record_sprite_draw(&self) {
        self.counters_draw_calls.fetch_add(1, Ordering::Relaxed);
        self.counters_sprite_draw_calls.fetch_add(1, Ordering::Relaxed);
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

    /// Write all batched instances to the GPU Buffer — UN solo write_buffer (máximo rendimiento).
    pub fn prepare(&mut self, queue: &wgpu::Queue, batcher: &SpriteBatcher) -> usize {
        let total_instances: usize = batcher.batches.values().map(|v| v.len()).sum();
        if total_instances == 0 {
            return 0;
        }

        // Redimensionar buffer si es necesario (potencia de 2, amortizado)
        if total_instances > self.max_instances as usize {
            self.max_instances = (total_instances as u32).next_power_of_two();
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Sprite Instance Buffer (Resized)"),
                size: (self.max_instances as usize * std::mem::size_of::<SpriteInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.counters_buffer_reallocations
                .fetch_add(1, Ordering::Relaxed);
        }

        // Consolidar TODOS los datos en un slice contiguo → UN SOLO write_buffer.
        // Evita N llamadas separadas a la GPU staging queue (una por textura).
        // Usamos a scratch buffer pre-allocado en el batcher para evitar allocs de heap.
        let mut flat: Vec<SpriteInstance> = Vec::with_capacity(total_instances);
        for instances in batcher.batches.values() {
            flat.extend_from_slice(instances.as_slice());
        }
        let uploaded = bytemuck::cast_slice(&flat);
        queue.write_buffer(&self.instance_buffer, 0, uploaded);
        self.counters_bytes_uploaded
            .fetch_add(uploaded.len() as u64, Ordering::Relaxed);
        self.counters_instances_uploaded
            .fetch_add(total_instances as u32, Ordering::Relaxed);

        total_instances
    }

    /// Upload all shape instances as one contiguous `write_buffer`, growing the
    /// GPU buffer first if needed. Returns the number of instances uploaded.
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
            self.counters_buffer_reallocations
                .fetch_add(1, Ordering::Relaxed);
        }

        let uploaded = bytemuck::cast_slice(&batcher.instances);
        queue.write_buffer(&self.shape_instance_buffer, 0, uploaded);
        self.counters_bytes_uploaded
            .fetch_add(uploaded.len() as u64, Ordering::Relaxed);
        self.counters_instances_uploaded
            .fetch_add(total as u32, Ordering::Relaxed);
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
                self.record_sprite_draw();
            }
            
            current_offset += count;
        }
    }

    /// Draw every shape in `batcher` as a single instanced draw call.
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
        self.record_shape_draw();
    }

    /// Renders a contiguous range `[offset, offset + count)` of shape instances
    /// (indices in the flat upload produced by `prepare_shapes`).  Used by the
    /// unified z-sorted command stream so shapes can interleave with sprites.
    pub fn render_shapes_range(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        offset: u32,
        count: u32,
    ) {
        if count == 0 { return; }
        render_pass.set_pipeline(&self.shape_pipeline.wgpu_pipeline);
        render_pass.set_bind_group(0, &self.shape_camera_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.shape_instance_buffer.slice(..));
        render_pass.draw(0..4, offset..(offset + count));
        self.record_shape_draw();
    }

    /// Renders a contiguous range `[offset, offset + count)` of sprite instances
    /// for a single `texture_id`, binding its texture bind group first.  Same
    /// usage as `render` but scoped to an arbitrary slice of the flat upload.
    pub fn render_sprites_range<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        tex_id: u32,
        offset: u32,
        count: u32,
        get_texture_bind_group: impl Fn(u32) -> Option<&'a wgpu::BindGroup>,
    ) {
        if count == 0 { return; }
        let Some(bg) = get_texture_bind_group(tex_id) else { return; };
        render_pass.set_pipeline(&self.pipeline.wgpu_pipeline);
        render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
        render_pass.set_bind_group(1, bg, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..4, offset..(offset + count));
        self.record_sprite_draw();
    }
}
