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
/// | `camera_system` | `CameraSystem` — ECS-friendly camera update system   |
/// | `frame_builder` | `FrameBuilder` — assembles `FramePacket` each frame  |
/// | `gizmo_system`  | `GizmoSystem` — debug line/shape rendering           |
/// | `pipeline`      | Bind-group layouts + compiled `WorldPipeline`        |
/// | `render_target` | Off-screen color + depth targets (MSAA-aware)        |
/// | `scene`         | `Aabb`, `Frustum`, culling helpers                   |
/// | `graph`         | `RenderPass` trait + `FramePacket`                   |
/// | `passes`        | Built-in passes: `WorldPass`, `UiPass`               |
/// | `materials`     | `MaterialRegistry`, PBR material management          |
pub mod camera;
pub mod camera_system;
pub mod context;
pub mod frame_builder;
pub mod geometry;
pub mod gizmo_system;
pub mod graph;
pub mod materials;
pub mod passes;
pub mod pipeline;
pub mod render_stats;
pub mod render_target;
pub mod resources;
pub mod scene;

// -- Public re-exports --------------------------------------------------------

#[cfg(feature = "gui")]
pub use ferrous_ui_render::{GuiBatch, GuiQuad};
pub use glam;

pub use camera::{Camera, Controller, GpuCamera};
pub use camera_system::CameraSystem;
pub use ferrous_core::input::{KeyCode, MouseButton};
pub use frame_builder::FrameBuilder;
pub use geometry::{Mesh, Vertex};
pub use gizmo_system::GizmoSystem;
pub use graph::frame_packet::Viewport;
pub use graph::{FramePacket, InstancedDrawCommand, RenderPass};
pub use pipeline::InstancingPipeline;
pub use render_stats::RenderStats;
pub use render_target::HdrTexture;
pub use render_target::RenderTarget;
pub use resources::InstanceBuffer;
pub use scene::{Aabb, Frustum};
pub use scene::{CameraData, DirectionalLightData, RenderInstance, SceneData};

use materials::MaterialRegistry;
// re-export material types for API consumers
// re-export material primitives from `ferrous_core` directly so that they
// remain public even though the local `materials` module imports them
// privately.  this avoids the privacy errors encountered when the compiler
// treated the earlier exports as private imports.
pub use ferrous_core::scene::{
    AlphaMode, MaterialDescriptor, MaterialHandle, RenderStyle, MATERIAL_DEFAULT,
};

// texture handles/constants are owned by the texture registry but we expose
// them here so end users can construct `MaterialDescriptor` values without
// digging into submodules.
pub use resources::texture_registry::{
    TextureHandle, TEXTURE_BLACK, TEXTURE_NORMAL, TEXTURE_WHITE,
};

// -- Internal imports ---------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;

// TextBatch is no longer used separately

use camera::controller::OrbitState;
use graph::frame_packet::CameraPacket;

// some pass-related types are only required when GUI is enabled
#[cfg(feature = "gui")]
use passes::{CelFrameData, FlatFrameData, OutlineFrameData};

use passes::{
    CelShadedPass, FlatShadedPass, OutlinePass, PostProcessPass, PrePass, SsaoBlurPass, SsaoPass,
    WorldPass,
};

#[cfg(feature = "gpu-driven")]
use passes::CullPass;

#[cfg(feature = "gui")]
use passes::UiPass;
use pipeline::{PbrPipeline, PipelineLayouts};
use resources::SsaoResources;

// -- RenderDest ---------------------------------------------------------------

enum RenderDest<'a> {
    Target,
    View(&'a wgpu::TextureView),
}

// -- RendererMode -------------------------------------------------------------

/// Controls which passes are executed each frame.
///
/// | Mode | World / Post-process | UI |
/// |------|----------------------|----|
/// | `Full3D` | ✓ | ✓ |
/// | `Desktop2D` | ✗ (skipped) | ✓ (clears to `world_pass.clear_color`) |
///
/// Set via [`Renderer::set_mode`].  The default is [`RendererMode::Full3D`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RendererMode {
    /// Full 3-D pipeline: prepass → SSAO → world → style → gizmos → post-process → UI.
    #[default]
    Full3D,
    /// 2-D GUI-only pipeline: the UI pass clears the surface directly.
    /// WorldPass, render-style passes, gizmos, and post-process are all skipped.
    Desktop2D,
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
    /// A copy of the pipeline bind-group layouts; needed when creating
    /// new materials or other GPU resources that rely on them.
    // Note: shared mesh caches (cube/quad/sphere) and mesh_cache are now
    // owned by `frame_builder` (Phase 8).

    // -- Gizmo system (Phase 3: extracted to GizmoSystem) -------------------
    /// Owns the line-list GPU pipeline and per-frame draw queue.
    pub gizmo_system: GizmoSystem,

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
    /// Use `set_mode(RendererMode::Desktop2D)` for GUI-only applications.
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
}

// ---------------------------------------------------------------------------
// Compatibility accessors � allow existing code that uses `renderer.camera`
// and `renderer.orbit` to keep working without re-writing every call site.
// These are zero-cost inline forwards.

impl Renderer {
    /// Direct reference to the CPU camera state (read-only).
    #[inline]
    pub fn camera(&self) -> &Camera {
        &self.camera_system.camera
    }

    /// Mutable reference to the CPU camera state.
    #[inline]
    pub fn camera_mut(&mut self) -> &mut Camera {
        &mut self.camera_system.camera
    }

    /// Mutable reference to the orbit controller.
    #[inline]
    pub fn orbit_mut(&mut self) -> &mut OrbitState {
        &mut self.camera_system.orbit
    }
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
        hdri_path: Option<&std::path::Path>,
    ) -> Self {
        let device = &context.device;

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
            fovy: 45.0f32.to_radians(),
            aspect: width as f32 / height as f32,
            znear: 0.1,
            zfar: 2000.0,
            controller: Controller::with_default_wasd(),
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
            format,
            1024, // initial max instances
            width,
            height,
            sample_count,
        );

        // now that we have a GUI renderer instance, create the corresponding
        // UI pass and the post-process (tone-mapping) pass
        #[cfg(feature = "gui")]
        let ui_pass = UiPass::new(ui_renderer);
        let mut post_process_pass = PostProcessPass::new();
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
        let mut prepass = PrePass::new(device, layouts.instance.clone(), width, height);
        prepass.set_instance_buffer(instance_buf.bind_group.clone());
        let ssao_pass = SsaoPass::new(device, width, height);
        let ssao_blur_pass = SsaoBlurPass::new(device, width, height);
        let ssao_resources = SsaoResources::new(device, &context.queue);

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
            instance_layout: layouts.instance.clone(),
            gizmo_system,
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

    /// [`TextureHandle`] that can later be plugged into a
    /// [`MaterialDescriptor`].
    /// The texture is treated as **sRGB** color data (albedo, emissive).
    /// For non-color data use [`register_texture_linear`] instead.
    pub fn register_texture(&mut self, width: u32, height: u32, data: &[u8]) -> TextureHandle {
        let handle = self.material_registry.register_texture_rgba8(
            &self.context.device,
            &self.context.queue,
            width,
            height,
            data,
        );
        // maintain the old behaviour of refreshing the world-pass table
        // even though the material bind groups are unchanged by adding a
        // standalone texture.  this keeps external code from implicitly
        // depending on the side-effect.
        self.world_pass.set_material_table(
            &self.material_registry.bind_group_table(),
            &self.material_registry,
        );
        self.sync_style_material_table();
        handle
    }

    /// Register a GPU texture from raw RGBA8 **linear** bytes and return a
    /// [`TextureHandle`].  Use this for non-color data such as normal maps,
    /// metallic-roughness and AO textures � the GPU will NOT apply gamma
    /// correction when sampling these.
    pub fn register_texture_linear(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> TextureHandle {
        let handle = self.material_registry.register_texture_rgba8_linear(
            &self.context.device,
            &self.context.queue,
            width,
            height,
            data,
        );
        self.world_pass.set_material_table(
            &self.material_registry.bind_group_table(),
            &self.material_registry,
        );
        self.sync_style_material_table();
        handle
    }

    /// Free a texture previously registered with [`register_texture`].  if
    /// the provided handle corresponds to one of the three built-in
    /// fallbacks this function is a no-op.  freed slots are recycled by the
    /// registry on future registrations.
    pub fn free_texture(&mut self, handle: TextureHandle) {
        self.material_registry.free_texture(handle);
    }

    /// Create a material using a full descriptor.  Returns a
    /// [`MaterialHandle`] that may be stored by the application.
    pub fn create_material(&mut self, desc: &MaterialDescriptor) -> MaterialHandle {
        let handle = self
            .material_registry
            .create(&self.context.device, &self.context.queue, desc);
        // sync the world pass table so newly-created slot is available to
        // shaders immediately; forgetting this leads to panics when the
        // pass tries to index past the end of its internal array.
        self.world_pass.set_material_table(
            &self.material_registry.bind_group_table(),
            &self.material_registry,
        );
        self.sync_style_material_table();
        handle
    }

    /// Register a mesh under a string key so that world elements can refer to
    /// it later.  This simply inserts the mesh into the renderer's internal
    /// cache; calling `sync_world` will cause any `ElementKind::Mesh`
    /// elements referencing `key` to use the provided geometry.  If a mesh
    /// already existed at that key it is overwritten.
    #[cfg(feature = "assets")]
    pub fn register_mesh(&mut self, key: &str, mesh: geometry::Mesh) {
        self.frame_builder.mesh_cache.insert(key.to_string(), mesh);
    }

    /// Remove a previously-registered mesh.  Any world elements still
    /// referring to the key will fall back to the cube primitive when the
    /// next `sync_world` runs.
    #[cfg(feature = "assets")]
    pub fn free_mesh(&mut self, key: &str) {
        self.frame_builder.mesh_cache.remove(key);
    }

    /// Free a material slot so that the corresponding bind group may be
    /// reused later.  the slot is overwritten with a clone of
    /// [`MATERIAL_DEFAULT`], ensuring that any draw commands referencing the
    /// old handle will simply render using the default material instead of
    /// crashing the GPU.  After freeing we refresh the world-pass table.
    pub fn free_material(&mut self, handle: MaterialHandle) {
        self.material_registry.free(handle);
        self.world_pass.set_material_table(
            &self.material_registry.bind_group_table(),
            &self.material_registry,
        );
        self.sync_style_material_table();
    }

    /// Update the scalar parameters of an existing material.  Only the
    /// uniform buffer is rewritten; texture handles are assumed to remain
    /// constant.  This is useful for tweaking colour/roughness/etc without
    /// reallocating the material.
    pub fn update_material_params(&mut self, handle: MaterialHandle, desc: &MaterialDescriptor) {
        self.material_registry
            .update_params(&self.context.queue, handle, desc);
    }

    /// Overwrite the contents of a texture handle with new RGBA8 data.  This
    /// is the hot-reload API; it does not allocate a new GPU texture but
    /// instead writes directly into the existing one.  Materials already
    /// pointing at the handle will observe the update automatically.
    pub fn update_texture_data(
        &mut self,
        handle: TextureHandle,
        width: u32,
        height: u32,
        data: &[u8],
    ) {
        self.material_registry.update_texture_data(
            &self.context.queue,
            handle,
            width,
            height,
            data,
        );
    }

    /// Adjust the global directional light used by the PBR shaders.
    ///
    /// The direction should be a normalized vector pointing *from* the light
    /// toward the scene.  Colour is linear RGB and `intensity` is a scalar
    /// multiplier.
    pub fn set_directional_light(&mut self, direction: [f32; 3], color: [f32; 3], intensity: f32) {
        // start from the default so we automatically populate
        // `light_view_proj` (it will be recalculated again by
        // `WorldPass::update_light` below).
        let mut uniform = crate::resources::light::DirectionalLightUniform::default();
        uniform.direction = direction;
        uniform.color = color;
        uniform.intensity = intensity;
        // Cache for style-pass frame data injection.
        self.current_dir_light = uniform;
        // delegate to world pass so that the buffer is encapsulated
        self.world_pass.update_light(&self.context.queue, uniform);
    }

    /// Switch to a new render style.
    ///
    /// * `RenderStyle::Pbr` — drops all style passes; the default `WorldPass` is used.
    /// * `RenderStyle::CelShaded` — creates `CelShadedPass` + optional `OutlinePass`.
    /// * `RenderStyle::FlatShaded` — creates `FlatShadedPass`.
    ///
    /// The method propagates the current instance buffer and material table to
    /// newly-created passes immediately so they are ready to draw on the next frame.
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

    /// Upload an explicit list of point lights to the GPU storage buffer.
    ///
    /// Call this if you manage lights outside of the `World` ECS (e.g. from a
    /// custom system).  For the automatic path see `sync_world`.
    pub fn set_point_lights(&mut self, lights: &[crate::resources::PointLightUniform]) {
        self.world_pass
            .update_point_lights(&self.context.device, &self.context.queue, lights);
    }

    /// Switch between the full 3-D pipeline and the lightweight 2-D/GUI-only
    /// pipeline.
    ///
    /// In `Desktop2D` mode the world pass, render-style passes, gizmos, and
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
            let clear = if mode == RendererMode::Desktop2D {
                Some(self.world_pass.clear_color)
            } else {
                None
            };
            self.ui_pass.set_clear_color(clear);
        }
    }

    #[cfg(feature = "gui")]
    pub fn render_to_target(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        ui_batch: Option<GuiBatch>,
    ) {
        self.do_render(encoder, RenderDest::Target, ui_batch);
    }

    /// Renders directly into an external `TextureView` (e.g. swapchain frame).
    #[cfg(feature = "gui")]
    pub fn render_to_view(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        ui_batch: Option<GuiBatch>,
    ) {
        self.do_render(encoder, RenderDest::View(view), ui_batch);
    }

    // -- Pass management -----------------------------------------------------

    // -- Phase 11: GPU-driven culling API ------------------------------------

    /// Enables or disables GPU-driven frustum culling.
    ///
    /// When `true`, `sync_world` uploads per-instance cull data to the GPU and
    /// `do_render` dispatches the cull compute shader before `WorldPass`.
    /// `WorldPass` will then use `draw_indexed_indirect` instead of `draw_indexed`.
    ///
    /// Disabling reverts to the CPU `draw_indexed` path using `instance_buf`.
    #[cfg(feature = "gpu-driven")]
    pub fn enable_gpu_culling(&mut self, enabled: bool) {
        self.gpu_culling_enabled = enabled;
        if enabled && self.cull_pass.is_none() {
            self.cull_pass = Some(CullPass::new(&self.context.device, &self.pipeline_layouts));
        }
        if !enabled {
            self.world_pass.clear_indirect_buffer();
        }
    }

    /// Returns visible-instance counts per batch from the most recent GPU cull pass.
    ///
    /// Performs a **synchronous** device poll + staging buffer readback.
    /// Call this *after* rendering a frame to obtain per-batch culling statistics.
    ///
    /// Returns an empty `Vec` if GPU culling is disabled or no batches were drawn.
    #[cfg(feature = "gpu-driven")]
    pub fn cull_visible_counts(&self) -> Vec<u32> {
        if let Some(cp) = &self.cull_pass {
            cp.sync_patch_indirect(&self.context.device, &self.context.queue)
        } else {
            vec![]
        }
    }

    // -- Scene sync ----------------------------------------------------------

    /// Push a fully-assembled [`SceneData`] to the renderer for this frame.
    ///
    /// This is the **preferred** entry point going forward.  The application
    /// layer (e.g. `ferrous_app`) queries the ECS, builds a [`SceneData`],
    /// and calls this method — keeping the renderer free of ECS knowledge.
    ///
    /// `sync_world` is kept for backward compatibility; it converts ECS state
    /// into an equivalent `SceneData` and delegates here.
    pub fn set_scene(&mut self, scene: &SceneData) {
        // 1. Apply camera if provided
        if let Some(cam) = &scene.camera {
            self.camera_system.camera.eye = cam.eye;
            self.camera_system.camera.target = cam.target;
            self.camera_system.camera.fovy = cam.fov_y;
            self.camera_system.camera.znear = cam.z_near;
            self.camera_system.camera.zfar = cam.z_far;
        }

        // 2. Apply directional light if provided
        if let Some(light) = &scene.directional_light {
            self.set_directional_light(
                light.direction.to_array(),
                light.color.to_array(),
                light.intensity,
            );
        }

        // 3. Mark scene dirty so frame_builder rebuilds next frame.
        //    Instance uploads are still handled by sync_world / build_world_commands
        //    until the full ECS→SceneData migration is complete (Phase 3 step 2).
        if !scene.instances.is_empty() {
            self.frame_builder.scene_dirty = true;
        }
    }

    /// Queue a gizmo for rendering this frame.
    /// Phase 9: Delegates geometry work to `FrameBuilder::build_world_commands`
    /// which queries the ECS directly.  All rendering goes through the ECS
    /// instanced path — the legacy `add_object` / `ModelBuffer` path is gone.
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
            use ferrous_core::scene::Material;
            // Collect (ecs_entity_index, MaterialDescriptor) for entities
            // that have a Material component attached.
            let mat_only: Vec<(u32, ferrous_core::scene::MaterialDescriptor)> = world
                .ecs
                .query::<Material>()
                .map(|(e, m)| (e.index, m.to_descriptor()))
                .collect();
            for (ecs_idx, desc) in mat_only {
                let ecs_id = ecs_idx as u64;
                let needs_update = self
                    .world_material_descs
                    .get(&ecs_id)
                    .map(|prev| *prev != desc)
                    .unwrap_or(true);
                if needs_update {
                    // Match to an Element by its id (bridge stores ECS index as id)
                    for element in world.iter() {
                        if element.id == ecs_id {
                            self.material_registry.update_params(
                                &self.context.queue,
                                element.material.handle,
                                &desc,
                            );
                            self.world_material_descs.insert(ecs_id, desc.clone());
                            break;
                        }
                    }
                }
            }
        }

        // 1. Build frustum from current camera
        let camera_packet = crate::graph::frame_packet::CameraPacket {
            view_proj: self.camera_system.camera.build_view_projection_matrix(),
            eye: self.camera_system.camera.eye,
        };
        let frustum = crate::scene::Frustum::from_view_proj(&camera_packet.view_proj);

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

        // -- Phase 11: GPU-driven cull data upload ---------------------------
        #[cfg(feature = "gpu-driven")]
        {
            if self.gpu_culling_enabled {
                // Ensure the CullPass exists.
                if self.cull_pass.is_none() {
                    self.cull_pass =
                        Some(CullPass::new(&self.context.device, &self.pipeline_layouts));
                }

                // `build_world_commands` populated world_instanced and
                // world_instance_matrices above.  We use those to build cull data.
                let instanced = &self.frame_builder.world_instanced;
                let matrices = &self.frame_builder.world_instance_matrices;

                if let Some(cp) = &mut self.cull_pass {
                    use crate::resources::draw_indirect::{
                        GpuDrawIndexedIndirect, InstanceCullData,
                    };

                    let mut cull_data: Vec<InstanceCullData> = Vec::with_capacity(matrices.len());
                    let mut templates: Vec<GpuDrawIndexedIndirect> =
                        Vec::with_capacity(instanced.len());

                    for (cmd_idx, cmd) in instanced.iter().enumerate() {
                        // Emit one template per batch (index_count, first_instance).
                        // instance_count = 0 — the cull shader fills it.
                        templates.push(GpuDrawIndexedIndirect {
                            index_count: cmd.index_count,
                            instance_count: 0,
                            first_index: 0,
                            base_vertex: 0,
                            first_instance: cmd.first_instance,
                        });

                        // Emit one InstanceCullData per entity within this batch.
                        let base = cmd.first_instance as usize;
                        for inst_idx in 0..cmd.instance_count as usize {
                            let model = matrices
                                .get(base + inst_idx)
                                .copied()
                                .unwrap_or(glam::Mat4::IDENTITY);
                            // Use a conservative world-space AABB that ensures nothing
                            // gets incorrectly culled for the Phase 11 baseline.
                            // Future passes (Phase 12) will extract real AABBs from mesh assets.
                            let aabb_half = glam::Vec3::splat(100.0);
                            let aabb_center = glam::Vec3::ZERO;
                            cull_data.push(InstanceCullData::new(
                                model,
                                aabb_center,
                                aabb_half,
                                cmd_idx as u32,
                            ));
                        }
                    }

                    if !cull_data.is_empty() {
                        cp.upload_instances(
                            &self.context.device,
                            &self.context.queue,
                            &cull_data,
                            &templates,
                        );
                        cp.reset_counters(&self.context.queue);
                        cp.update_params(&self.context.queue, &frustum);

                        // Arm WorldPass with the GPU-driven indirect buffer and
                        // compacted output instance bind group.
                        let indirect = cp.indirect_buf.buffer.clone();
                        let out_bg = cp.out_instance_bg.clone();
                        self.world_pass.set_indirect_buffer(indirect, out_bg);
                    } else {
                        self.world_pass.clear_indirect_buffer();
                    }
                }
            } else {
                // CPU-driven path — ensure WorldPass does not use stale indirect buf.
                self.world_pass.clear_indirect_buffer();
            }
        }
        for element in world.iter() {
            let id = element.id;
            let desc = &element.material.descriptor;
            let needs_update = self
                .world_material_descs
                .get(&id)
                .map(|prev| prev != desc)
                .unwrap_or(true);
            if needs_update {
                self.material_registry.update_params(
                    &self.context.queue,
                    element.material.handle,
                    desc,
                );
                self.world_material_descs.insert(id, desc.clone());
            }
        }

        // Prune entries for despawned entities
        let live_ids: std::collections::HashSet<u64> = world.iter().map(|e| e.id).collect();
        self.world_material_descs
            .retain(|id, _| live_ids.contains(id));

        // 4. Sync material table to style passes
        self.world_pass.set_material_table(
            &self.material_registry.bind_group_table(),
            &self.material_registry,
        );
        self.sync_style_material_table();

        // 5. Collect point lights from World entities
        let mut point_light_uniforms: Vec<crate::resources::PointLightUniform> = Vec::new();
        for element in world.iter() {
            if let Some(pl) = &element.point_light {
                let pos = element.transform.position;
                point_light_uniforms.push(crate::resources::PointLightUniform::new(
                    [pos.x, pos.y, pos.z],
                    pl.radius,
                    pl.color,
                    pl.intensity,
                ));
            }
        }
        self.world_pass.update_point_lights(
            &self.context.device,
            &self.context.queue,
            &point_light_uniforms,
        );
    } // -- Pass management ------------------------------------------------------

    /// Queue a gizmo for rendering this frame.
    ///
    /// Typically called by the `ferrous_app` runner which drains
    /// `AppContext::gizmos` after `FerrousApp::draw_3d` returns � app code
    /// should push to `ctx.gizmos` rather than calling this directly.
    ///
    /// The gizmo list is automatically cleared after
    /// [`GizmoSystem::execute`] runs, so there is no need to manage lifetime
    /// manually.
    pub fn queue_gizmo(&mut self, gizmo: scene::GizmoDraw) {
        self.gizmo_system.queue(gizmo);
        // mark scene dirty so that the world pass will rebuild the packet; the
        // gizmos are drawn separately but the packet cache logic should reset
        // when an unrelated draw request arrives.
        self.frame_builder.scene_dirty = true;
    }

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
    }

    /// Explicitly sets the viewport rectangle and updates the camera aspect ratio.
    pub fn set_viewport(&mut self, vp: Viewport) {
        self.viewport = vp;
        self.camera_system
            .set_aspect(vp.width as f32 / vp.height as f32);
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
        #[cfg(feature = "gui")]
        {
            self.ui_pass.set_font_atlas(view, sampler);
        }
    }

    // -- Input ----------------------------------------------------------------

    /// Applies keyboard/mouse input to the orbit camera. `dt` is seconds elapsed.
    pub fn handle_input(&mut self, input: &mut ferrous_core::input::InputState, dt: f32) {
        self.camera_system.handle_input(input, dt);
    }

    // -- Private helpers ------------------------------------------------------

    /// Propagates the current material bind-group table to all active style passes.
    fn sync_style_material_table(&mut self) {
        let table = self.material_registry.bind_group_table();
        if let Some(p) = &mut self.cel_pass {
            p.set_material_table(&table);
        }
        if let Some(p) = &mut self.outline_pass {
            p.set_material_table(&table);
        }
        if let Some(p) = &mut self.flat_pass {
            p.set_material_table(&table);
        }
    }

    /// Propagates a new instance-buffer bind group to all active style passes.
    fn sync_style_instance_buffer(&mut self, bg: Arc<wgpu::BindGroup>) {
        if let Some(p) = &mut self.cel_pass {
            p.set_instance_buffer(bg.clone());
        }
        if let Some(p) = &mut self.outline_pass {
            p.set_instance_buffer(bg.clone());
        }
        if let Some(p) = &mut self.flat_pass {
            p.set_instance_buffer(bg);
        }
    }

    #[cfg(feature = "gui")]
    fn do_render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        dest: RenderDest<'_>,
        ui_batch: Option<GuiBatch>,
    ) {
        self.camera_system.sync_gpu(&self.context.queue);

        let camera_packet = CameraPacket {
            view_proj: self.camera_system.camera.build_view_projection_matrix(),
            eye: self.camera_system.camera.eye,
        };
        let (mut packet, stats) = self.frame_builder.build(self.viewport, camera_packet);
        // Propagate the (possibly-reallocated) instance buffer to style passes.
        self.sync_style_instance_buffer(self.instance_buf.bind_group.clone());
        self.render_stats = stats;

        if let Some(b) = ui_batch {
            packet.insert(b);
        }

        // ── Desktop2D fast path ───────────────────────────────────────────────
        // In GUI-only mode skip the world pass, render-style passes, gizmos,
        // and post-process entirely.  The UI pass already holds a clear_color
        // set by `set_mode` so it will clear the surface before drawing.
        if self.mode == RendererMode::Desktop2D {
            let target_view = match dest {
                RenderDest::Target => &self.render_target.color.view,
                RenderDest::View(v) => v,
            };
            self.ui_pass
                .prepare(&self.context.device, &self.context.queue, &packet);
            self.ui_pass.execute(
                &self.context.device,
                &self.context.queue,
                encoder,
                target_view,
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
                    target_view,
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
            let view = self.camera_system.view_matrix();
            let proj = self.camera_system.proj_matrix();
            self.prepass
                .update_camera(&self.context.queue, view, proj, self.camera_system.eye());
            self.prepass
                .prepare(&self.context.device, &self.context.queue, &packet);
            self.prepass.execute(
                &self.context.device,
                &self.context.queue,
                encoder,
                &dummy_view,
                None,
                None,
                &packet,
            );
        }

        // -- 2. SSAO passes (only when enabled) --------------------------------
        if self.ssao_enabled {
            let proj = self.camera_system.proj_matrix();
            let inv_proj = proj.inverse();
            let ssao_w = self.ssao_pass.ssao_texture.width;
            let ssao_h = self.ssao_pass.ssao_texture.height;
            self.ssao_resources
                .update_params(&self.context.queue, ssao_w, ssao_h, proj, inv_proj);

            self.ssao_pass.run(
                &self.context.device,
                encoder,
                &self.ssao_resources,
                &self.prepass.normal_depth,
            );

            self.ssao_blur_pass.run(
                &self.context.device,
                encoder,
                &self.ssao_pass.ssao_texture,
                &self.prepass.normal_depth,
            );

            let ssao_view = Arc::new(
                self.ssao_blur_pass
                    .blurred
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default()),
            );
            let ssao_sampler = Arc::new(self.context.device.create_sampler(
                &wgpu::SamplerDescriptor {
                    label: Some("SSAO Result Sampler"),
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    address_mode_u: wgpu::AddressMode::ClampToEdge,
                    address_mode_v: wgpu::AddressMode::ClampToEdge,
                    ..Default::default()
                },
            ));
            self.world_pass
                .update_ssao(&self.context.device, ssao_view, ssao_sampler);
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
        self.world_pass
            .prepare(&self.context.device, &self.context.queue, &packet);
        self.world_pass.execute(
            &self.context.device,
            &self.context.queue,
            encoder,
            &dummy_view,
            None,
            Some(&self.render_target.depth.view),
            &packet,
        );

        // -- 3b. Render Style Passes ------------------------------------------
        // Inject per-frame data for whichever style is active, then run its
        // pass(es).  All style passes render into the same HDR texture
        // (LoadOp::Load) so they composite on top of the world geometry.
        match &self.render_style {
            RenderStyle::CelShaded {
                toon_levels,
                outline_width,
            } => {
                let toon_levels = *toon_levels;
                let outline_width = *outline_width;
                packet.insert(CelFrameData {
                    light: self.current_dir_light,
                    toon_levels,
                    outline_width,
                });
                if outline_width > 0.0 {
                    packet.insert(OutlineFrameData {
                        light: self.current_dir_light,
                        toon_levels,
                        outline_width,
                        color: [0.0, 0.0, 0.0, 1.0],
                    });
                }
                if let Some(p) = &mut self.cel_pass {
                    p.prepare(&self.context.device, &self.context.queue, &packet);
                    p.execute(
                        &self.context.device,
                        &self.context.queue,
                        encoder,
                        &self.world_pass.hdr_texture.view,
                        None,
                        Some(&self.render_target.depth.view),
                        &packet,
                    );
                }
                if outline_width > 0.0 {
                    if let Some(p) = &mut self.outline_pass {
                        p.prepare(&self.context.device, &self.context.queue, &packet);
                        p.execute(
                            &self.context.device,
                            &self.context.queue,
                            encoder,
                            &self.world_pass.hdr_texture.view,
                            None,
                            Some(&self.render_target.depth.view),
                            &packet,
                        );
                    }
                }
            }
            RenderStyle::FlatShaded => {
                packet.insert(FlatFrameData {
                    light: self.current_dir_light,
                });
                if let Some(p) = &mut self.flat_pass {
                    p.prepare(&self.context.device, &self.context.queue, &packet);
                    p.execute(
                        &self.context.device,
                        &self.context.queue,
                        encoder,
                        &self.world_pass.hdr_texture.view,
                        None,
                        Some(&self.render_target.depth.view),
                        &packet,
                    );
                }
            }
            RenderStyle::Pbr => {}
        }

        // -- 4. Gizmo Pass -----------------------------------------------------
        self.gizmo_system.execute(
            &self.context.device,
            encoder,
            &self.world_pass.hdr_texture.view,
            &self.render_target.depth.view, // Use real depth target instead of shadow map
            &self.camera_system.gpu.bind_group,
        );

        // -- 5. Post-Process (Tone Mapping) ------------------------------------
        let target_view = match dest {
            RenderDest::Target => &self.render_target.color.view,
            RenderDest::View(v) => v,
        };

        self.post_process_pass.render(
            &self.context.device,
            encoder,
            &self.world_pass.hdr_texture,
            target_view,
        );

        // -- 6. UI Pass --------------------------------------------------------
        self.ui_pass
            .prepare(&self.context.device, &self.context.queue, &packet);
        self.ui_pass.execute(
            &self.context.device,
            &self.context.queue,
            encoder,
            target_view,
            None,
            None,
            &packet,
        );

        // -- 7. Extra Passes ---------------------------------------------------
        for pass in &mut self.extra_passes {
            pass.prepare(&self.context.device, &self.context.queue, &packet);
            pass.execute(
                &self.context.device,
                &self.context.queue,
                encoder,
                target_view,
                None,
                None,
                &packet,
            );
        }

        self.frame_builder.reclaim(packet);
    }
}
