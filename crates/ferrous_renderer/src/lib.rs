/// `ferrous_renderer` -- modular, extensible GPU rendering for Ferrous Engine.
///
/// # Module layout
///
/// | Module          | Responsibility                                       |
/// |-----------------|------------------------------------------------------|
/// | `context`       | Re-exports `EngineContext`; device/queue accessors   |
/// | `resources`     | Low-level buffer / texture allocation helpers        |
/// | `geometry`      | `Vertex`, `Mesh`, built-in primitives                |
/// | `camera`        | GPU camera uniform + orbit controller                |
/// | `pipeline`      | Bind-group layouts + compiled `WorldPipeline`        |
/// | `render_target` | Off-screen color + depth targets (MSAA-aware)        |
/// | `scene`         | `RenderObject`, `sync_world` helper                  |
/// | `graph`         | `RenderPass` trait + `FramePacket`                   |
/// | `passes`        | Built-in passes: `WorldPass`, `UiPass`               |
pub mod camera;
pub mod context;
pub mod geometry;
pub mod graph;
pub mod passes;
pub mod pipeline;
pub mod render_stats;
pub mod render_target;
pub mod resources;
pub mod scene;

// -- Public re-exports --------------------------------------------------------

pub use ferrous_gui::{GuiBatch, GuiQuad};
pub use glam;

pub use camera::{Camera, Controller, GpuCamera};
pub use ferrous_core::input::{KeyCode, MouseButton};
pub use geometry::{Mesh, Vertex};
pub use graph::frame_packet::Viewport;
pub use graph::{FramePacket, InstancedDrawCommand, RenderPass};
pub use pipeline::InstancingPipeline;
pub use render_stats::RenderStats;
pub use render_target::RenderTarget;
pub use resources::InstanceBuffer;
pub use scene::{Aabb, Frustum, RenderObject};
use rayon::prelude::*;

// -- Internal imports ---------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;

use ferrous_gui::TextBatch;

use camera::controller::OrbitState;
use graph::frame_packet::{CameraPacket, DrawCommand};
use passes::{UiPass, WorldPass};
use pipeline::{PipelineLayouts, WorldPipeline};
use resources::ModelBuffer;

// -- RenderDest ---------------------------------------------------------------

enum RenderDest<'a> {
    Target,
    View(&'a wgpu::TextureView),
}

// -- Renderer -----------------------------------------------------------------

/// Top-level renderer.
///
/// Holds GPU resources and executes a list of [`RenderPass`] stages each frame
/// using the two-phase **prepare -> execute** pattern.
///
/// ## Built-in passes
/// `WorldPass` (3-D/2-D geometry) and `UiPass` (GUI overlay) are always present
/// as typed fields, giving direct access without any downcast.
///
/// ## Custom passes
/// Call [`Renderer::add_pass`] to append extra passes. They execute after
/// the built-in ones and receive `on_resize` / `on_attach` automatically.
///
/// ## 2-D / 3-D support
/// Both modes work simultaneously. Use an orthographic camera for 2-D,
/// perspective for 3-D. The pipeline is the same either way.
pub struct Renderer {
    pub context: context::EngineContext,
    pub render_target: RenderTarget,

    // -- Built-in passes (direct typed access, zero-cost) --------------------
    pub world_pass: WorldPass,
    pub ui_pass: UiPass,
    /// Additional user-supplied passes executed after the built-ins.
    pub extra_passes: Vec<Box<dyn RenderPass>>,

    // -- Camera ---------------------------------------------------------------
    pub camera: Camera,
    pub orbit: OrbitState,
    gpu_camera: GpuCamera,

    // -- Scene (O(1) lookup by id) --------------------------------------------
    /// Legacy manual objects (started at u64::MAX and descending)
    legacy_objects: HashMap<u64, RenderObject>,
    /// World objects mirrored from `ferrous_core::scene::World` (indices match World IDs)
    world_objects: Vec<Option<RenderObject>>,
    next_manual_id: u64,
    /// Shared dynamic uniform buffer for all model matrices (legacy/manual objects).
    model_buf: ModelBuffer,
    /// Next free slot in `model_buf` for manually-spawned objects.
    next_slot: usize,
    /// Layout kept for `model_buf.ensure_capacity` reallocation.
    model_layout: Arc<wgpu::BindGroupLayout>,
    /// Storage buffer for instanced World entities.
    instance_buf: InstanceBuffer,
    /// Layout for the instance storage buffer bind group.
    instance_layout: Arc<wgpu::BindGroupLayout>,

    /// Shared cube mesh — lazily created on first World spawn so that every
    /// cube RenderObject carries the same Arc<Buffer> pointers, enabling
    /// instanced grouping by vertex-buffer pointer in build_base_packet.
    shared_cube_mesh: Option<geometry::Mesh>,

    // -- Per-frame caches (reused across frames, zero heap alloc/frame) -------
    /// Reusable `DrawCommand` list — cleared and filled each frame.
    draw_commands_cache: Vec<DrawCommand>,
    /// Reusable `InstancedDrawCommand` list — cleared and filled each frame.
    instanced_commands_cache: Vec<InstancedDrawCommand>,
    /// Scratch buffer for matrices written to the instance buffer each frame.
    instance_matrix_scratch: Vec<glam::Mat4>,
    
    // -- Caching optimization flags -------------------------------------------
    prev_view_proj: Option<glam::Mat4>,
    scene_dirty: bool,

    // -- Surface info (for registering passes post-construction) --------------
    format: wgpu::TextureFormat,
    sample_count: u32,

    // -- Viewport -------------------------------------------------------------
    pub viewport: Viewport,
    width: u32,
    height: u32,

    // -- Per-frame render statistics ------------------------------------------
    /// Statistics from the most recently completed frame (vertices, triangles,
    /// draw calls).  Updated by `build_base_packet` every frame.
    pub render_stats: RenderStats,
}

impl Renderer {
    /// Creates a `Renderer` with the default world + UI passes.
    ///
    /// `sample_count`: `1` = no MSAA, `4` = 4x MSAA (recommended).
    pub fn new(
        context: context::EngineContext,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let device = &context.device;

        let rt = RenderTarget::new(device, width, height, format, sample_count);

        let layouts = PipelineLayouts::new(device);
        let world_pipeline = WorldPipeline::new(device, format, rt.sample_count(), layouts.clone());
        let instancing_pipeline =
            InstancingPipeline::new(device, format, rt.sample_count(), layouts.clone());

        let camera = Camera {
            eye: glam::Vec3::new(0.0, 0.0, 5.0),
            target: glam::Vec3::ZERO,
            up: glam::Vec3::Y,
            fovy: 45.0f32.to_radians(),
            aspect: width as f32 / height as f32,
            znear: 0.1,
            zfar: 2000.0,
            controller: Controller::with_default_wasd(),
        };
        let gpu_camera = GpuCamera::new(device, &camera, &layouts.camera);

        let world_pass = WorldPass::new(
            world_pipeline,
            instancing_pipeline,
            gpu_camera.bind_group.clone(),
        );
        let ui_renderer = ferrous_gui::GuiRenderer::new(
            context.device.clone(),
            format,
            1024,
            width,
            height,
            rt.sample_count(),
        );
        let ui_pass = UiPass::new(ui_renderer);

        // Create the shared dynamic model buffer and register it with WorldPass.
        let model_buf = ModelBuffer::new(&context.device, &layouts.model, 64);
        let instance_buf = InstanceBuffer::new(&context.device, &layouts.instance, 64);
        let mut world_pass_init = world_pass;
        world_pass_init.set_model_buffer(model_buf.bind_group.clone(), model_buf.stride);
        world_pass_init.set_instance_buffer(instance_buf.bind_group.clone());

        Self {
            context,
            render_target: rt,
            world_pass: world_pass_init,
            ui_pass,
            extra_passes: Vec::new(),
            camera,
            orbit: OrbitState::default(),
            gpu_camera,
            legacy_objects: HashMap::new(),
            world_objects: Vec::new(),
            next_manual_id: u64::MAX,
            model_buf,
            next_slot: 0,
            model_layout: layouts.model,
            instance_buf,
            instance_layout: layouts.instance,
            shared_cube_mesh: None,
            draw_commands_cache: Vec::new(),
            instanced_commands_cache: Vec::new(),
            instance_matrix_scratch: Vec::new(),
            prev_view_proj: None,
            scene_dirty: true,
            format,
            sample_count,
            viewport: Viewport {
                x: 0,
                y: 0,
                width,
                height,
            },
            width,
            height,
            render_stats: RenderStats::default(),
        }
    }

    // -- Frame API ------------------------------------------------------------

    /// Allocates a fresh `CommandEncoder` for the current frame.
    pub fn begin_frame(&self) -> wgpu::CommandEncoder {
        self.context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Encoder"),
            })
    }

    /// Renders into the internal off-screen [`RenderTarget`].
    pub fn render_to_target(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        ui_batch: Option<GuiBatch>,
        text_batch: Option<TextBatch>,
    ) {
        self.do_render(encoder, RenderDest::Target, ui_batch, text_batch);
    }

    /// Renders directly into an external `TextureView` (e.g. swapchain frame).
    pub fn render_to_view(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        ui_batch: Option<GuiBatch>,
        text_batch: Option<TextBatch>,
    ) {
        self.do_render(encoder, RenderDest::View(view), ui_batch, text_batch);
    }

    // -- Scene management -----------------------------------------------------

    /// Spawns a mesh instance at `pos`; returns a stable u64 handle.
    pub fn add_object(&mut self, mesh: Mesh, pos: glam::Vec3) -> u64 {
        let id = self.next_manual_id;
        self.next_manual_id = self.next_manual_id.wrapping_sub(1);

        let slot = self.next_slot;
        self.next_slot += 1;
        let matrix = glam::Mat4::from_translation(pos);

        // Grow the buffer if needed, then update WorldPass bind group.
        let prev_bg = self.model_buf.bind_group.clone();
        self.model_buf
            .ensure_capacity(&self.context.device, &self.model_layout, slot + 1);
        if !Arc::ptr_eq(&prev_bg, &self.model_buf.bind_group) {
            self.world_pass
                .set_model_buffer(self.model_buf.bind_group.clone(), self.model_buf.stride);
        }
        self.model_buf.write(&self.context.queue, slot, &matrix);

        let obj = RenderObject::new(&self.context.device, id, mesh, matrix, slot);
        self.legacy_objects.insert(id, obj);
        self.scene_dirty = true;
        id
    }

    /// Moves an existing object (GPU write). No-op if id is unknown.
    pub fn set_object_position(&mut self, id: u64, pos: glam::Vec3) {
        if let Some(obj) = self.legacy_objects.get_mut(&id) {
            let matrix = glam::Mat4::from_translation(pos);
            obj.set_matrix(matrix);
            self.model_buf.write(&self.context.queue, obj.slot, &matrix);
            self.scene_dirty = true;
        }
    }

    /// Returns the world-space position of an object, or `None`.
    pub fn get_object_position(&self, id: u64) -> Option<glam::Vec3> {
        self.legacy_objects.get(&id).map(|o| {
            let w = o.matrix.w_axis;
            glam::Vec3::new(w.x, w.y, w.z)
        })
    }

    /// Removes a manually-spawned object. No-op if unknown.
    pub fn remove_object(&mut self, id: u64) {
        if self.legacy_objects.remove(&id).is_some() {
            self.scene_dirty = true;
        }
    }

    /// Synchronises a `ferrous_core::scene::World` with the renderer's object map.
    pub fn sync_world(&mut self, world: &ferrous_core::scene::World) {
        let mutated = scene::sync_world(
            world,
            &mut self.world_objects,
            &self.context.device,
            &mut self.shared_cube_mesh,
        );
        if mutated {
            self.scene_dirty = true;
        }
    }    // -- Pass management ------------------------------------------------------

    /// Appends a custom pass after the built-in ones.
    /// `on_attach` is called immediately with the current surface format.
    pub fn add_pass<P: RenderPass>(&mut self, mut pass: P) {
        pass.on_attach(
            &self.context.device,
            &self.context.queue,
            self.format,
            self.sample_count,
        );
        self.extra_passes.push(Box::new(pass));
    }

    /// Removes all user-supplied passes.  Built-in passes are NOT removed.
    pub fn clear_extra_passes(&mut self) {
        self.extra_passes.clear();
    }

    // -- Resize / viewport ----------------------------------------------------

    /// Recreates GPU textures when the window changes size.
    /// Notifies every pass via `on_resize` -- no downcast needed.
    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        if new_width == self.width && new_height == self.height {
            return;
        }
        self.render_target
            .resize(&self.context.device, new_width, new_height);

        if self.viewport.width == self.width && self.viewport.height == self.height {
            self.viewport.width = new_width;
            self.viewport.height = new_height;
            self.camera.set_aspect(new_width as f32 / new_height as f32);
        }

        self.width = new_width;
        self.height = new_height;

        // Built-in passes
        self.world_pass.on_resize(
            &self.context.device,
            &self.context.queue,
            new_width,
            new_height,
        );
        self.ui_pass.on_resize(
            &self.context.device,
            &self.context.queue,
            new_width,
            new_height,
        );
        // User passes
        for pass in &mut self.extra_passes {
            pass.on_resize(
                &self.context.device,
                &self.context.queue,
                new_width,
                new_height,
            );
        }
    }

    /// Explicitly sets the viewport rectangle and updates the camera aspect ratio.
    pub fn set_viewport(&mut self, vp: Viewport) {
        self.viewport = vp;
        self.camera.set_aspect(vp.width as f32 / vp.height as f32);
    }

    // -- Configuration helpers (direct typed access, zero-cost) ---------------

    /// Sets the sky / background color used to clear the 3-D viewport.
    #[inline]
    pub fn set_clear_color(&mut self, color: wgpu::Color) {
        self.world_pass.clear_color = color;
    }

    /// Uploads a font atlas texture to the UI pass. Call once after font loading.
    #[inline]
    pub fn set_font_atlas(&mut self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) {
        self.ui_pass.set_font_atlas(view, sampler);
    }

    // -- Input ----------------------------------------------------------------

    /// Applies keyboard/mouse input to the orbit camera. `dt` is seconds elapsed.
    pub fn handle_input(&mut self, input: &mut ferrous_core::input::InputState, dt: f32) {
        self.orbit.update(&mut self.camera, input, dt);
    }

    // -- Private helpers ------------------------------------------------------

    fn do_render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        dest: RenderDest<'_>,
        ui_batch: Option<GuiBatch>,
        text_batch: Option<TextBatch>,
    ) {
        self.gpu_camera.sync(&self.context.queue, &self.camera);

        let mut packet = self.build_base_packet();
        if let Some(b) = ui_batch {
            packet.insert(b);
        }
        if let Some(b) = text_batch {
            packet.insert(b);
        }

        let (color_view, resolve_target) = match dest {
            RenderDest::Target => self.render_target.color_views(),
            RenderDest::View(v) => {
                if self.render_target.sample_count() > 1 {
                    (
                        self.render_target.color.msaa_view.as_ref().unwrap(),
                        Some(v),
                    )
                } else {
                    (v, None)
                }
            }
        };
        let depth_view = self.render_target.depth_view();

        let dev = &self.context.device;
        let q = &self.context.queue;

        // Built-in passes first
        self.world_pass.prepare(dev, q, &packet);
        self.world_pass.execute(
            dev,
            q,
            encoder,
            color_view,
            resolve_target,
            Some(depth_view),
            &packet,
        );

        self.ui_pass.prepare(dev, q, &packet);
        self.ui_pass
            .execute(dev, q, encoder, color_view, resolve_target, None, &packet);

        // User-supplied passes
        for pass in &mut self.extra_passes {
            pass.prepare(dev, q, &packet);
            pass.execute(
                dev,
                q,
                encoder,
                color_view,
                resolve_target,
                Some(depth_view),
                &packet,
            );
        }

        // Reclaim the Vec<DrawCommand> allocation back into cache for next frame.
        self.reclaim_packet_cache(packet);
    }

    fn build_base_packet(&mut self) -> FramePacket {
        let camera_packet = CameraPacket {
            view_proj: glam::Mat4::from_cols_array_2d(&self.gpu_camera.uniform.view_proj),
            eye: self.camera.eye,
        };

        // If the scene hasn't mutated and the camera hasn't moved, we can reuse the cached draw commands.
        if !self.scene_dirty && self.prev_view_proj == Some(camera_packet.view_proj) {
            let mut packet = FramePacket::new(Some(self.viewport), camera_packet);
            std::mem::swap(&mut packet.scene_objects, &mut self.draw_commands_cache);
            std::mem::swap(&mut packet.instanced_objects, &mut self.instanced_commands_cache);
            return packet;
        }

        self.scene_dirty = false;
        self.prev_view_proj = Some(camera_packet.view_proj);

        self.draw_commands_cache.clear();
        self.instanced_commands_cache.clear();
        self.instance_matrix_scratch.clear();

        let frustum = scene::Frustum::from_view_proj(&camera_packet.view_proj);

        for obj in self.legacy_objects.values() {
            // Legacy manual object.
            if frustum.intersects_aabb(&obj.world_aabb()) {
                self.draw_commands_cache.push(DrawCommand {
                    vertex_buffer: obj.mesh.vertex_buffer.clone(),
                    index_buffer: obj.mesh.index_buffer.clone(),
                    index_count: obj.mesh.index_count,
                    vertex_count: obj.mesh.vertex_count,
                    index_format: obj.mesh.index_format,
                    model_slot: obj.slot,
                });
            }
        }

        // Multi-mesh DOD parallel frustum culling + grouping
        // We use fold & reduce to aggregate matrices by mesh vertex_buffer pointer.
        use std::collections::HashMap;

        let visible_mesh_groups = self.world_objects.par_iter().flatten()
            .filter(|obj| frustum.intersects_aabb(&obj.world_aabb()))
            .fold(
                || HashMap::new(),
                |mut map: HashMap<usize, (geometry::Mesh, Vec<glam::Mat4>)>, obj| {
                    let key = Arc::as_ptr(&obj.mesh.vertex_buffer) as usize;
                    map.entry(key)
                        .or_insert_with(|| (obj.mesh.clone(), Vec::with_capacity(128)))
                        .1.push(obj.matrix);
                    map
                }
            )
            .reduce(
                || HashMap::new(),
                |mut map1, map2| {
                    for (k, (mesh, mut mats)) in map2 {
                        map1.entry(k)
                            .or_insert_with(|| (mesh, Vec::with_capacity(mats.len())))
                            .1.append(&mut mats);
                    }
                    map1
                }
            );

        let mut total_visible = 0;
        for (_, (_, mats)) in &visible_mesh_groups {
            total_visible += mats.len();
        }

        // Upload visible matrices (single write_buffer call, zero heap alloc).
        if total_visible > 0 {
            let prev_bg = self.instance_buf.bind_group.clone();
            self.instance_buf
                .reserve(&self.context.device, &self.instance_layout, total_visible);
            if !Arc::ptr_eq(&prev_bg, &self.instance_buf.bind_group) {
                self.world_pass
                    .set_instance_buffer(self.instance_buf.bind_group.clone());
            }

            // Flatten clustered matrices sequentially into scratch buffer
            let mut offset = 0;
            self.instance_matrix_scratch.reserve(total_visible);
            
            for (_key, (mesh, mats)) in visible_mesh_groups {
                let count = mats.len() as u32;
                self.instance_matrix_scratch.extend_from_slice(&mats);
                
                self.instanced_commands_cache.push(InstancedDrawCommand {
                    vertex_buffer: mesh.vertex_buffer.clone(),
                    index_buffer: mesh.index_buffer.clone(),
                    index_count: mesh.index_count,
                    vertex_count: mesh.vertex_count,
                    index_format: mesh.index_format,
                    first_instance: offset,
                    instance_count: count,
                });
                
                offset += count;
            }

            self.instance_buf
                .write_slice(&self.context.queue, 0, &self.instance_matrix_scratch);
        }

        // -- Compute render statistics ----------------------------------------
        let mut stats = RenderStats::default();
        for cmd in &self.draw_commands_cache {
            stats.vertex_count   += cmd.vertex_count as u64;
            stats.triangle_count += (cmd.index_count / 3) as u64;
            stats.draw_calls     += 1;
        }
        for cmd in &self.instanced_commands_cache {
            let inst = cmd.instance_count as u64;
            stats.vertex_count   += cmd.vertex_count as u64 * inst;
            stats.triangle_count += (cmd.index_count / 3) as u64 * inst;
            stats.draw_calls     += 1;
        }
        self.render_stats = stats;

        let mut packet = FramePacket::new(Some(self.viewport), camera_packet);
        std::mem::swap(&mut packet.scene_objects, &mut self.draw_commands_cache);
        std::mem::swap(
            &mut packet.instanced_objects,
            &mut self.instanced_commands_cache,
        );
        packet
    }

    /// Called at end of `do_render` to reclaim the Vec allocations back into
    /// the caches so they are reused next frame.
    #[inline]
    fn reclaim_packet_cache(&mut self, mut packet: FramePacket) {
        // Swap the (now-empty after execute) Vecs back into the caches.
        std::mem::swap(&mut self.draw_commands_cache, &mut packet.scene_objects);
        std::mem::swap(
            &mut self.instanced_commands_cache,
            &mut packet.instanced_objects,
        );
        // `packet` is dropped here; Arc clones inside are cheap.
    }
}
