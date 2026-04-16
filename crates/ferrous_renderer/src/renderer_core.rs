/// `ferrous_renderer::renderer_core` -- Core Renderer struct and lifecycle methods.
///
/// This module contains the fundamental Renderer struct definition and its core
/// lifecycle and state management methods. The Renderer is the top-level GPU
/// rendering context that holds all GPU resources and executes render passes.

use std::collections::HashMap;
use std::sync::Arc;

// Re-export types needed for the Renderer struct
// Module declarations
// Module re-exports from the crate root (since files are located in src/)
pub use crate::camera;
pub use crate::pipeline;
pub use crate::camera_system;
pub use crate::frame_builder;
pub use crate::gizmo_system;
pub use crate::render_stats;

// Re-export types needed for the Renderer struct
pub use camera::{Camera, Controller, GpuCamera};
pub use camera_system::CameraSystem;
pub use frame_builder::FrameBuilder;
pub use gizmo_system::GizmoSystem;
pub use crate::graph::frame_packet::Viewport;
pub use crate::graph::RenderPass;
pub use pipeline::PipelineLayouts;
pub use crate::geometry::{Mesh, Vertex, compute_tangents};
pub use render_stats::RenderStats;
pub use crate::render_target::RenderTarget;
pub use crate::resources::InstanceBuffer;
pub use ferrous_core::Color;
pub use ferrous_core::scene::{RenderStyle, MaterialDescriptor};
pub use crate::passes::{
    CelShadedPass, FlatShadedPass, OutlinePass, ParticleSystem, PostProcessPass, PrePass,
    ProceduralSkyPass, SkinningPass, SkyMode, SsaoBlurPass, SsaoPass, WorldPass,
};

#[cfg(feature = "gui")]
pub use crate::passes::UiPass;

#[cfg(feature = "gpu-driven")]
pub use crate::passes::CullPass;

// Internal imports needed for method implementations
use crate::materials::MaterialRegistry;
use pipeline::PbrPipeline;
use crate::resources::SsaoResources;
use camera::controller::OrbitState;
pub use pipeline::InstancingPipeline;

use ferrous_2d::render::{Renderer2d, ShapeBatcher};


use ferrous_core::context::EngineContext;
use crate::graph::frame_packet::CameraPacket;
use crate::scene::culling::Frustum;
use crate::scene::scene_data::SceneData;

/// Controls which passes are executed each frame.
///
/// | Mode | World / Post-process | UI |
/// |------|----------------------|----|
enum RenderDest<'a> {
    Target,
    View(&'a wgpu::TextureView),
}

/// | `Full3D` | ✓ | ✓ |
/// | `Flat2D` | ✗ (skipped) | ✓ (clears to `world_pass.clear_color`) |
///
/// Set via [`Renderer::set_mode`].  The default is [`RendererMode::Full3D`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RendererMode {
    /// Full 3-D pipeline: prepass → SSAO → world → style → gizmos → post-process → UI.
    #[default]
    Full3D,
    /// 2-D GUI-only pipeline: the UI pass clears the surface directly.
    /// WorldPass, render-style passes, gizmos, and post-process are all skipped.
    Flat2D,
}

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
    pub context: EngineContext,
    pub render_target: RenderTarget,

    // -- Built-in passes (direct typed access, zero-cost) --------------------
    pub world_pass: WorldPass,
    #[cfg(feature = "gui")]
    pub ui_pass: UiPass,
    /// Post-process pass: tone mapping + gamma correction.
    pub post_process_pass: PostProcessPass,
    /// Additional user-supplied passes executed after the built-ins.
    pub extra_passes: Vec<Box<dyn RenderPass>>,

    // -- Camera (Fase 3: delegated to CameraSystem) ---------------------------
    /// All camera state: CPU camera, orbit controller, GPU uniform.
    pub camera_system: CameraSystem,

    // -- Scene (O(1) lookup by id) --------------------------------------------
    /// CPU-side material descriptor cache for detecting changes during sync_world.
    /// Keyed by entity id (u64).
    world_material_descs: HashMap<u64, ferrous_core::scene::MaterialDescriptor>,
    /// Storage buffer for instanced World entities.
    instance_buf: InstanceBuffer,
    /// Layout for the instance storage buffer bind group.
    instance_layout: Arc<wgpu::BindGroupLayout>,
    pub particle_system: Option<ParticleSystem>,
    /// A copy of the pipeline bind-group layouts; needed when creating
    /// new materials or other GPU resources that rely on them.
    // Note: shared mesh caches (cube/quad/sphere) and mesh_cache are now
    // owned by `frame_builder` (Phase 8).

    // -- Gizmo system (Phase 3: extracted to GizmoSystem) -------------------
    /// Owns the line-list GPU pipeline and per-frame draw queue.
    pub gizmo_system: GizmoSystem,
    pub debug_lines: Vec<DebugLine>,

    // -- Per-frame state (Fase 3: extracted to FrameBuilder) ------------------
    frame_builder: FrameBuilder,
    /// Separate instance buffer for shadow casters.  Not camera-culled.
    shadow_instance_buf: InstanceBuffer,

    /// Material manager handling textures and bind groups.
    material_registry: MaterialRegistry,

    // -- Surface info (for registering passes post-construction) --------------
    format: wgpu::TextureFormat,
    sample_count: u32,

    // -- Viewport -------------------------------------------------------------
    pub viewport: Viewport,
    width: u32,
    height: u32,

    // -- Renderer mode --------------------------------------------------------
    /// Controls which passes execute each frame.  Defaults to `Full3D`.
    /// Use `set_mode(RendererMode::Flat2D)` for GUI-only applications.
    pub mode: RendererMode,

    // -- Per-frame render statistics ------------------------------------------
    /// Statistics from the most recently completed frame (vertices, triangles,
    /// draw calls).  Updated by `build_base_packet` every frame.
    pub render_stats: RenderStats,

    // -- SSAO -----------------------------------------------------------------
    /// Depth-normal prepass (runs before WorldPass).
    pub prepass: PrePass,
    /// SSAO generation pass (half-resolution).
    pub ssao_pass: SsaoPass,
    /// SSAO bilateral blur pass.
    pub ssao_blur_pass: SsaoBlurPass,
    /// CPU-side SSAO resources (kernel, noise, params buffers).
    pub ssao_resources: SsaoResources,
    /// GPU skinning pass.
    pub skinning_pass: SkinningPass,
    /// When true, SSAO is computed and applied to the IBL ambient term.
    pub ssao_enabled: bool,

    // -- Render style (Phase 7) -----------------------------------------------
    /// Active render style.  Defaults to `RenderStyle::Pbr`.
    pub render_style: RenderStyle,
    /// Cel-shaded pass (active when `render_style == CelShaded`).
    cel_pass: Option<CelShadedPass>,
    /// Inverted-hull outline pass (active when `CelShaded { outline_width > 0 }`).
    outline_pass: Option<OutlinePass>,
    /// Flat-shaded pass (active when `render_style == FlatShaded`).
    flat_pass: Option<FlatShadedPass>,
    /// Copy of pipeline layouts so we can construct style passes at runtime
    /// without borrowing `Renderer::new` locals.
    pipeline_layouts: PipelineLayouts,
    /// Cached directional light for per-frame packet injection.
    current_dir_light: crate::resources::DirectionalLightUniform,

    // -- Phase 11: GPU-Driven Rendering --------------------------------------
    /// When `true`, `sync_world` uploads cull data and `do_render` dispatches
    /// the compute cull pass before `WorldPass`. Defaults to `false`.
    #[cfg(feature = "gpu-driven")]
    pub gpu_culling_enabled: bool,
    /// The GPU frustum-cull compute pass. Created lazily the first time
    /// `enable_gpu_culling(true)` is called.
    #[cfg(feature = "gpu-driven")]
    cull_pass: Option<CullPass>,

    // -- Antialiasing (Phase AA) ----------------------------------------------
    /// Configurable antialiasing post-process (FXAA / SMAA / None).
    pub aa_pass: crate::passes::AntialiasingPass,

    // -- Technical 2D Rendering -----------------------------------------------
    pub renderer_2d: Renderer2d,
    pub shape_batcher: ShapeBatcher,
}


impl Renderer {
    /// Creates a `Renderer` with the default world + UI passes.
    ///
    /// `sample_count`: `1` = no MSAA, `4` = 4x MSAA (recommended).
    pub fn new(
        context: EngineContext,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        sample_count: u32,
        hdri_path: Option<&std::path::Path>,
    ) -> Self {
        let device = &context.device;

        log::info!("[WGPU] Creating Renderer with {}x MSAA", sample_count);
        let rt = RenderTarget::new(device, width, height, format, sample_count);

        let layouts = PipelineLayouts::new(device);
        // pbr pipelines write to the HDR texture (Rgba16Float) so values > 1.0
        // are preserved; tone mapping happens in the post-process pass.
        let hdr_format = crate::render_target::HdrTexture::FORMAT;
        let pbr_pipeline = PbrPipeline::new(
            device,
            hdr_format,
            rt.sample_count(),
            layouts.clone(),
            Some(wgpu::Face::Back),
            None, // no blending for opaque
            true, // depth write enabled
        );
        let pbr_pipeline_double = PbrPipeline::new(
            device,
            hdr_format,
            rt.sample_count(),
            layouts.clone(),
            None,
            None,
            true,
        );
        let pbr_pipeline_blend = PbrPipeline::new(
            device,
            hdr_format,
            rt.sample_count(),
            layouts.clone(),
            Some(wgpu::Face::Back),
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
        );
        let pbr_pipeline_blend_double = PbrPipeline::new(
            device,
            hdr_format,
            rt.sample_count(),
            layouts.clone(),
            None,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
        );
        let instancing_pipeline = InstancingPipeline::new(
            device,
            hdr_format,
            rt.sample_count(),
            layouts.clone(),
            Some(wgpu::Face::Back),
            None,
            true,
        );
        let instancing_pipeline_double = InstancingPipeline::new(
            device,
            hdr_format,
            rt.sample_count(),
            layouts.clone(),
            None,
            None,
            true,
        );
        let instancing_pipeline_blend = InstancingPipeline::new(
            device,
            hdr_format,
            rt.sample_count(),
            layouts.clone(),
            Some(wgpu::Face::Back),
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
        );
        let instancing_pipeline_blend_double = InstancingPipeline::new(
            device,
            hdr_format,
            rt.sample_count(),
            layouts.clone(),
            None,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
        );

        let camera = Camera {
            eye: glam::Vec3::new(0.0, 0.0, 5.0),
            target: glam::Vec3::ZERO,
            up: glam::Vec3::Y,
            projection_type: ferrous_core::scene::camera::ProjectionType::Perspective,
            fovy: 45.0f32.to_radians(),
            aspect: width as f32 / height as f32,
            ortho_size: 2.0,
            znear: 0.1,
            zfar: 2000.0,
            controller: {
                let mut c = Controller::new();
                c.speed = 0.0;
                c.mouse_sensitivity = 0.0;
                c
            },
        };
        let gpu_camera = GpuCamera::new(device, &camera, &layouts.camera);
        let camera_system = CameraSystem {
            camera,
            orbit: OrbitState::default(),
            gpu: gpu_camera,
        };
        // built-in passes will be created after the GUI renderer below
        // create the world pass, forwarding the optional HDRI path from the
        // caller.  the pass will internally build its own shadow pipeline and
        // texture (2048� depth map) and keep the cubemaps for image-based
        // lighting if an HDRI was provided.
        let world_pass = WorldPass::new(
            pbr_pipeline,
            pbr_pipeline_double,
            pbr_pipeline_blend,
            pbr_pipeline_blend_double,
            instancing_pipeline,
            instancing_pipeline_double,
            instancing_pipeline_blend,
            instancing_pipeline_blend_double,
            camera_system.gpu.bind_group.clone(),
            device,
            &context.queue,
            &layouts,
            width,
            height,
            rt.sample_count(),
            hdri_path,
        );
        // when the pass is created it will internally construct its own
        // shadow pipeline and texture (2048� depth map).  no additional
        // arguments are necessary since those objects only depend on the
        // device and the common pipeline layouts that we already pass in.
        // create material registry (includes default white material)
        let material_registry = MaterialRegistry::new(device, &context.queue, &layouts);
        let mut world_pass = world_pass;
        world_pass.set_material_table(&material_registry.bind_group_table(), &material_registry);
        #[cfg(feature = "gui")]
        let ui_renderer = ferrous_gui::GuiRenderer::new(
            device.clone(),
            hdr_format,
            1024, // initial max instances
            width,
            height,
            sample_count, // Use hardware MSAA
        );

        // now that we have a GUI renderer instance, create the corresponding
        // UI pass and the post-process (tone-mapping) pass
        #[cfg(feature = "gui")]
        let ui_pass = UiPass::new(ui_renderer);
        let mut post_process_pass = PostProcessPass::new();
        post_process_pass.set_camera_layout(layouts.camera.clone());
        // on_attach builds the bloom pipelines (and the tone-mapping pipeline
        // keyed to the swapchain format); must be called before on_resize.
        post_process_pass.on_attach(device, &context.queue, format, sample_count);

        let instance_buf = InstanceBuffer::new(&context.device, &layouts.instance, 64);
        // Separate instance buffer for shadow casters (all objects, not camera-culled).
        let shadow_instance_buf = InstanceBuffer::new(&context.device, &layouts.instance, 64);
        let mut world_pass_init = world_pass;
        world_pass_init.set_instance_buffer(instance_buf.bind_group.clone());
        world_pass_init.set_shadow_instance_buffer(shadow_instance_buf.bind_group.clone());

        // gizmo system: owns the GPU pipeline and the per-frame draw queue.
        let gizmo_system = GizmoSystem::new(device, hdr_format, rt.sample_count(), layouts.clone());

        // -- SSAO: build passes before the Self literal consumes the buffers --
        let mut prepass = PrePass::new(device, layouts.instance.clone(), width, height, sample_count);
        prepass.set_instance_buffer(instance_buf.bind_group.clone());
        prepass.set_material_table(&material_registry.bind_group_table(), &material_registry);
        let ssao_pass = SsaoPass::new(device, width, height);
        let ssao_blur_pass = SsaoBlurPass::new(device, width, height);
        let ssao_resources = SsaoResources::new(device, &context.queue);
        let skinning_pass = SkinningPass::new(device);
        let particle_system = ParticleSystem::new(device, &layouts.camera, 1_000_000, sample_count);

        // -- Antialiasing pass -----------------------------------------------
        let mut aa_pass = crate::passes::AntialiasingPass::new(device);
        aa_pass.on_attach(device, hdr_format);
        aa_pass.on_resize(device, width, height);

        let renderer_2d = Renderer2d::new(context.device.clone(), hdr_format, sample_count, 1024);
        let shape_batcher = ShapeBatcher::default();

        Self {
            context,
            render_target: rt,
            world_pass: world_pass_init,
            #[cfg(feature = "gui")]
            ui_pass,
            post_process_pass,
            extra_passes: Vec::new(),
            camera_system,
            world_material_descs: HashMap::new(),
            instance_buf,
            particle_system: Some(particle_system),
            instance_layout: layouts.instance.clone(),
            gizmo_system,
            debug_lines: Vec::new(),
            frame_builder: FrameBuilder::new(),
            shadow_instance_buf,
            material_registry,
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
            mode: RendererMode::Full3D,
            render_stats: RenderStats::default(),
            prepass,
            ssao_pass,
            ssao_blur_pass,
            ssao_resources,
            skinning_pass,
            ssao_enabled: true,
            render_style: RenderStyle::Pbr,
            cel_pass: None,
            outline_pass: None,
            flat_pass: None,
            pipeline_layouts: layouts,
            current_dir_light: crate::resources::DirectionalLightUniform::default(),
            #[cfg(feature = "gpu-driven")]
            gpu_culling_enabled: false,
            #[cfg(feature = "gpu-driven")]
            cull_pass: None,
            aa_pass,
            renderer_2d,
            shape_batcher,
        }
    }


    /// Resizes the renderer's internal render target and updates all dependent passes.
    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        if new_width == self.width && new_height == self.height {
            return;
        }
        self.render_target
            .resize(&self.context.device, new_width, new_height);

        if self.viewport.width == self.width && self.viewport.height == self.height {
            self.viewport.width = new_width;
            self.viewport.height = new_height;
            self.camera_system
                .set_aspect(new_width as f32 / new_height as f32);
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
        // SSAO passes
        self.prepass.on_resize(
            &self.context.device,
            &self.context.queue,
            new_width,
            new_height,
        );
        self.ssao_pass
            .on_resize(&self.context.device, new_width, new_height);
        self.ssao_blur_pass
            .on_resize(&self.context.device, new_width, new_height);
        // post-process pass owns bloom textures which also depend on size
        self.post_process_pass.on_resize(
            &self.context.device,
            &self.context.queue,
            new_width,
            new_height,
        );
        self.aa_pass.on_resize(&self.context.device, new_width, new_height);
        
        #[cfg(feature = "gui")]
        {
            self.ui_pass.on_resize(
                &self.context.device,
                &self.context.queue,
                new_width,
                new_height,
            );
        }
        // User passes
        for pass in &mut self.extra_passes {
            pass.on_resize(
                &self.context.device,
                &self.context.queue,
                new_width,
                new_height,
            );
        }
        // Antialiasing textures
        self.aa_pass.on_resize(&self.context.device, new_width, new_height);
    }

    /// Switch between the full 3-D pipeline and the lightweight 2-D/GUI-only
    /// pipeline.
    ///
    /// In `Flat2D` mode the world pass, render-style passes, gizmos, and
    /// post-process pass are all skipped.  The UI pass clears the surface to
    /// `world_pass.clear_color` instead of compositing on top of a rendered
    /// scene, so the background colour is preserved.
    pub fn set_mode(&mut self, mode: RendererMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        #[cfg(feature = "gui")]
        {
            let clear = if mode == RendererMode::Flat2D {
                Some(self.world_pass.clear_color)
            } else {
                None
            };
            self.ui_pass.set_clear_color(clear);
        }
    }

    /// Convenience helper to switch to Flat 2D mode with a specific background color.
    pub fn enable_flat_2d(&mut self, background_color: wgpu::Color) {
        self.set_mode(RendererMode::Flat2D);
        self.set_clear_color(background_color);
    }

    /// Convenience helper to switch back to full 3-D mode.
    pub fn enable_full_3d(&mut self) {
        self.set_mode(RendererMode::Full3D);
    }

    /// Sets the active render style (PBR, CelShaded, or FlatShaded).
    ///
    /// This method creates or destroys the appropriate render passes based on the
    /// selected style. When switching to CelShaded, it also creates an OutlinePass
    /// if the outline width is greater than zero.
    pub fn set_render_style(&mut self, style: RenderStyle) {
        let hdr_format = crate::render_target::HdrTexture::FORMAT;
        match &style {
            RenderStyle::Pbr => {
                self.cel_pass = None;
                self.outline_pass = None;
                self.flat_pass = None;
            }
            RenderStyle::CelShaded {
                toon_levels,
                outline_width,
            } => {
                let toon_levels = *toon_levels;
                let outline_width = *outline_width;

                let mut cp = CelShadedPass::new(
                    &self.context.device,
                    &self.pipeline_layouts,
                    self.camera_system.gpu.bind_group.clone(),
                    hdr_format,
                    self.sample_count,
                    toon_levels,
                    outline_width,
                );
                cp.set_instance_buffer(self.instance_buf.bind_group.clone());
                cp.set_material_table(&self.material_registry.bind_group_table());
                self.cel_pass = Some(cp);

                if outline_width > 0.0 {
                    let mut op = OutlinePass::new(
                        &self.context.device,
                        &self.pipeline_layouts,
                        self.camera_system.gpu.bind_group.clone(),
                        hdr_format,
                        self.sample_count,
                        toon_levels,
                        outline_width,
                        [0.0, 0.0, 0.0, 1.0],
                    );
                    op.set_instance_buffer(self.instance_buf.bind_group.clone());
                    op.set_material_table(&self.material_registry.bind_group_table());
                    self.outline_pass = Some(op);
                } else {
                    self.outline_pass = None;
                }
                self.flat_pass = None;
            }
            RenderStyle::FlatShaded => {
                let mut fp = FlatShadedPass::new(
                    &self.context.device,
                    &self.pipeline_layouts,
                    self.camera_system.gpu.bind_group.clone(),
                    hdr_format,
                    self.sample_count,
                );
                fp.set_instance_buffer(self.instance_buf.bind_group.clone());
                fp.set_material_table(&self.material_registry.bind_group_table());
                self.flat_pass = Some(fp);
                self.cel_pass = None;
                self.outline_pass = None;
            }
        }
        self.render_style = style;
    }

    /// Explicitly sets the viewport rectangle and updates the camera aspect ratio.
    pub fn set_viewport(&mut self, vp: Viewport) {
        self.viewport = vp;
        self.camera_system
            .set_aspect(vp.width as f32 / vp.height as f32);
    }

    /// Set the camera projection type (Perspective or Orthographic).
    pub fn set_projection_type(&mut self, proj: ferrous_core::scene::camera::ProjectionType) {
        self.camera_system.camera.projection_type = proj;
    }

    /// Set the vertical size for the orthographic projection.
    pub fn set_ortho_size(&mut self, size: f32) {
        self.camera_system.camera.ortho_size = size;
    }

    /// Set the background clear color and switch to Solid sky mode.
    pub fn set_background_color(&mut self, color: wgpu::Color) {
        self.set_clear_color(color);
        self.world_pass.sky_mode = SkyMode::Solid(color);
    }

    /// Set the global ambient light for the scene.
    pub fn set_ambient_light(&mut self, color: [f32; 3], intensity: f32) {
        crate::renderer_api::set_ambient_light(&mut self.camera_system, &self.context.queue, color, intensity);
    }

    /// Configure the antialiasing mode applied after the gizmo pass.
    ///
    /// # Examples
    /// ```rust,ignore
    /// // FXAA with default quality (recommended for most use cases)
    /// renderer.set_antialiasing(AntialiasingMode::Fxaa(FxaaParams::default()));
    ///
    /// // SMAA — sharper edges, three sub-passes
    /// renderer.set_antialiasing(AntialiasingMode::Smaa);
    ///
    /// // Disabled — fastest, no blur
    /// renderer.set_antialiasing(AntialiasingMode::None);
    /// ```
    pub fn set_antialiasing(&mut self, mode: crate::passes::AntialiasingMode) {
        self.aa_pass.set_mode(mode);
    }

    /// Handles input events, specifically camera control input.
    ///
    /// Delegates input handling to the camera system for orbit controls.
    pub fn handle_input(&mut self, input: &mut ferrous_core::input::InputState, dt: f32) {
        self.camera_system.handle_input(input, dt);
    }

    pub fn begin_frame(&self) -> wgpu::CommandEncoder {
        self.context.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Renderer Frame Encoder"),
        })
    }

    /// Mutable reference to the internal frame builder (mesh cache, packets).
    pub fn frame_builder_mut(&mut self) -> &mut FrameBuilder {
        &mut self.frame_builder
    }

    pub fn camera(&self) -> &crate::camera::Camera {
        &self.camera_system.camera
    }

    /// Converts a screen-space coordinate (mouse position) into a world-space ray.
    ///
    /// Returns `(origin, direction)`.  `screen_pos` should be in pixels relative
    /// to the top-left of the renderer's viewport.
    pub fn get_ray(&self, screen_pos: (f32, f32)) -> (glam::Vec3, glam::Vec3) {
        let (w, h) = (self.width as f32, self.height as f32);
        
        // Convert screen pixels to Normalized Device Coordinates (NDC) [-1, 1]
        // WebGPU Y is up in NDC, but screen Y is down.
        let ndc = glam::Vec2::new(
            (screen_pos.0 / w) * 2.0 - 1.0,
            1.0 - (screen_pos.1 / h) * 2.0,
        );
        
        let inv_vp = self.camera_system.view_proj().inverse();
        
        // Unproject two points (near and far plane) to find the ray direction
        let n = inv_vp.project_point3(glam::Vec3::new(ndc.x, ndc.y, 0.0));
        let f = inv_vp.project_point3(glam::Vec3::new(ndc.x, ndc.y, 1.0));
        
        let dir = (f - n).normalize();
        (n, dir)
    }

    pub fn queue_gizmo(&mut self, gizmo: crate::scene::GizmoDraw) {
        self.gizmo_system.queue(gizmo);
    }

    /// Creates a GPU mesh from a list of vertices and indices.
    ///
    /// This method automatically calculates the mesh's AABB (bounding box)
    /// and generates tangent vectors for correct normal mapping.
    pub fn create_mesh(
        &self,
        name: &str,
        mut vertices: Vec<Vertex>,
        indices: Vec<u32>,
    ) -> Mesh {
        log::debug!("[Mesh] Creating: {} (V:{}, I:{})", name, vertices.len(), indices.len());

        if indices.is_empty() {
             return Mesh::empty(&self.context.device);
        }

        log::debug!("[Mesh] Computing tangents for {}...", name);
        
        crate::geometry::compute_tangents(&mut vertices, &indices);
        
        log::debug!("[Mesh] Uploading {} to GPU...", name);
        
        // Use full u32 indices for professionalism and to support larger meshes.
        // Previously this was truncated to u16 which caused issues with meshes > 65k vertices.
        
        // Calculate the Axis-Aligned Bounding Box (AABB) from vertex positions
        let mut min = glam::Vec3::splat(f32::INFINITY);
        let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
        
        for v in &vertices {
            let p = glam::Vec3::new(v.position[0], v.position[1], v.position[2]);
            min = min.min(p);
            max = max.max(p);
        }
        
        // Add a small epsilon to Y for perfectly flat meshes like floors to avoid culling issues
        if (max.y - min.y).abs() < 0.01 {
            min.y -= 0.01;
            max.y += 0.01;
        }

        let aabb = crate::scene::culling::Aabb::new(min, max);

        Mesh {
            vertex_buffer: crate::resources::buffer::create_vertex(&self.context.device, &format!("{} VB", name), &vertices),
            index_buffer: crate::resources::buffer::create_index(&self.context.device, &format!("{} IB", name), &indices),
            index_count: indices.len() as u32,
            vertex_count: vertices.len() as u32,
            index_format: wgpu::IndexFormat::Uint32,
            aabb,
        }
    }

    /// Sets the background sky mode (Solid, Cubemap, or Procedural).
    pub fn set_sky_mode(&mut self, mode: SkyMode) {
        self.world_pass.sky_mode = mode;
    }

    /// Convenience helper to switch to the procedural atmospheric HDR sky.
    pub fn set_sky_procedural(&mut self) {
        let sky = ProceduralSkyPass::new(
            &self.context.device,
            &self.pipeline_layouts,
            self.camera_system.gpu.bind_group.clone(),
            self.world_pass.environment.bind_group.clone(),
            crate::render_target::HdrTexture::FORMAT,
            1, // HDR target is always single-sample
        );
        self.set_sky_mode(SkyMode::Procedural(sky));
    }

    pub fn sync_world(&mut self, world: &ferrous_core::scene::World) {
        // 0. Sync DirectionalLight ECS component → GPU uniform (if present)
        {
            use ferrous_core::scene::DirectionalLight;
            let lights: Vec<DirectionalLight> = world
                .ecs
                .query::<DirectionalLight>()
                .map(|(_, l)| *l)
                .collect();
            if let Some(light) = lights.first() {
                self.set_directional_light(
                    [light.direction.x, light.direction.y, light.direction.z],
                    [light.color.r, light.color.g, light.color.b],
                    light.intensity,
                );
            }
        }

        // 0d. Sync ParticleEmitter ECS components → ParticleSystem
        {
            use ferrous_core::scene::ParticleEmitter;
            use ferrous_ecs::prelude::Query;
            let emitters = Query::<(&ferrous_core::scene::GlobalTransform, &ParticleEmitter)>::new(&world.ecs);
            let emitters_data: Vec<_> = emitters.iter()
                .map(|(_, (t, e))| (*t, e.clone()))
                .collect();
            
            if let Some((transform, emitter)) = emitters_data.first() {
                if let Some(ps) = &mut self.particle_system {
                    let (_, _, translation) = transform.0.to_scale_rotation_translation();
                    ps.update(
                        &self.context.queue,
                        0.016, // hardcoded for now, should use TimeSystem
                        [translation.x, translation.y, translation.z],
                        if emitter.active { emitter.spawn_rate } else { 0.0 },
                    );
                }
            }
        }

        // 0b. Sync Camera3D ECS component → renderer camera (if present)
        {
            use ferrous_core::scene::Camera3D;
            let cameras: Vec<Camera3D> = world.ecs.query::<Camera3D>().map(|(_, c)| *c).collect();
            if let Some(cam3d) = cameras.first() {
                self.camera_system.camera.eye = cam3d.eye;
                self.camera_system.camera.target = cam3d.target;
                self.camera_system.camera.fovy = cam3d.fov_deg.to_radians();
                self.camera_system.camera.znear = cam3d.near;
                self.camera_system.camera.zfar = cam3d.far;
            }
        }

        // 0c. Sync Material ECS components → MaterialDescriptors
        {
            use ferrous_core::scene::world::MaterialComponent;
            let mat_updates: Vec<(u64, ferrous_core::scene::MaterialHandle, ferrous_core::scene::MaterialDescriptor)> = world
                .ecs
                .query::<MaterialComponent>()
                .map(|(e, m)| (e.index as u64, m.handle, m.descriptor.clone()))
                .collect();

            for (ecs_id, handle, desc) in mat_updates {
                let needs_update = self
                    .world_material_descs
                    .get(&ecs_id)
                    .map(|prev| *prev != desc)
                    .unwrap_or(true);
                
                if needs_update {
                    self.material_registry.update_params(
                        &self.context.queue,
                        handle,
                        &desc,
                    );
                    self.world_material_descs.insert(ecs_id, desc.clone());
                }
            }
        }

        // 1. Build frustum from current camera
        let camera_packet = crate::graph::frame_packet::CameraPacket {
            view_proj: self.camera_system.camera.build_view_projection_matrix(),
            eye: self.camera_system.camera.eye,
        };
        let frustum = Frustum::from_view_proj(&camera_packet.view_proj);

        // 2. ECS query -> populate frame_builder world instanced caches
        {
            let world_pass_ref = &mut self.world_pass;
            let prepass_ref = &mut self.prepass;
            self.frame_builder.build_world_commands(
                world,
                &self.context.device,
                &frustum,
                self.camera_system.camera.eye,
                &mut self.instance_buf,
                &self.instance_layout,
                &mut self.shadow_instance_buf,
                &mut |bg, shadow_bg| {
                    world_pass_ref.set_instance_buffer(bg.clone());
                    world_pass_ref.set_shadow_instance_buffer(shadow_bg);
                    prepass_ref.set_instance_buffer(bg);
                },
                &self.context.queue,
            );
        }
        self.frame_builder.scene_dirty = true;
    }

    #[cfg(feature = "gui")]
    pub fn render_to_view(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        ui_batch: Option<ferrous_gui::GuiBatch>,
    ) {
        self.camera_system.sync_gpu(&self.context.queue);

        let camera_packet = CameraPacket {
            view_proj: self.camera_system.camera.build_view_projection_matrix(),
            eye: self.camera_system.camera.eye,
        };
        let (mut packet, stats) = self.frame_builder.build(self.viewport, camera_packet);

        // Propagate the (possibly-reallocated) instance buffer to style passes.
        let bg = self.instance_buf.bind_group.clone();
        if let Some(p) = &mut self.cel_pass {
            p.set_instance_buffer(bg.clone());
        }
        if let Some(p) = &mut self.outline_pass {
            p.set_instance_buffer(bg.clone());
        }
        if let Some(p) = &mut self.flat_pass {
            p.set_instance_buffer(bg);
        }

        // Sync material table to all passes that need it (Phase 12 Professional Sync)
        let material_table = self.material_registry.bind_group_table();
        self.world_pass.set_material_table(&material_table, &self.material_registry);
        self.prepass.set_material_table(&material_table, &self.material_registry);
        if let Some(p) = &mut self.cel_pass {
            p.set_material_table(&material_table);
        }
        if let Some(p) = &mut self.outline_pass {
            p.set_material_table(&material_table);
        }
        if let Some(p) = &mut self.flat_pass {
            p.set_material_table(&material_table);
        }


        self.render_stats = stats;

        if let Some(b) = ui_batch {
            packet.insert(b);
        }

        // ── Flat2D fast path ───────────────────────────────────────────────
        if self.mode == RendererMode::Flat2D {
            self.ui_pass.prepare(&self.context.device, &self.context.queue, &packet);
            self.ui_pass.execute(
                &self.context.device,
                &self.context.queue,
                encoder,
                view,
                None,
                None,
                &packet,
            );
            for pass in &mut self.extra_passes {
                pass.prepare(&self.context.device, &self.context.queue, &packet);
                pass.execute(
                    &self.context.device,
                    &self.context.queue,
                    encoder,
                    view,
                    None,
                    None,
                    &packet,
                );
            }
            self.frame_builder.reclaim(packet);
            return;
        }

        let dummy_view = self
            .render_target
            .color
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // -- 1. Depth-Normal Prepass (required by SSAO) ------------------------
        {
            let v = self.camera_system.view_matrix();
            let p = self.camera_system.proj_matrix();
            self.prepass.update_camera(&self.context.queue, v, p, self.camera_system.camera.eye);
            self.prepass.prepare(&self.context.device, &self.context.queue, &packet);
            self.prepass.execute(
                &self.context.device,
                &self.context.queue,
                encoder,
                &dummy_view,
                None,
                Some(&self.render_target.depth.view), // share main depth buffer
                &packet,
            );
        }

        // -- 2. SSAO passes (only when enabled) --------------------------------
        if self.ssao_enabled {
            let p = self.camera_system.proj_matrix();
            let ssao_w = self.ssao_pass.ssao_texture.width;
            let ssao_h = self.ssao_pass.ssao_texture.height;
            self.ssao_resources.update_params(&self.context.queue, ssao_w, ssao_h, p, p.inverse());

            self.ssao_pass.run(&self.context.device, encoder, &self.ssao_resources, &self.prepass.normal_depth);
            self.ssao_blur_pass.run(&self.context.device, encoder, &self.ssao_pass.ssao_texture, &self.prepass.normal_depth);

            let ssao_view = Arc::new(self.ssao_blur_pass.blurred.texture.create_view(&wgpu::TextureViewDescriptor::default()));
            let ssao_sampler = Arc::new(self.context.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("SSAO Result Sampler"),
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                ..Default::default()
            }));
            self.world_pass.update_ssao(&self.context.device, ssao_view, ssao_sampler);
        }

        // -- 3. Phase 11: GPU cull compute dispatch (if enabled) --------------
        #[cfg(feature = "gpu-driven")]
        {
            if self.gpu_culling_enabled {
                if let Some(cp) = &self.cull_pass {
                    cp.dispatch(encoder);
                    cp.copy_counters_to_staging(encoder);
                }
            }
        }

        // -- 4. World Pass (Opaque + Blended) ----------------------------------
        self.world_pass.prepare(&self.context.device, &self.context.queue, &packet);
        self.world_pass.execute(
            &self.context.device,
            &self.context.queue,
            encoder,
            &dummy_view,
            None,
            Some(&self.render_target.depth.view),
            &packet,
        );

        let (scene_view, scene_rt) = if let Some(m_view) = &self.world_pass.hdr_texture.multisampled_view {
            (m_view, Some(&self.world_pass.hdr_texture.view))
        } else {
            (&self.world_pass.hdr_texture.view, None)
        };
        if let Some(ps) = &self.particle_system {
            ps.run_compute(encoder);
            
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Particle Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: scene_view,
                    resolve_target: scene_rt,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.render_target.depth.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            ps.run_render(&mut rpass, &self.camera_system.gpu.bind_group);
        }

        // -- 5. Render Style Passes ------------------------------------------

        match &self.render_style {
            RenderStyle::CelShaded { toon_levels, outline_width } => {
                let toon_levels = *toon_levels;
                let outline_width = *outline_width;
                packet.insert(crate::passes::CelFrameData { light: self.current_dir_light, toon_levels, outline_width });
                if outline_width > 0.0 {
                    packet.insert(crate::passes::OutlineFrameData { light: self.current_dir_light, toon_levels, outline_width, color: [0.0, 0.0, 0.0, 1.0] });
                }
                if let Some(p) = &mut self.cel_pass {
                    p.prepare(&self.context.device, &self.context.queue, &packet);
                    p.execute(&self.context.device, &self.context.queue, encoder, scene_view, scene_rt, Some(&self.render_target.depth.view), &packet);
                }
                if outline_width > 0.0 {
                    if let Some(p) = &mut self.outline_pass {
                        p.prepare(&self.context.device, &self.context.queue, &packet);
                        p.execute(&self.context.device, &self.context.queue, encoder, scene_view, scene_rt, Some(&self.render_target.depth.view), &packet);
                    }
                }
            }
            RenderStyle::FlatShaded => {
                packet.insert(crate::passes::FlatFrameData { light: self.current_dir_light });
                if let Some(p) = &mut self.flat_pass {
                    p.prepare(&self.context.device, &self.context.queue, &packet);
                    p.execute(&self.context.device, &self.context.queue, encoder, scene_view, scene_rt, Some(&self.render_target.depth.view), &packet);
                }
            }
            RenderStyle::Pbr => {}
        }

        // -- 6. Gizmo Pass -----------------------------------------------------
        for line in self.debug_lines.drain(..) {
            self.gizmo_system.draw_line(line);
        }

        self.gizmo_system.execute(&self.context.device, encoder, scene_view, scene_rt, &self.render_target.depth.view, &self.camera_system.gpu.bind_group);

        // -- 6a. Technical 2D Pass (Walls, etc.) -------------------------------
        self.renderer_2d.update_camera(&self.context.queue, self.camera_system.view_proj());
        self.renderer_2d.prepare_shapes(&self.context.queue, &self.shape_batcher);
        
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Technical 2D Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: scene_view,
                    resolve_target: scene_rt,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.render_target.depth.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.renderer_2d.render_shapes(&mut rpass, &self.shape_batcher);
        }

        // -- 6b. UI Pass (Hardware MSAA) ---------------------------------------
        // By rendering UI to the MSAA target before resolve, we get perfect edges.
        #[cfg(feature = "gui")]
        {
            self.ui_pass.prepare(&self.context.device, &self.context.queue, &packet);
            self.ui_pass.execute(
                &self.context.device,
                &self.context.queue,
                encoder,
                scene_view,
                scene_rt,
                None, // UI doesn't need depth resolve
                &packet,
            );
        }

        // -- 6c. Antialiasing Pass (between gizmos and tone-mapping) -----------

        self.aa_pass.update_params(&self.context.queue, self.width, self.height);
        self.aa_pass.run_aa(&self.context.device, encoder, &self.world_pass.hdr_texture);

        // -- 7. Post-Process (Tone Mapping) ------------------------------------
        // Route through AA output when a mode is active; fall back to raw HDR.
        use crate::passes::AntialiasingMode;
        match self.aa_pass.mode {
            AntialiasingMode::None => {
                self.post_process_pass.render(
                    &self.context.device, encoder,
                    &self.world_pass.hdr_texture,
                    view,
                    &self.camera_system.gpu.bind_group,
                );
            }
            _ => {
                // aa_pass.output() borrows self.aa_pass and self.world_pass.hdr_texture.
                // We need to pass the views by raw ptr to avoid conflicting borrows
                // with the encoder.  This is sound because the textures outlive
                // the render pass scope and are never moved during the blit.
                let aa_view    = self.aa_pass.output(&self.world_pass.hdr_texture) as *const _;
                let aa_sampler = self.aa_pass.output_sampler(&self.world_pass.hdr_texture) as *const _;
                // SAFETY: views & samplers are GPU-side descriptors stored in
                // self.aa_pass / self.world_pass which cannot be deallocated while
                // self is alive.
                let aa_view    = unsafe { &*aa_view };
                let aa_sampler = unsafe { &*aa_sampler };
                self.post_process_pass.render_with_view(
                    &self.context.device, encoder,
                    aa_view, aa_sampler,
                    &self.world_pass.hdr_texture,
                    view,
                    &self.camera_system.gpu.bind_group,
                );
            }
        }

        // -- Clear batcher for next frame --

        self.shape_batcher.clear();

        // -- 9. Extra Passes ---------------------------------------------------
        for pass in &mut self.extra_passes {
            pass.prepare(&self.context.device, &self.context.queue, &packet);
            pass.execute(&self.context.device, &self.context.queue, encoder, view, None, None, &packet);
        }

        self.frame_builder.reclaim(packet);
    }


    pub fn register_texture_linear(&mut self, w: u32, h: u32, pixels: &[u8]) -> crate::resources::TextureHandle {
        crate::renderer_api::register_texture(
            &mut self.material_registry,
            &self.context.device,
            &self.context.queue,
            &mut self.world_pass,
            w,
            h,
            pixels,
        )
    }

    pub fn register_texture(&mut self, w: u32, h: u32, pixels: &[u8]) -> crate::resources::TextureHandle {
        crate::renderer_api::register_texture(
            &mut self.material_registry,
            &self.context.device,
            &self.context.queue,
            &mut self.world_pass,
            w,
            h,
            pixels,
        )
    }

    pub fn create_material(&mut self, desc: &ferrous_core::scene::MaterialDescriptor) -> ferrous_core::scene::MaterialHandle {
        crate::renderer_api::create_material(
            &mut self.material_registry,
            &self.context.device,
            &self.context.queue,
            &mut self.world_pass,
            desc,
        )
    }

    pub fn update_material_params(&mut self, handle: ferrous_core::scene::MaterialHandle, desc: &ferrous_core::scene::MaterialDescriptor) {
        crate::renderer_api::update_material_params(
            &mut self.material_registry,
            &self.context.queue,
            handle,
            desc,
        );
    }

    pub fn register_mesh(&mut self, key: &str, mesh: crate::geometry::Mesh) {
        // Unconditional — routes to procedural_mesh_cache (always available).
        crate::renderer_api::register_mesh(&mut self.frame_builder, key, mesh);
    }

    pub fn set_clear_color(&mut self, color: wgpu::Color) {
        crate::renderer_api::set_clear_color(
            &mut self.world_pass,
            #[cfg(feature = "gui")] &mut self.ui_pass,
            self.mode,
            color,
        );
    }

    /// Configure global atmosphere settings (fog and exposure).
    pub fn set_exposure(&mut self, exposure: f32) {
        crate::renderer_api::set_exposure(&mut self.camera_system, &self.context.queue, exposure);
    }

    pub fn set_fog(&mut self, color: [f32; 3], density: f32) {
        crate::renderer_api::set_fog(&mut self.camera_system, &self.context.queue, color, density);
    }



    pub fn set_ssao_params(&mut self, radius: f32, bias: f32, intensity: f32, power: f32) {
        self.ssao_resources.radius = radius;
        self.ssao_resources.bias = bias;
        self.ssao_resources.intensity = intensity;
        self.ssao_resources.power = power;
        
        // Sync to GPU immediately so the next frame uses new params
        let cam = &self.camera_system.camera;
        let proj = glam::Mat4::perspective_rh(cam.fovy, cam.aspect, cam.znear, cam.zfar);
        self.ssao_resources.update_params(
            &self.context.queue,
            self.width,
            self.height,
            proj,
            proj.inverse(),
        );
    }

    pub fn set_font_atlas(&mut self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) {
        #[cfg(feature = "gui")]
        crate::renderer_api::set_font_atlas(&mut self.ui_pass, view, sampler);
    }

    pub fn set_directional_light(&mut self, direction: [f32; 3], color: [f32; 3], intensity: f32) {
        crate::renderer_api::set_directional_light(
            &mut self.world_pass,
            &self.context.queue,
            &mut self.current_dir_light,
            direction,
            color,
            intensity,
        );
    }

    pub fn add_pass<P: crate::graph::RenderPass + 'static>(&mut self, pass: P) {
        self.extra_passes.push(Box::new(pass));
    }

    pub fn camera_mut(&mut self) -> &mut Camera {
        &mut self.camera_system.camera
    }

    /// Push a technical 2D shape for rendering this frame.
    pub fn draw_2d_shape(&mut self, instance: ferrous_2d::render::types::ShapeInstance) {
        self.shape_batcher.push_shape(instance);
    }

    /// Push a fully-assembled SceneData to the renderer for this frame.

    pub fn set_scene(&mut self, scene: &SceneData) {
        if let Some(cam) = &scene.camera {
            self.camera_system.camera.eye = cam.eye;
            self.camera_system.camera.target = cam.target;
            self.camera_system.camera.fovy = cam.fov_y;
            self.camera_system.camera.znear = cam.z_near;
            self.camera_system.camera.zfar = cam.z_far;
        }

        if let Some(light) = &scene.directional_light {
            self.set_directional_light(
                light.direction.to_array(),
                light.color.to_array(),
                light.intensity,
            );
        }

        if !scene.instances.is_empty() {
            self.frame_builder.scene_dirty = true;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DebugLine {
    pub start: ferrous_core::glam::Vec3,
    pub end: ferrous_core::glam::Vec3,
    pub color: [f32; 3],
}

impl Renderer {
    pub fn draw_line(&mut self, start: ferrous_core::glam::Vec3, end: ferrous_core::glam::Vec3, color: Color) {
        let [r, g, b, _] = color.to_array();
        self.debug_lines.push(DebugLine { start, end, color: [r, g, b] });
    }

    pub fn viewport_size(&self) -> ferrous_core::glam::Vec2 {
        ferrous_core::glam::Vec2::new(
            self.render_target.width as f32,
            self.render_target.height as f32,
        )
    }
}
