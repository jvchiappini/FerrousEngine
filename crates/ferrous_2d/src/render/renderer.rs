use crate::render::{
    BlendMode2d, Grid2dParams, GridPipeline, ShapeGpu, ShapePipeline, SpriteBatcher,
    SpriteInstance, SpritePipeline, Text2dPipeline, TextInstance,
};
use glam::Mat4;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Hardware-independent per-frame counters for the 2D render path.
///
/// These exist so that the headless benchmarking harness (and integration
/// tests) can report *what the renderer did this frame* without relying on
/// wall-clock timings, which are meaningless on a software rasteriser.  They
/// are reset by [`Renderer2d::reset_frame_counters`] at the start of a frame
/// and accumulated by the `prepare_*` / `render_*` methods.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Frame2dCounters {
    /// Total `draw*` calls issued
    /// (`shape_draw_calls + sprite_draw_calls + text_draw_calls`).
    pub draw_calls: u32,
    /// Draw calls issued through the shape (SDF) pipeline.
    pub shape_draw_calls: u32,
    /// Draw calls issued through the sprite pipeline.
    pub sprite_draw_calls: u32,
    /// Draw calls issued through the MSDF text pipeline.
    pub text_draw_calls: u32,
    /// Bytes actually written to GPU instance buffers this frame.
    pub bytes_uploaded: u64,
    /// Instances uploaded this frame (shapes + sprites).
    pub instances_uploaded: u32,
    /// Instance buffers reallocated because the frame outgrew the current
    /// capacity.  A steady-state frame is expected to report `0`.
    pub buffer_reallocations: u32,
    /// Instance instances discarded by CPU-side 2D culling because their world
    /// AABB did not intersect the visible camera rect (shapes + sprites).
    /// `0` when culling is disabled (`Render2dConfig::cull_2d == false`).
    pub culled_instances: u32,
    /// Retained static 2D batches drawn this frame.  Their instances are *not*
    /// uploaded per frame (the buffer is persistent), so they contribute to
    /// `draw_calls`/`shape_draw_calls` but not to `instances_uploaded` /
    /// `bytes_uploaded`.  `0` when `Render2dConfig::static_batches == false`.
    pub static_batches_drawn: u32,
    /// Shape instances covered by the retained static batches drawn this frame.
    pub static_instances_drawn: u32,
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
    /// MSDF text pipeline and its font-atlas bind-group layout.
    pub text_pipeline: Text2dPipeline,

    // Buffers
    instance_buffer: wgpu::Buffer,
    shape_instance_buffer: wgpu::Buffer,
    text_instance_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,

    camera_bind_group: wgpu::BindGroup,
    shape_camera_bind_group: wgpu::BindGroup,
    text_camera_bind_group: wgpu::BindGroup,

    // Capacity
    max_instances: u32,
    max_shape_instances: u32,
    max_text_instances: u32,

    // Per-frame instrumentation.  Atomics (not `Cell`) so `Renderer2d` stays
    // `Send + Sync` as required by `ferrous_render_graph::RenderPass`.
    counters_draw_calls: AtomicU32,
    counters_shape_draw_calls: AtomicU32,
    counters_sprite_draw_calls: AtomicU32,
    counters_text_draw_calls: AtomicU32,
    counters_bytes_uploaded: AtomicU64,
    counters_instances_uploaded: AtomicU32,
    counters_buffer_reallocations: AtomicU32,
    counters_culled_instances: AtomicU32,
    counters_static_batches_drawn: AtomicU32,
    counters_static_instances_drawn: AtomicU32,

    /// Reusable staging buffer for the flattened sprite upload.  Reusing it
    /// turns the per-frame `Vec::with_capacity` + `extend` (one heap alloc and
    /// one grow per dynamic frame, measured as part of the 56-92 allocs/frame
    /// in the audit) into an in-place `clear()` + `extend_from_slice` that
    /// keeps the previous frame's capacity.  Gated by [`Self::scratch_reuse`]
    /// so a consumer can A/B compare against the allocating path.
    scratch_sprites: Vec<SpriteInstance>,
    /// Reusable staging buffer for packing [`crate::render::ShapeInstance`]s
    /// into the compact [`ShapeGpu`] upload layout.
    scratch_shapes: Vec<ShapeGpu>,
    /// When `false`, [`Self::prepare`] allocates a fresh `Vec` per frame
    /// instead of reusing [`Self::scratch_sprites`].  Defaults to `true`.
    scratch_reuse: bool,

    /// Device pixels per logical pixel, passed to `shape_2d.wgsl` in
    /// `Uniform2d::padding.x`. Used to resolve a per-instance `min_px`
    /// (logical px) into world units. Stored as bits so `update_camera(&self)`
    /// can read it without `&mut self`.
    pixel_ratio_bits: AtomicU32,
    /// `1` = legacy line stabiliser (pre-Phase-1 A/B switch, `?render=legacy`).
    /// Sent to the shader in `Uniform2d::padding.y`.
    legacy_2d: AtomicU32,
    /// Dash table (16 patterns Ã— up to 8 lengths, metres). Index 0 is unused.
    /// Written rarely (pattern registration), read every frame when the
    /// camera uniform is uploaded.
    dash_table: Mutex<[[f32; 8]; 16]>,
    /// Per-frame gradient table, written into every view uniform (see `GradientTable2d`).
    gradient_table: Mutex<Vec<[f32; 4]>>,

    /// Procedural background grid (optional).
    pub grid_pipeline: GridPipeline,
    grid_uniform_buffer: wgpu::Buffer,
    grid_bind_group: wgpu::BindGroup,
    grid_params: Option<Grid2dParams>,

    /// Per-view camera/grid resources for the multi-viewport Pure2D path.
    /// Index `i` belongs to `Renderer`'s `views_2d[i]`.
    view_camera_buffers: Vec<wgpu::Buffer>,
    view_shape_bgs: Vec<wgpu::BindGroup>,
    view_sprite_bgs: Vec<wgpu::BindGroup>,
    view_text_bgs: Vec<wgpu::BindGroup>,
    view_grid_buffers: Vec<wgpu::Buffer>,
    view_grid_bgs: Vec<wgpu::BindGroup>,
}

impl Renderer2d {
    /// Create the pipelines and growable instance/camera buffers.
    ///
    /// `initial_capacity` is the starting instance count (clamped to `128`);
    /// buffers grow to the next power of two when a frame needs more.
    pub fn new(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
        initial_capacity: u32,
    ) -> Self {
        Self::new_with_cache(
            device,
            output_format,
            depth_format,
            sample_count,
            initial_capacity,
            None,
        )
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
        let pipeline = SpritePipeline::new_with_cache(
            device.clone(),
            output_format,
            depth_format,
            sample_count,
            cache,
        );
        let shape_pipeline = ShapePipeline::new_with_cache(
            device.clone(),
            output_format,
            depth_format,
            sample_count,
            cache,
        );
        let text_pipeline = Text2dPipeline::new_with_cache(
            device.clone(),
            output_format,
            depth_format,
            sample_count,
            cache,
        );
        let grid_pipeline =
            GridPipeline::new_with_cache(device.clone(), output_format, sample_count, cache);

        let max_instances = initial_capacity.max(128);

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sprite Instance Buffer"),
            size: (max_instances as usize * std::mem::size_of::<SpriteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shape_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shape Instance Buffer"),
            size: (max_instances as usize * std::mem::size_of::<ShapeGpu>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let text_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Text Instance Buffer"),
            size: (max_instances as usize * std::mem::size_of::<TextInstance>()) as u64,
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
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("Sprite Camera Bind Group"),
        });

        let shape_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &shape_pipeline.camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("Shape Camera Bind Group"),
        });

        let text_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &text_pipeline.camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
            label: Some("Text Camera Bind Group"),
        });

        let grid_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Grid Uniform Buffer"),
            size: std::mem::size_of::<crate::render::types::Grid2dUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let grid_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &grid_pipeline.camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: grid_uniform_buffer.as_entire_binding(),
            }],
            label: Some("Grid Bind Group"),
        });

        Self {
            device,
            pipeline,
            shape_pipeline,
            text_pipeline,
            instance_buffer,
            shape_instance_buffer,
            text_instance_buffer,
            camera_buffer,
            camera_bind_group,
            shape_camera_bind_group,
            text_camera_bind_group,
            max_instances,
            max_shape_instances: max_instances,
            max_text_instances: max_instances,
            counters_draw_calls: AtomicU32::new(0),
            counters_shape_draw_calls: AtomicU32::new(0),
            counters_sprite_draw_calls: AtomicU32::new(0),
            counters_text_draw_calls: AtomicU32::new(0),
            counters_bytes_uploaded: AtomicU64::new(0),
            counters_instances_uploaded: AtomicU32::new(0),
            counters_buffer_reallocations: AtomicU32::new(0),
            counters_culled_instances: AtomicU32::new(0),
            counters_static_batches_drawn: AtomicU32::new(0),
            counters_static_instances_drawn: AtomicU32::new(0),
            scratch_sprites: Vec::new(),
            scratch_shapes: Vec::new(),
            scratch_reuse: true,
            pixel_ratio_bits: AtomicU32::new(1.0f32.to_bits()),
            legacy_2d: AtomicU32::new(0),
            dash_table: Mutex::new([[0.0; 8]; 16]),
            gradient_table: Mutex::new(Vec::new()),
            grid_pipeline,
            grid_uniform_buffer,
            grid_bind_group,
            grid_params: None,
            view_camera_buffers: Vec::new(),
            view_shape_bgs: Vec::new(),
            view_sprite_bgs: Vec::new(),
            view_text_bgs: Vec::new(),
            view_grid_buffers: Vec::new(),
            view_grid_bgs: Vec::new(),
        }
    }

    /// Sets the device-pixels-per-logical-pixel ratio used by the shader to
    /// resolve per-instance `min_px` (logical) into world units.
    ///
    /// Values below `1.0` are accepted but the shader clamps the ratio to
    /// `>= 1.0`, so a minimum width always covers at least one device pixel.
    pub fn set_pixel_ratio(&self, ratio: f32) {
        let ratio = if ratio.is_finite() && ratio > 0.0 {
            ratio
        } else {
            1.0
        };
        self.pixel_ratio_bits
            .store(ratio.to_bits(), Ordering::Relaxed);
    }

    /// Current device-pixels-per-logical-pixel ratio.
    pub fn pixel_ratio(&self) -> f32 {
        f32::from_bits(self.pixel_ratio_bits.load(Ordering::Relaxed))
    }

    /// A/B switch: restore the legacy thin-line stabiliser / 4 px AA band.
    pub fn set_legacy_2d(&self, legacy: bool) {
        self.legacy_2d
            .store(if legacy { 1 } else { 0 }, Ordering::Relaxed);
    }

    /// Whether the legacy line stabiliser is active.
    pub fn legacy_2d(&self) -> bool {
        self.legacy_2d.load(Ordering::Relaxed) != 0
    }

    /// Registers a shader dash pattern. `id` must be `1..=15` (`0` = solid).
    /// Lengths are alternating dash/gap in world units (metres in Koncreta);
    /// up to 8 values, zero-terminated. Registration is persistent.
    pub fn set_dash_pattern(&self, id: u8, lengths: &[f32]) {
        if id == 0 {
            return;
        }
        let mut table = self
            .dash_table
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let slot = &mut table[(id as usize).min(15)];
        *slot = [0.0; 8];
        for (dst, &src) in slot.iter_mut().zip(lengths.iter()) {
            if src.is_finite() && src > 0.0 {
                *dst = src;
            }
        }
    }

    /// Uploads the per-view gradient table used by shape and text instances.
    ///
    /// A gradient is encoded as:
    ///
    /// ```text
    /// [kind, stop_count, first_stop_slot, 0]      kind 0 = linear, 1 = radial
    /// [start.xy, end.xy]  or  [center.xy, radius, 0]
    /// [r, g, b, a] [offset, 0, 0, 0]  (one pair per stop, linear RGBA)
    /// ```
    ///
    /// `first_stop_slot` is the absolute vec4 index of the first stop pair.
    /// The table is copied into every view's camera uniform on the next
    /// `prepare_view`/`update_camera`; entries past
    /// [`crate::render::types::GRADIENT_TABLE_VEC4`] are dropped.
    pub fn set_gradient_table(&mut self, table: &[[f32; 4]]) {
        let mut stored = self
            .gradient_table
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        stored.clear();
        stored.extend_from_slice(
            &table[..table.len().min(crate::render::types::GRADIENT_TABLE_VEC4)],
        );
    }

    /// The gradient table padded to the uniform's fixed size.
    fn gradient_slots(&self) -> [[f32; 4]; crate::render::types::GRADIENT_TABLE_VEC4] {
        let mut slots = [[0.0f32; 4]; crate::render::types::GRADIENT_TABLE_VEC4];
        let table = self
            .gradient_table
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for (slot, value) in slots.iter_mut().zip(table.iter()) {
            *slot = *value;
        }
        slots
    }

    /// Enable/disable reuse of the internal sprite-staging scratch buffer.
    ///
    /// Defaults to `true`.  When disabled, [`Self::prepare`] allocates a fresh
    /// `Vec` every frame (the pre-optimization behaviour) so the scratch-reuse
    /// optimization can be A/B compared at runtime.
    pub fn set_scratch_reuse(&mut self, enabled: bool) {
        self.scratch_reuse = enabled;
    }

    /// Whether the internal scratch buffers are being reused across frames.
    pub fn scratch_reuse(&self) -> bool {
        self.scratch_reuse
    }

    fn camera_uniform(
        &self,
        proj_view: Mat4,
        resolution: glam::Vec2,
        viewport_origin: glam::Vec2,
    ) -> crate::render::types::Uniform2d {
        let mut dash = [[0.0f32; 4]; 32];
        {
            let table = self
                .dash_table
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            for p in 0..16 {
                dash[p * 2][0] = table[p][0];
                dash[p * 2][1] = table[p][1];
                dash[p * 2][2] = table[p][2];
                dash[p * 2][3] = table[p][3];
                dash[p * 2 + 1][0] = table[p][4];
                dash[p * 2 + 1][1] = table[p][5];
                dash[p * 2 + 1][2] = table[p][6];
                dash[p * 2 + 1][3] = table[p][7];
            }
        }
        crate::render::types::Uniform2d {
            view_proj: proj_view.to_cols_array(),
            resolution: resolution.to_array(),
            padding: [self.pixel_ratio(), if self.legacy_2d() { 1.0 } else { 0.0 }],
            viewport_origin: viewport_origin.to_array(),
            _pad: [0.0; 2],
            dash,
            gradients: self.gradient_slots(),
        }
    }

    fn ensure_view_slots(&mut self, index: usize) {
        while self.view_camera_buffers.len() <= index {
            let camera_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("View Camera Uniform Buffer"),
                size: std::mem::size_of::<crate::render::types::Uniform2d>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let shape_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.shape_pipeline.camera_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                }],
                label: Some("View Shape Camera Bind Group"),
            });
            let sprite_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.pipeline.camera_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                }],
                label: Some("View Sprite Camera Bind Group"),
            });
            let text_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.text_pipeline.camera_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                }],
                label: Some("View Text Camera Bind Group"),
            });
            let grid_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("View Grid Uniform Buffer"),
                size: std::mem::size_of::<crate::render::types::Grid2dUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let grid_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.grid_pipeline.camera_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: grid_buffer.as_entire_binding(),
                }],
                label: Some("View Grid Bind Group"),
            });
            self.view_camera_buffers.push(camera_buffer);
            self.view_shape_bgs.push(shape_bg);
            self.view_sprite_bgs.push(sprite_bg);
            self.view_text_bgs.push(text_bg);
            self.view_grid_buffers.push(grid_buffer);
            self.view_grid_bgs.push(grid_bg);
        }
    }

    /// Uploads one view's camera + grid uniform (multi-viewport Pure2D path).
    pub fn prepare_view(
        &mut self,
        index: usize,
        queue: &wgpu::Queue,
        view_proj: Mat4,
        rect_px: [f32; 4],
        inv_view_proj: Mat4,
        world_per_px: f32,
    ) {
        self.ensure_view_slots(index);
        let res = glam::Vec2::new(rect_px[2].max(1.0), rect_px[3].max(1.0));
        let origin = glam::Vec2::new(rect_px[0], rect_px[1]);
        let u = self.camera_uniform(view_proj, res, origin);
        queue.write_buffer(&self.view_camera_buffers[index], 0, bytemuck::bytes_of(&u));
        if let Some(p) = self.grid_params {
            let gu = crate::render::types::Grid2dUniform {
                inv_view_proj: inv_view_proj.to_cols_array(),
                resolution: res.to_array(),
                _pad0: [0.0; 2],
                viewport: rect_px,
                world_per_px: world_per_px.max(1e-9),
                dpp: self.pixel_ratio(),
                _pad1: [0.0; 2],
                params: [
                    p.base_step.max(1e-9),
                    p.major_every.max(1.0),
                    p.minor_width_px.max(0.0),
                    p.major_width_px.max(0.0),
                ],
                params2: [
                    p.axis_width_px.max(0.0),
                    if p.axes { 1.0 } else { 0.0 },
                    0.0,
                    0.0,
                ],
                minor_color: p.minor_color,
                major_color: p.major_color,
                axis_x_color: p.axis_x_color,
                axis_y_color: p.axis_y_color,
            };
            queue.write_buffer(&self.view_grid_buffers[index], 0, bytemuck::bytes_of(&gu));
        }
    }

    /// Camera bind group for view `index`.
    pub fn view_shape_bg(&self, index: usize) -> Option<&wgpu::BindGroup> {
        self.view_shape_bgs.get(index)
    }

    /// Sprite camera bind group for view `index`.
    pub fn view_sprite_bg(&self, index: usize) -> Option<&wgpu::BindGroup> {
        self.view_sprite_bgs.get(index)
    }

    /// Text camera bind group for view `index`.
    pub fn view_text_bg(&self, index: usize) -> Option<&wgpu::BindGroup> {
        self.view_text_bgs.get(index)
    }

    /// Grid bind group for view `index` (only meaningful when the grid is on).
    pub fn view_grid_bg(&self, index: usize) -> Option<&wgpu::BindGroup> {
        self.view_grid_bgs.get(index)
    }

    /// Draws the procedural grid using a view-specific bind group.
    pub fn render_grid_bg(&self, rpass: &mut wgpu::RenderPass<'_>, bg: &wgpu::BindGroup) {
        if self.grid_params.is_none() {
            return;
        }
        rpass.set_pipeline(&self.grid_pipeline.wgpu_pipeline);
        rpass.set_bind_group(0, bg, &[]);
        rpass.draw(0..3, 0..1);
    }

    /// Enables / replaces / disables (`None`) the procedural background grid.
    pub fn set_grid(&mut self, params: Option<Grid2dParams>) {
        self.grid_params = params;
    }

    /// Whether a grid pass will be drawn this frame.
    pub fn has_grid(&self) -> bool {
        self.grid_params.is_some()
    }

    /// Uploads the grid uniform for this frame. No-op when the grid is off.
    pub fn prepare_grid(
        &self,
        queue: &wgpu::Queue,
        inv_view_proj: Mat4,
        resolution: glam::Vec2,
        world_per_px: f32,
    ) {
        let Some(p) = self.grid_params else {
            return;
        };
        let u = crate::render::types::Grid2dUniform {
            inv_view_proj: inv_view_proj.to_cols_array(),
            resolution: resolution.to_array(),
            _pad0: [0.0; 2],
            viewport: [0.0, 0.0, resolution.x, resolution.y],
            world_per_px: world_per_px.max(1e-9),
            dpp: self.pixel_ratio(),
            _pad1: [0.0; 2],
            params: [
                p.base_step.max(1e-9),
                p.major_every.max(1.0),
                p.minor_width_px.max(0.0),
                p.major_width_px.max(0.0),
            ],
            params2: [
                p.axis_width_px.max(0.0),
                if p.axes { 1.0 } else { 0.0 },
                0.0,
                0.0,
            ],
            minor_color: p.minor_color,
            major_color: p.major_color,
            axis_x_color: p.axis_x_color,
            axis_y_color: p.axis_y_color,
        };
        queue.write_buffer(&self.grid_uniform_buffer, 0, bytemuck::bytes_of(&u));
    }

    /// Draws the procedural grid (full-screen triangle, one draw call).
    pub fn render_grid(&self, rpass: &mut wgpu::RenderPass<'_>) {
        if self.grid_params.is_none() {
            return;
        }
        rpass.set_pipeline(&self.grid_pipeline.wgpu_pipeline);
        rpass.set_bind_group(0, &self.grid_bind_group, &[]);
        rpass.draw(0..3, 0..1);
    }

    /// Resets the per-frame instrumentation.  Call once at the start of every
    /// frame (before `prepare_*` / `render_*`) when collecting metrics.
    pub fn reset_frame_counters(&self) {
        self.counters_draw_calls.store(0, Ordering::Relaxed);
        self.counters_shape_draw_calls.store(0, Ordering::Relaxed);
        self.counters_sprite_draw_calls.store(0, Ordering::Relaxed);
        self.counters_text_draw_calls.store(0, Ordering::Relaxed);
        self.counters_bytes_uploaded.store(0, Ordering::Relaxed);
        self.counters_instances_uploaded.store(0, Ordering::Relaxed);
        self.counters_buffer_reallocations
            .store(0, Ordering::Relaxed);
        self.counters_culled_instances.store(0, Ordering::Relaxed);
        self.counters_static_batches_drawn
            .store(0, Ordering::Relaxed);
        self.counters_static_instances_drawn
            .store(0, Ordering::Relaxed);
    }

    /// Adds `n` instances to the per-frame culled counter.  Called by the
    /// engine's Pure2D culling pass (see `renderer_core`); exposed here so the
    /// counter is reset with the rest of the frame instrumentation and reported
    /// by [`Self::frame_counters`].
    #[inline]
    pub fn add_culled_instances(&self, n: u32) {
        if n != 0 {
            self.counters_culled_instances
                .fetch_add(n, Ordering::Relaxed);
        }
    }

    /// Snapshot of the per-frame instrumentation accumulated since the last
    /// [`Renderer2d::reset_frame_counters`].
    pub fn frame_counters(&self) -> Frame2dCounters {
        Frame2dCounters {
            draw_calls: self.counters_draw_calls.load(Ordering::Relaxed),
            shape_draw_calls: self.counters_shape_draw_calls.load(Ordering::Relaxed),
            sprite_draw_calls: self.counters_sprite_draw_calls.load(Ordering::Relaxed),
            text_draw_calls: self.counters_text_draw_calls.load(Ordering::Relaxed),
            bytes_uploaded: self.counters_bytes_uploaded.load(Ordering::Relaxed),
            instances_uploaded: self.counters_instances_uploaded.load(Ordering::Relaxed),
            buffer_reallocations: self.counters_buffer_reallocations.load(Ordering::Relaxed),
            culled_instances: self.counters_culled_instances.load(Ordering::Relaxed),
            static_batches_drawn: self.counters_static_batches_drawn.load(Ordering::Relaxed),
            static_instances_drawn: self.counters_static_instances_drawn.load(Ordering::Relaxed),
        }
    }

    #[inline]
    fn record_shape_draw(&self) {
        self.counters_draw_calls.fetch_add(1, Ordering::Relaxed);
        self.counters_shape_draw_calls
            .fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn record_sprite_draw(&self) {
        self.counters_draw_calls.fetch_add(1, Ordering::Relaxed);
        self.counters_sprite_draw_calls
            .fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn record_text_draw(&self) {
        self.counters_draw_calls.fetch_add(1, Ordering::Relaxed);
        self.counters_text_draw_calls
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Update the Orthographic Camera Matrix and Resolution
    pub fn update_camera(&self, queue: &wgpu::Queue, proj_view: Mat4, resolution: glam::Vec2) {
        let mut dash = [[0.0f32; 4]; 32];
        {
            let table = self
                .dash_table
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            for p in 0..16 {
                dash[p * 2][0] = table[p][0];
                dash[p * 2][1] = table[p][1];
                dash[p * 2][2] = table[p][2];
                dash[p * 2][3] = table[p][3];
                dash[p * 2 + 1][0] = table[p][4];
                dash[p * 2 + 1][1] = table[p][5];
                dash[p * 2 + 1][2] = table[p][6];
                dash[p * 2 + 1][3] = table[p][7];
            }
        }
        let uniform = crate::render::types::Uniform2d {
            view_proj: proj_view.to_cols_array(),
            resolution: resolution.to_array(),
            // x = device px per logical px (min_px resolution),
            // y = legacy stabiliser flag (>0.5 = on).
            padding: [self.pixel_ratio(), if self.legacy_2d() { 1.0 } else { 0.0 }],
            // The single main camera fills the whole target.
            viewport_origin: [0.0, 0.0],
            _pad: [0.0; 2],
            dash,
            gradients: self.gradient_slots(),
        };
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// Write all batched instances to the GPU Buffer â€” UN solo write_buffer (mÃ¡ximo rendimiento).
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

        // Consolidar TODOS los datos en un slice contiguo â†’ UN SOLO write_buffer.
        // Evita N llamadas separadas a la GPU staging queue (una por textura).
        // Con `scratch_reuse` (default) el buffer de staging se reutiliza entre
        // frames: `clear()` conserva la capacidad y evita la alloc/grow que
        // medÃ­a la auditorÃ­a (H8).  Con `scratch_reuse = false` se reproduce el
        // comportamiento antiguo (un `Vec::with_capacity` por frame) para poder
        // comparar A/B en runtime.
        let bytes_written = if self.scratch_reuse {
            let scratch = &mut self.scratch_sprites;
            scratch.clear();
            for entries in batcher.batches.values() {
                scratch.extend(entries.iter().map(|entry| entry.instance));
            }
            let uploaded = bytemuck::cast_slice(scratch.as_slice());
            queue.write_buffer(&self.instance_buffer, 0, uploaded);
            uploaded.len() as u64
        } else {
            let mut flat: Vec<SpriteInstance> = Vec::with_capacity(total_instances);
            for entries in batcher.batches.values() {
                flat.extend(entries.iter().map(|entry| entry.instance));
            }
            let uploaded = bytemuck::cast_slice(&flat);
            queue.write_buffer(&self.instance_buffer, 0, uploaded);
            uploaded.len() as u64
        };
        self.counters_bytes_uploaded
            .fetch_add(bytes_written, Ordering::Relaxed);
        self.counters_instances_uploaded
            .fetch_add(total_instances as u32, Ordering::Relaxed);

        total_instances
    }

    /// Upload all shape instances as one contiguous `write_buffer`, growing the
    /// GPU buffer first if needed. Returns the number of instances uploaded.
    pub fn prepare_shapes(
        &mut self,
        queue: &wgpu::Queue,
        batcher: &crate::render::ShapeBatcher,
    ) -> usize {
        self.prepare_shape_slice(queue, &batcher.instances)
    }

    /// Upload an arbitrary, caller-ordered slice of shape instances.
    ///
    /// The Pure2D path uses this to upload a **painter-ordered** coalesced
    /// buffer without mutating the caller's `ShapeBatcher`.  Same single
    /// `write_buffer` and counters as [`Self::prepare_shapes`].
    pub fn prepare_shape_slice(
        &mut self,
        queue: &wgpu::Queue,
        data: &[crate::render::types::ShapeInstance],
    ) -> usize {
        let mut packed = std::mem::take(&mut self.scratch_shapes);
        packed.clear();
        packed.extend(data.iter().map(ShapeGpu::pack));
        let n = self.prepare_shape_gpu_slice(queue, &packed);
        self.scratch_shapes = packed;
        n
    }

    /// Upload an already-packed, caller-ordered slice of [`ShapeGpu`]s as one
    /// `write_buffer`, growing the GPU buffer first if needed.
    ///
    /// The engine's Pure2D path packs while it builds the painter-ordered
    /// upload, so it calls this directly and avoids a second copy.
    pub fn prepare_shape_gpu_slice(&mut self, queue: &wgpu::Queue, data: &[ShapeGpu]) -> usize {
        let total = data.len();
        if total == 0 {
            return 0;
        }

        if total > self.max_shape_instances as usize {
            self.max_shape_instances = (total as u32).next_power_of_two();
            self.shape_instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Shape Instance Buffer (Resized)"),
                size: (self.max_shape_instances as usize * std::mem::size_of::<ShapeGpu>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.counters_buffer_reallocations
                .fetch_add(1, Ordering::Relaxed);
        }

        let uploaded: &[u8] = bytemuck::cast_slice(data);
        queue.write_buffer(&self.shape_instance_buffer, 0, uploaded);
        self.counters_bytes_uploaded
            .fetch_add(uploaded.len() as u64, Ordering::Relaxed);
        self.counters_instances_uploaded
            .fetch_add(total as u32, Ordering::Relaxed);
        total
    }

    /// Upload an arbitrary, caller-ordered slice of sprite instances.
    ///
    /// Companion to [`Self::prepare_shape_slice`] for the painter-ordered
    /// coalesced Pure2D upload.
    pub fn prepare_sprite_slice(&mut self, queue: &wgpu::Queue, data: &[SpriteInstance]) -> usize {
        let total = data.len();
        if total == 0 {
            return 0;
        }

        if total > self.max_instances as usize {
            self.max_instances = (total as u32).next_power_of_two();
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Sprite Instance Buffer (Resized)"),
                size: (self.max_instances as usize * std::mem::size_of::<SpriteInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.counters_buffer_reallocations
                .fetch_add(1, Ordering::Relaxed);
        }

        let uploaded = bytemuck::cast_slice(data);
        queue.write_buffer(&self.instance_buffer, 0, uploaded);
        self.counters_bytes_uploaded
            .fetch_add(uploaded.len() as u64, Ordering::Relaxed);
        self.counters_instances_uploaded
            .fetch_add(total as u32, Ordering::Relaxed);
        total
    }

    /// Upload an arbitrary, caller-ordered slice of text instances as one
    /// `write_buffer`, growing the GPU buffer first if needed.
    ///
    /// The Pure2D painter-order pass packs glyphs into painter order and calls
    /// this once per frame; the buffer grows geometrically and is reused, so
    /// steady-state frames upload only their own bytes.
    pub fn prepare_text_slice(&mut self, queue: &wgpu::Queue, data: &[TextInstance]) -> usize {
        let total = data.len();
        if total == 0 {
            return 0;
        }

        if total > self.max_text_instances as usize {
            self.max_text_instances = (total as u32).next_power_of_two();
            self.text_instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Text Instance Buffer (Resized)"),
                size: (self.max_text_instances as usize * std::mem::size_of::<TextInstance>())
                    as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.counters_buffer_reallocations
                .fetch_add(1, Ordering::Relaxed);
        }

        let uploaded: &[u8] = bytemuck::cast_slice(data);
        queue.write_buffer(&self.text_instance_buffer, 0, uploaded);
        self.counters_bytes_uploaded
            .fetch_add(uploaded.len() as u64, Ordering::Relaxed);
        self.counters_instances_uploaded
            .fetch_add(total as u32, Ordering::Relaxed);
        total
    }

    /// Draw `[offset, offset + count)` text instances with `font_bg` bound at
    /// group 1 (one glyph atlas per draw call, coalesced by the caller).
    ///
    /// Uses [`BlendMode2d::Normal`]; the coalesced Pure2D stream calls
    /// [`Self::render_text_range_blend`] with the command's mode.
    pub fn render_text_range(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        font_bg: &wgpu::BindGroup,
        offset: u32,
        count: u32,
    ) {
        self.render_text_range_blend(render_pass, font_bg, offset, count, BlendMode2d::Normal);
    }

    /// [`Self::render_text_range`] with an explicit per-draw blend mode.
    pub fn render_text_range_blend(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        font_bg: &wgpu::BindGroup,
        offset: u32,
        count: u32,
        mode: BlendMode2d,
    ) {
        if count == 0 {
            return;
        }
        render_pass.set_pipeline(self.text_pipeline.pipeline(mode));
        render_pass.set_bind_group(0, &self.text_camera_bind_group, &[]);
        render_pass.set_bind_group(1, font_bg, &[]);
        render_pass.set_vertex_buffer(0, self.text_instance_buffer.slice(..));
        render_pass.draw(0..4, offset..(offset + count));
        self.record_text_draw();
    }

    /// Text range with an explicit camera bind group (multi-viewport path).
    pub fn render_text_range_bg(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        camera_bg: &wgpu::BindGroup,
        font_bg: &wgpu::BindGroup,
        offset: u32,
        count: u32,
    ) {
        self.render_text_range_bg_blend(
            render_pass,
            camera_bg,
            font_bg,
            offset,
            count,
            BlendMode2d::Normal,
        );
    }

    /// [`Self::render_text_range_bg`] with an explicit per-draw blend mode.
    #[allow(clippy::too_many_arguments)]
    pub fn render_text_range_bg_blend(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        camera_bg: &wgpu::BindGroup,
        font_bg: &wgpu::BindGroup,
        offset: u32,
        count: u32,
        mode: BlendMode2d,
    ) {
        if count == 0 {
            return;
        }
        render_pass.set_pipeline(self.text_pipeline.pipeline(mode));
        render_pass.set_bind_group(0, camera_bg, &[]);
        render_pass.set_bind_group(1, font_bg, &[]);
        render_pass.set_vertex_buffer(0, self.text_instance_buffer.slice(..));
        render_pass.draw(0..4, offset..(offset + count));
        self.record_text_draw();
    }

    /// Renders the prepared sprite instances.
    ///
    /// Entries of one texture bucket are split into maximal runs of equal
    /// [`BlendMode2d`]; the pipeline is re-bound when the mode changes.  The
    /// Pure2D immediate path uses the coalesced stream instead
    /// ([`Self::render_sprites_range_blend`]); this helper serves the legacy
    /// ECS batcher path.
    pub fn render<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        batcher: &'a SpriteBatcher,
        mut get_texture_bind_group: impl FnMut(u32) -> Option<&'a wgpu::BindGroup>,
    ) {
        render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));

        let mut current_offset = 0u32;
        for (&tex_id, entries) in &batcher.batches {
            let bucket_len = entries.len() as u32;
            let Some(bg) = get_texture_bind_group(tex_id) else {
                current_offset += bucket_len;
                continue;
            };
            render_pass.set_bind_group(1, bg, &[]);
            let mut run_start = 0usize;
            while run_start < entries.len() {
                let mode = entries[run_start].blend;
                let mut run_end = run_start + 1;
                while run_end < entries.len() && entries[run_end].blend == mode {
                    run_end += 1;
                }
                render_pass.set_pipeline(self.pipeline.pipeline(mode));
                let start = current_offset + run_start as u32;
                let count = (run_end - run_start) as u32;
                render_pass.draw(0..4, start..(start + count));
                self.record_sprite_draw();
                run_start = run_end;
            }
            current_offset += bucket_len;
        }
    }

    /// Draw every shape in `batcher` as a single instanced draw call.
    ///
    /// Uses [`BlendMode2d::Normal`]; see [`Self::render_shapes_blend`].
    pub fn render_shapes<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        batcher: &'a crate::render::ShapeBatcher,
    ) {
        self.render_shapes_blend(render_pass, batcher, BlendMode2d::Normal);
    }

    /// [`Self::render_shapes`] with an explicit per-draw blend mode.
    pub fn render_shapes_blend<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        batcher: &'a crate::render::ShapeBatcher,
        mode: BlendMode2d,
    ) {
        if batcher.instances.is_empty() {
            return;
        }
        render_pass.set_pipeline(self.shape_pipeline.pipeline(mode));
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
        self.render_shapes_range_blend(render_pass, offset, count, BlendMode2d::Normal);
    }

    /// [`Self::render_shapes_range`] with an explicit per-draw blend mode.
    pub fn render_shapes_range_blend(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        offset: u32,
        count: u32,
        mode: BlendMode2d,
    ) {
        if count == 0 {
            return;
        }
        render_pass.set_pipeline(self.shape_pipeline.pipeline(mode));
        render_pass.set_bind_group(0, &self.shape_camera_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.shape_instance_buffer.slice(..));
        render_pass.draw(0..4, offset..(offset + count));
        self.record_shape_draw();
    }

    /// Same as [`Self::render_shapes_range`] with an explicit camera bind group
    /// (multi-viewport path).
    pub fn render_shapes_range_bg(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        bg: &wgpu::BindGroup,
        offset: u32,
        count: u32,
    ) {
        self.render_shapes_range_bg_blend(render_pass, bg, offset, count, BlendMode2d::Normal);
    }

    /// [`Self::render_shapes_range_bg`] with an explicit per-draw blend mode.
    pub fn render_shapes_range_bg_blend(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        bg: &wgpu::BindGroup,
        offset: u32,
        count: u32,
        mode: BlendMode2d,
    ) {
        if count == 0 {
            return;
        }
        render_pass.set_pipeline(self.shape_pipeline.pipeline(mode));
        render_pass.set_bind_group(0, bg, &[]);
        render_pass.set_vertex_buffer(0, self.shape_instance_buffer.slice(..));
        render_pass.draw(0..4, offset..(offset + count));
        self.record_shape_draw();
    }

    /// Draws a **persistent** shape instance buffer as a single instanced draw
    /// call, using the shape (SDF) pipeline and the current camera bind group.
    ///
    /// Used by the engine's retained static 2D batches: the buffer is owned by
    /// the batch and is uploaded only on create/update, so calling this every
    /// frame does **no** upload.  `count` must not exceed the buffer capacity.
    /// Records one shape draw plus the static-batch telemetry counters.
    ///
    /// Retained batches always blend with [`BlendMode2d::Normal`].
    pub fn render_static_shapes(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        buffer: &wgpu::Buffer,
        count: u32,
    ) {
        if count == 0 {
            return;
        }
        render_pass.set_pipeline(self.shape_pipeline.pipeline(BlendMode2d::Normal));
        render_pass.set_bind_group(0, &self.shape_camera_bind_group, &[]);
        render_pass.set_vertex_buffer(0, buffer.slice(..));
        render_pass.draw(0..4, 0..count);
        self.record_shape_draw();
        self.counters_static_batches_drawn
            .fetch_add(1, Ordering::Relaxed);
        self.counters_static_instances_drawn
            .fetch_add(count, Ordering::Relaxed);
    }

    /// Draws `[start, start + count)` of a **persistent** [`ShapeGpu`] buffer.
    ///
    /// Used by the engine's chunk-culled static batches: one call per visible
    /// run of chunks.  Records one shape draw and adds `count` to the
    /// static-instance counter; the caller records the batch itself with
    /// [`Self::record_static_batch`].
    pub fn render_static_shapes_range(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        buffer: &wgpu::Buffer,
        start: u32,
        count: u32,
    ) {
        if count == 0 {
            return;
        }
        render_pass.set_pipeline(self.shape_pipeline.pipeline(BlendMode2d::Normal));
        render_pass.set_bind_group(0, &self.shape_camera_bind_group, &[]);
        render_pass.set_vertex_buffer(0, buffer.slice(..));
        render_pass.draw(0..4, start..(start + count));
        self.record_shape_draw();
        self.counters_static_instances_drawn
            .fetch_add(count, Ordering::Relaxed);
    }

    /// Static-batch range draw with an explicit camera bind group.
    pub fn render_static_shapes_range_bg(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        bg: &wgpu::BindGroup,
        buffer: &wgpu::Buffer,
        start: u32,
        count: u32,
    ) {
        if count == 0 {
            return;
        }
        render_pass.set_pipeline(self.shape_pipeline.pipeline(BlendMode2d::Normal));
        render_pass.set_bind_group(0, bg, &[]);
        render_pass.set_vertex_buffer(0, buffer.slice(..));
        render_pass.draw(0..4, start..(start + count));
        self.record_shape_draw();
        self.counters_static_instances_drawn
            .fetch_add(count, Ordering::Relaxed);
    }

    /// Counts one retained static batch as drawn this frame (see
    /// [`Self::render_static_shapes_range`]).
    pub fn record_static_batch(&self) {
        self.counters_static_batches_drawn
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Renders a contiguous range `[offset, offset + count)` of sprite instances
    /// for a single `texture_id`, binding its texture bind group first.  Same
    /// usage as `render` but scoped to an arbitrary slice of the flat upload.
    ///
    /// Uses [`BlendMode2d::Normal`]; see
    /// [`Self::render_sprites_range_blend`].
    pub fn render_sprites_range<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        tex_id: u32,
        offset: u32,
        count: u32,
        get_texture_bind_group: impl Fn(u32) -> Option<&'a wgpu::BindGroup>,
    ) {
        self.render_sprites_range_blend(
            render_pass,
            tex_id,
            offset,
            count,
            get_texture_bind_group,
            BlendMode2d::Normal,
        );
    }

    /// [`Self::render_sprites_range`] with an explicit per-draw blend mode.
    #[allow(clippy::too_many_arguments)]
    pub fn render_sprites_range_blend<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        tex_id: u32,
        offset: u32,
        count: u32,
        get_texture_bind_group: impl Fn(u32) -> Option<&'a wgpu::BindGroup>,
        mode: BlendMode2d,
    ) {
        if count == 0 {
            return;
        }
        let Some(bg) = get_texture_bind_group(tex_id) else {
            return;
        };
        render_pass.set_pipeline(self.pipeline.pipeline(mode));
        render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
        render_pass.set_bind_group(1, bg, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..4, offset..(offset + count));
        self.record_sprite_draw();
    }

    /// Sprite range with an explicit camera bind group (multi-viewport path).
    pub fn render_sprites_range_bg<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        camera_bg: &'a wgpu::BindGroup,
        tex_id: u32,
        offset: u32,
        count: u32,
        get_texture_bind_group: impl Fn(u32) -> Option<&'a wgpu::BindGroup>,
    ) {
        self.render_sprites_range_bg_blend(
            render_pass,
            camera_bg,
            tex_id,
            offset,
            count,
            get_texture_bind_group,
            BlendMode2d::Normal,
        );
    }

    /// [`Self::render_sprites_range_bg`] with an explicit per-draw blend mode.
    #[allow(clippy::too_many_arguments)]
    pub fn render_sprites_range_bg_blend<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        camera_bg: &'a wgpu::BindGroup,
        tex_id: u32,
        offset: u32,
        count: u32,
        get_texture_bind_group: impl Fn(u32) -> Option<&'a wgpu::BindGroup>,
        mode: BlendMode2d,
    ) {
        if count == 0 {
            return;
        }
        let Some(bg) = get_texture_bind_group(tex_id) else {
            return;
        };
        render_pass.set_pipeline(self.pipeline.pipeline(mode));
        render_pass.set_bind_group(0, camera_bg, &[]);
        render_pass.set_bind_group(1, bg, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..4, offset..(offset + count));
        self.record_sprite_draw();
    }
}

impl Renderer2d {
    /// Draws a shape range with an explicit pipeline (offscreen layer format).
    ///
    /// The camera bind group, instance buffer and counters are the renderer's
    /// own; only the compiled pipeline differs, which is what lets the layered
    /// path render into `Rgba16Float` targets.
    pub fn render_shapes_range_blend_on(
        &self,
        shape: &ShapePipeline,
        render_pass: &mut wgpu::RenderPass<'_>,
        offset: u32,
        count: u32,
        mode: BlendMode2d,
    ) {
        if count == 0 {
            return;
        }
        render_pass.set_pipeline(shape.pipeline(mode));
        render_pass.set_bind_group(0, &self.shape_camera_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.shape_instance_buffer.slice(..));
        render_pass.draw(0..4, offset..(offset + count));
        self.record_shape_draw();
    }

    /// Draws a sprite range with an explicit pipeline and texture bind group.
    ///
    /// The camera bind group is the renderer's own; an independently created
    /// (but structurally identical) pipeline has a compatible layout.
    pub fn render_sprites_range_blend_on<'a>(
        &'a self,
        sprite: &'a SpritePipeline,
        render_pass: &mut wgpu::RenderPass<'a>,
        texture_bg: &'a wgpu::BindGroup,
        offset: u32,
        count: u32,
        mode: BlendMode2d,
    ) {
        if count == 0 {
            return;
        }
        render_pass.set_pipeline(sprite.pipeline(mode));
        render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
        render_pass.set_bind_group(1, texture_bg, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.draw(0..4, offset..(offset + count));
        self.record_sprite_draw();
    }

    /// Draws a text range with an explicit pipeline and font bind group.
    pub fn render_text_range_blend_on<'a>(
        &'a self,
        text: &'a Text2dPipeline,
        render_pass: &mut wgpu::RenderPass<'a>,
        font_bg: &'a wgpu::BindGroup,
        offset: u32,
        count: u32,
        mode: BlendMode2d,
    ) {
        if count == 0 {
            return;
        }
        render_pass.set_pipeline(text.pipeline(mode));
        render_pass.set_bind_group(0, &self.text_camera_bind_group, &[]);
        render_pass.set_bind_group(1, font_bg, &[]);
        render_pass.set_vertex_buffer(0, self.text_instance_buffer.slice(..));
        render_pass.draw(0..4, offset..(offset + count));
        self.record_text_draw();
    }
}
