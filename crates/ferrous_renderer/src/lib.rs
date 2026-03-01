pub mod camera;
/// `ferrous_renderer` — modular, extensible GPU rendering for Ferrous Engine.
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
pub mod context;
pub mod geometry;
pub mod graph;
pub mod passes;
pub mod pipeline;
pub mod render_target;
pub mod resources;
pub mod scene;

// ── Public re-exports ─────────────────────────────────────────────────────────

pub use ferrous_gui::{GuiBatch, GuiQuad};
pub use glam;

pub use camera::{Camera, Controller, GpuCamera};
pub use geometry::{Mesh, Vertex};
pub use graph::frame_packet::Viewport;
pub use graph::{FramePacket, RenderPass};
pub use render_target::RenderTarget;
pub use scene::RenderObject;
// Key / mouse button types — users need these to configure Controller bindings.
pub use ferrous_core::input::{KeyCode, MouseButton};

// ── Internal imports ──────────────────────────────────────────────────────────

use std::sync::Arc;

use ferrous_gui::TextBatch;

use camera::controller::OrbitState;
use graph::frame_packet::{CameraPacket, DrawCommand};
use passes::{UiPass, WorldPass};
use pipeline::{PipelineLayouts, WorldPipeline};

// ── RenderDest ────────────────────────────────────────────────────────────────

/// Where a render call should write its output.
enum RenderDest<'a> {
    /// Internal off-screen [`RenderTarget`].
    Target,
    /// An external `TextureView` supplied by the caller (e.g. swapchain surface).
    View(&'a wgpu::TextureView),
}

// ── Renderer ──────────────────────────────────────────────────────────────────

/// Top-level renderer.
///
/// Holds GPU resources and executes a list of [`RenderPass`] stages each frame
/// using the two-phase **prepare → execute** pattern.
///
/// Custom passes can be appended with [`Renderer::add_pass`].  The built-in
/// [`WorldPass`] (3-D geometry) and [`UiPass`] (GUI overlay) are registered
/// automatically during construction.
pub struct Renderer {
    pub context: context::EngineContext,
    pub render_target: RenderTarget,
    /// Ordered list of passes executed every frame.
    pub passes: Vec<Box<dyn RenderPass>>,

    // ── Camera ────────────────────────────────────────────────────────────
    pub camera: Camera,
    pub orbit: OrbitState,
    gpu_camera: GpuCamera,

    // ── Scene ─────────────────────────────────────────────────────────────
    objects: Vec<RenderObject>,
    model_layout: Arc<wgpu::BindGroupLayout>,

    // ── Viewport ──────────────────────────────────────────────────────────
    pub viewport: Viewport,
    width: u32,
    height: u32,
}

impl Renderer {
    /// Creates a `Renderer` with the default world + UI passes.
    pub fn new(
        context: context::EngineContext,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let device = &context.device;

        // GPU render target (MSAA x4)
        let rt = RenderTarget::new(device, width, height, format, 4);

        // Shared bind-group layouts + world pipeline
        let layouts = PipelineLayouts::new(device);
        let world_pipeline = WorldPipeline::new(device, format, rt.sample_count(), layouts.clone());

        // Camera
        let camera = Camera {
            eye: glam::Vec3::new(0.0, 0.0, 5.0),
            target: glam::Vec3::ZERO,
            up: glam::Vec3::Y,
            fovy: 45.0f32.to_radians(),
            aspect: width as f32 / height as f32,
            znear: 0.1,
            zfar: 100.0,
            controller: Controller::with_default_wasd(),
        };
        let gpu_camera = GpuCamera::new(device, &camera, &layouts.camera);

        // Built-in passes
        let world_pass = WorldPass::new(world_pipeline, gpu_camera.bind_group.clone());
        let ui_renderer = ferrous_gui::GuiRenderer::new(
            context.device.clone(),
            format,
            1024,
            width,
            height,
            rt.sample_count(),
        );
        let ui_pass = UiPass::new(ui_renderer);

        let mut passes: Vec<Box<dyn RenderPass>> = Vec::new();
        passes.push(Box::new(world_pass));
        passes.push(Box::new(ui_pass));

        Self {
            context,
            render_target: rt,
            passes,
            camera,
            orbit: OrbitState::default(),
            gpu_camera,
            objects: Vec::new(),
            model_layout: layouts.model,
            viewport: Viewport {
                x: 0,
                y: 0,
                width,
                height,
            },
            width,
            height,
        }
    }

    // ── Frame API ─────────────────────────────────────────────────────────────

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
        ui_batch: Option<&GuiBatch>,
        text_batch: Option<&TextBatch>,
    ) {
        self.do_render(encoder, RenderDest::Target, ui_batch, text_batch);
    }

    /// Renders directly into an external `TextureView` (e.g. a swapchain frame).
    pub fn render_to_view(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        ui_batch: Option<&GuiBatch>,
        text_batch: Option<&TextBatch>,
    ) {
        self.do_render(encoder, RenderDest::View(view), ui_batch, text_batch);
    }

    // ── Scene management ──────────────────────────────────────────────────────

    /// Spawns a mesh instance at `pos`; returns its stable handle index.
    pub fn add_object(&mut self, mesh: Mesh, pos: glam::Vec3) -> usize {
        let obj = RenderObject::new(&self.context.device, mesh, pos, &self.model_layout);
        self.objects.push(obj);
        self.objects.len() - 1
    }

    /// Moves an existing object to `pos` (GPU write).
    pub fn set_object_position(&mut self, idx: usize, pos: glam::Vec3) {
        if let Some(obj) = self.objects.get_mut(idx) {
            obj.set_position(&self.context.queue, pos);
        }
    }

    /// Returns the world-space position of an object, or `None` if OOB.
    pub fn get_object_position(&self, idx: usize) -> Option<glam::Vec3> {
        self.objects.get(idx).map(|o| o.position)
    }

    /// Synchronises a `ferrous_core::scene::World` with the renderer's object list.
    pub fn sync_world(&mut self, world: &mut ferrous_core::scene::World) {
        scene::sync_world(
            world,
            &mut self.objects,
            &self.context.device,
            &self.context.queue,
            &self.model_layout,
        );
    }

    // ── Pass management ───────────────────────────────────────────────────────

    /// Appends a custom pass.  Passes execute in insertion order.
    pub fn add_pass(&mut self, pass: Box<dyn RenderPass>) {
        self.passes.push(pass);
    }

    /// Removes all registered passes (including the built-in ones).
    pub fn clear_passes(&mut self) {
        self.passes.clear();
    }

    // ── Resize / viewport ─────────────────────────────────────────────────────

    /// Recreates GPU textures when the window changes size.
    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        if new_width == self.width && new_height == self.height {
            return;
        }
        self.render_target
            .resize(&self.context.device, new_width, new_height);

        // Stretch the viewport if it covered the full window before.
        if self.viewport.width == self.width && self.viewport.height == self.height {
            self.viewport.width = new_width;
            self.viewport.height = new_height;
            self.camera.set_aspect(new_width as f32 / new_height as f32);
        }

        self.width = new_width;
        self.height = new_height;

        for pass in &mut self.passes {
            if let Some(ui) = pass.as_any_mut().downcast_mut::<UiPass>() {
                ui.resize(&self.context.queue, new_width, new_height);
            }
        }
    }

    /// Explicitly sets the 3-D viewport rectangle and updates camera aspect.
    pub fn set_viewport(&mut self, vp: Viewport) {
        self.viewport = vp;
        self.camera.set_aspect(vp.width as f32 / vp.height as f32);
    }

    // ── Font atlas ────────────────────────────────────────────────────────────

    /// Uploads font atlas data to the `UiPass`.  Call once after font loading.
    pub fn set_font_atlas(&mut self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) {
        for pass in &mut self.passes {
            if let Some(ui) = pass.as_any_mut().downcast_mut::<UiPass>() {
                ui.set_font_atlas(view, sampler);
            }
        }
    }

    // ── Input ─────────────────────────────────────────────────────────────────

    /// Applies keyboard/mouse input to the camera.  `dt` is seconds elapsed.
    pub fn handle_input(&mut self, input: &mut ferrous_core::input::InputState, dt: f32) {
        self.orbit.update(&mut self.camera, input, dt);
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn do_render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        dest: RenderDest<'_>,
        ui_batch: Option<&GuiBatch>,
        text_batch: Option<&TextBatch>,
    ) {
        // 1. Upload camera to GPU
        self.gpu_camera.sync(&self.context.queue, &self.camera);

        // 2. Assemble the frame packet (pure CPU data)
        let packet = self.build_packet(ui_batch, text_batch);

        // 3. Resolve color / depth views
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

        // 4. Execute every registered pass
        for pass in &mut self.passes {
            pass.prepare(&self.context.device, &self.context.queue, &packet);
            pass.execute(
                &self.context.device,
                &self.context.queue,
                encoder,
                color_view,
                resolve_target,
                Some(depth_view),
                &packet,
            );
        }
    }

    fn build_packet(
        &self,
        ui_batch: Option<&GuiBatch>,
        text_batch: Option<&TextBatch>,
    ) -> FramePacket {
        let scene_objects = self
            .objects
            .iter()
            .map(|obj| DrawCommand {
                vertex_buffer: obj.mesh.vertex_buffer.clone(),
                index_buffer: obj.mesh.index_buffer.clone(),
                index_count: obj.mesh.index_count,
                index_format: obj.mesh.index_format,
                model_bind_group: obj.model_bind_group.clone(),
            })
            .collect();

        FramePacket {
            viewport: Some(self.viewport),
            camera: CameraPacket {
                view_proj: glam::Mat4::from_cols_array_2d(&self.gpu_camera.uniform.view_proj),
                eye: self.camera.eye,
            },
            scene_objects,
            ui_batch: ui_batch.cloned(),
            text_batch: text_batch.cloned(),
        }
    }
}
