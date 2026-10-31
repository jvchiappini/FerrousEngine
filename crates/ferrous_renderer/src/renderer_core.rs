//! `ferrous_renderer::renderer_core` -- Core Renderer struct and lifecycle methods.
//!
//! This module contains the fundamental Renderer struct definition and its core
//! lifecycle and state management methods. The Renderer is the top-level GPU
//! rendering context that holds all GPU resources and executes render passes.
use std::collections::HashMap;
use std::sync::Arc;

// Re-export types needed for the Renderer struct
// Module declarations
// Module re-exports from the crate root (since files are located in src/)
pub use crate::camera;
pub use crate::camera_system;
pub use crate::frame_builder;
pub use crate::gizmo_system;
pub use crate::pipeline;
pub use crate::render_stats;

// Re-export types needed for the Renderer struct
pub use crate::geometry::{compute_tangents, Mesh, Vertex};
pub use crate::graph::frame_packet::Viewport;
pub use crate::graph::RenderPass;
pub use crate::passes::{
    CelShadedPass, FlatShadedPass, OutlinePass, ParticleSystem, PostProcessPass, PrePass,
    ProceduralSkyPass, SkinningPass, SkyMode, SsaoBlurPass, SsaoPass, WorldPass,
};
pub use crate::render_target::RenderTarget;
pub use crate::resources::InstanceBuffer;
pub use camera::{Camera, Controller, GpuCamera};
pub use camera_system::CameraSystem;
pub use ferrous_core::scene::{MaterialDescriptor, RenderStyle};
pub use ferrous_core::Color;
pub use frame_builder::FrameBuilder;
pub use gizmo_system::GizmoSystem;
pub use pipeline::PipelineLayouts;
pub use render_stats::RenderStats;

#[cfg(feature = "gui")]
pub use crate::passes::UiPass;

/// Optional per-frame GUI batch consumed by [`Renderer::render_to_view`].
///
/// With the `gui` feature this is `ferrous_gui::GuiBatch`; without it, it is an
/// uninhabited-in-practice placeholder so the renderer and the Pure2D headless
/// harness build with `default-features = false`.  Callers always pass `None`
/// in that configuration.
#[cfg(feature = "gui")]
pub use ferrous_gui::GuiBatch as UiBatch;
/// Placeholder GUI batch used when the `gui` feature is disabled.
#[cfg(not(feature = "gui"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct UiBatch;

#[cfg(feature = "gpu-driven")]
pub use crate::passes::CullPass;

// Internal imports needed for method implementations
use crate::materials::MaterialRegistry;
use crate::resources::SsaoResources;
use camera::controller::OrbitState;
pub use pipeline::InstancingPipeline;
use pipeline::PbrPipeline;

use ferrous_2d::render::{DrawCommand2d, FontId2d, Renderer2d, ShapeBatcher, TextInstance};
use ferrous_font::{FontError, GlyphQuad, TextAlign};

use crate::render_config::Render2dConfig;
use crate::static_2d::{Static2dBatch, Static2dBatchId, Static2dBatchSlot};

use crate::graph::frame_packet::CameraPacket;
use crate::scene::culling::Frustum;
use crate::scene::scene_data::SceneData;
use ferrous_core::context::EngineContext;

/// Controls which passes are executed each frame.
///
/// | Mode | World / Post-process | UI |
/// |------|----------------------|----|
enum RenderDest<'a> {
    Target,
    View(&'a wgpu::TextureView),
}

/// A maximal run of consecutive same-key commands in the Pure2D painter-order
/// command stream.  `start`/`count` index the coalesced painter-order uploads
/// produced by the Pure2D path, not the batchers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Coalesced2dDraw {
    /// `ShapeBatcher` SDF pipeline, `[start, start+count)` in the shape upload.
    Shapes { start: u32, count: u32 },
    /// Sprite pipeline bound to `texture_id`, `[start, start+count)` in the
    /// sprite upload.
    Sprites {
        texture_id: u32,
        start: u32,
        count: u32,
    },
    /// Text pipeline bound to `font`'s atlas, `[start, start+count)` in the
    /// text upload.
    Text {
        font: FontId2d,
        start: u32,
        count: u32,
    },
}

/// A registered 2D font: its baked atlas and the bind group that samples it.
struct Font2dEntry {
    font: ferrous_font::Font,
    bind_group: std::sync::Arc<wgpu::BindGroup>,
}

/// World-space axis-aligned bounding box `(min, max)` of a [`ShapeInstance`]'s
/// canonical quad, used by the Pure2D culling pass.
///
/// The instance's `Mat4` maps a canonical local quad to world space.  The
/// shader has two canonical quads:
///
/// - rounded-rect / circle / line family: `[-0.5, 0.5]²` (`vertex_index`-based
///   corners are re-centred by `0.5` in `shape_2d.wgsl`);
/// - triangle (`params.w > 1.5`, see `SHAPE_KIND_TRIANGLE`): `[0, 1]²`, with
///   `c0 = b - a`, `c1 = c - a`, `c3 = a` (the triangle itself is contained).
///
/// Transforming the four corners and taking their min/max yields a world-space
/// AABB that contains the rendered geometry.  It is **conservative but not the
/// tight SDF bounds**: for lines the round/square caps sit exactly on the quad
/// edge, so the AABB is tight; the fragment shader's AA band and the sub-pixel
/// line widening extend a few pixels past it, which is why the cull rect is
/// expanded by a margin before testing.  No constructor produces geometry
/// outside its canonical quad.
#[inline]
fn shape_world_aabb(s: &ferrous_2d::render::ShapeInstance) -> (glam::Vec2, glam::Vec2) {
    s.world_aabb()
}

/// World-space AABB of a [`SpriteInstance`].  `sprite_2d.wgsl` maps the
/// canonical `[-0.5, 0.5]²` quad through the instance matrix (no AA expansion),
/// so this is exact.
#[inline]
fn sprite_world_aabb(s: &ferrous_2d::render::SpriteInstance) -> (glam::Vec2, glam::Vec2) {
    let c0 = glam::Vec2::new(s.transform_c0[0], s.transform_c0[1]);
    let c1 = glam::Vec2::new(s.transform_c1[0], s.transform_c1[1]);
    let c3 = glam::Vec2::new(s.transform_c3[0], s.transform_c3[1]);
    let mut min = glam::Vec2::splat(f32::INFINITY);
    let mut max = glam::Vec2::splat(f32::NEG_INFINITY);
    for (lx, ly) in [(-0.5f32, -0.5f32), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)] {
        let p = c3 + c0 * lx + c1 * ly;
        min = min.min(p);
        max = max.max(p);
    }
    (min, max)
}

/// Standard separating-axis AABB overlap test.  Touching counts as overlap
/// (the cull rect already carries the AA margin).
#[inline]
fn aabb_intersects(
    min: glam::Vec2,
    max: glam::Vec2,
    rect_min: glam::Vec2,
    rect_max: glam::Vec2,
) -> bool {
    min.x <= rect_max.x && max.x >= rect_min.x && min.y <= rect_max.y && max.y >= rect_min.y
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
    /// Use this ONLY for pure Dear-ImGui / GUI overlays with no geometric 2D.
    Flat2D,
    /// Pure 2-D pipeline: shapes + sprites rendered directly to the swapchain.
    /// WorldPass, SSAO, prepass, gizmos, particles, and post-process are all skipped.
    /// The camera is automatically configured as orthographic pixel-perfect
    /// (1 world-unit = 1 pixel, origin at screen center) when using `enable_pure_2d()`.
    /// Use this for 2D games, mathematical visualizations, and animation tools.
    Pure2D,
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
/// One recorded 2D view (multi-viewport Pure2D path).
#[derive(Clone, Copy, Debug)]
struct View2dCommand {
    /// Pixel rect `[x, y, w, h]` in the render target.
    rect: [f32; 4],
    view_proj: glam::Mat4,
    shape_start: usize,
    shape_end: usize,
    /// Draw the grid + retained static batches for this view (background
    /// views). Overlay-only views skip them so the scene is not drawn twice.
    background: bool,
}

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
    pub world_material_descs: HashMap<u64, ferrous_core::scene::MaterialDescriptor>,
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
    pub material_registry: MaterialRegistry,

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

    // -- Runtime render/performance configuration (audit M4/H8/H3) ------------
    /// Runtime-settable config knobs.  See [`Render2dConfig`].
    pub render_2d_config: Render2dConfig,

    // -- Wall-clock frame timing ---------------------------------------------
    /// Wall-clock duration of the last frame in milliseconds.  Written by the
    /// engine runner around the per-frame work; `0.0` when no runner is used
    /// (e.g. the headless harness).  Only meaningful as a same-machine
    /// relative delta — never an absolute fps claim.
    pub frame_ms: f32,
    /// Exponential moving average of [`Self::frame_ms`] (smoothing factor
    /// `1/60`).  Cheap to update, no allocation.
    pub frame_ms_avg: f32,
    /// Number of frames observed by [`Self::record_frame_time`].
    pub frame_count: u64,

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
    /// 2D renderer para overlay sobre el HDR texture en Full3D (con MSAA del mundo).
    pub renderer_2d: Renderer2d,
    /// 2D renderer para Pure2D: sample_count=1, formato swapchain, sin overhead HDR.
    pub renderer_2d_pure: Renderer2d,
    /// Recorded multi-view segments (`begin_2d_view`/`end_2d_view`).
    views_2d: Vec<View2dCommand>,
    view_2d_open: bool,
    pub shape_batcher: ShapeBatcher,
    /// Batcher de sprites dedicado al modo Pure2D.
    pub sprite_batcher_2d: ferrous_2d::render::SpriteBatcher,
    /// Reusable scratch buffer for the per-frame, z-sorted unified 2D draw
    /// list (`(z, DrawCommand2d)`).  Reused when
    /// [`Render2dConfig::scratch_reuse`] is set (default), avoiding one heap
    /// allocation + grow per frame (audit H8/H3).
    draw_list_scratch: Vec<(f32, DrawCommand2d)>,
    /// Reusable bucket-order flat copy of the sprite instances, used to resolve
    /// the flat indices stored in `DrawCommand2d::Sprite` while the coalesced
    /// painter-order upload is built.
    sprite_flat_scratch: Vec<ferrous_2d::render::SpriteInstance>,
    /// Reusable painter-ordered shape upload for the coalesced Pure2D path.
    shape_upload_scratch: Vec<ferrous_2d::render::ShapeGpu>,
    /// Reusable painter-ordered sprite upload for the coalesced Pure2D path.
    sprite_upload_scratch: Vec<ferrous_2d::render::SpriteInstance>,
    /// Reusable list of coalesced draw runs, emitted in painter order.
    coalesced_2d_scratch: Vec<Coalesced2dDraw>,

    // -- Retained static 2D batches ------------------------------------------
    /// Persistent per-batch shape buffers.  `None` slots are free; the slot
    /// generation invalidates stale [`Static2dBatchId`]s.
    static_2d_batches: Vec<Static2dBatchSlot>,
    /// Free slot indices, reused before the table grows.
    static_2d_free: Vec<u32>,
    /// Reusable ordering scratch (slot indices sorted by batch `z`).
    static_2d_order_scratch: Vec<u32>,
    /// Reusable list of visible `(start, count)` chunk runs for one batch.
    static_2d_runs_scratch: Vec<(u32, u32)>,

    /// Bind groups de texturas indexados por texture_id para sprites 2D en Pure2D.
    pub sprite_bind_groups: std::collections::HashMap<u32, std::sync::Arc<wgpu::BindGroup>>,
    /// High-water mark: the next never-before-used sprite texture id.
    pub next_sprite_texture_id: u32,
    /// Ids returned by `free_sprite_texture`, reused before allocating a new id.
    /// Without recycling, repeatedly loading plan textures grew
    /// `next_sprite_texture_id` without bound.
    pub freed_sprite_texture_ids: Vec<u32>,

    // -- Pure2D text ----------------------------------------------------------
    /// Registered 2D fonts (atlas + sampling bind group), keyed by handle.
    fonts_2d: HashMap<FontId2d, Font2dEntry>,
    /// High-water mark for [`FontId2d`] allocation.
    next_font_2d_id: u32,
    /// Font ids returned by [`Self::free_font_2d`], reused before allocating.
    freed_font_2d_ids: Vec<u32>,
    /// Per-frame glyph instances plus the font each one belongs to.
    /// `DrawCommand2d::Text(i)` indexes this vector directly, so submission
    /// order is preserved for equal `z`.
    pub text_batcher_2d: Vec<(FontId2d, TextInstance)>,
    /// Reusable CPU layout output for [`Self::draw_text_2d`].
    text_layout_scratch: Vec<GlyphQuad>,
    /// Reusable painter-ordered text upload for the coalesced Pure2D path.
    text_upload_scratch: Vec<TextInstance>,

    // -- Exportacion Headless (Fase 1) ----------------------------------------
    pub readback_manager: Option<crate::resources::readback::ReadbackFrameManager>,
}

impl Renderer {
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
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
            projection: ferrous_core::scene::camera::Projection::Perspective {
                fov_y_radians: 45.0f32.to_radians(),
                aspect_ratio: width as f32 / height as f32,
                z_near: 0.1,
                z_far: 2000.0,
            },
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
            context.pipeline_cache.clone(),
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
        let mut prepass = PrePass::new(
            device,
            layouts.instance.clone(),
            width,
            height,
            sample_count,
        );
        prepass.set_instance_buffer(instance_buf.bind_group.clone());
        prepass.set_material_table(&material_registry.bind_group_table(), &material_registry);
        let ssao_pass = SsaoPass::new(device, width, height);
        let ssao_blur_pass = SsaoBlurPass::new(device, width, height);
        let ssao_resources = SsaoResources::new(device, &context.queue);
        let skinning_pass = SkinningPass::new(device);
        let particle_system = ParticleSystem::new(device, &layouts.camera, 1_000_000, sample_count);

        // -- Antialiasing pass -----------------------------------------------
        let render_2d_config = Render2dConfig::default();
        let mut aa_pass = crate::passes::AntialiasingPass::new(device);
        aa_pass.on_attach(device, hdr_format, context.pipeline_cache.as_deref());
        aa_pass.on_resize(device, width, height);
        // Default policy: no redundant post-AA on top of the SDF analytic AA
        // (audit M4).  Consumers opt back in via `set_antialiasing` or
        // `set_render_2d_config`.
        aa_pass.set_mode(render_2d_config.antialiasing.to_mode());

        let renderer_2d = Renderer2d::new_with_cache(
            context.device.clone(),
            hdr_format,
            Some(wgpu::TextureFormat::Depth32Float),
            sample_count,
            1024,
            context.pipeline_cache.as_deref(),
        );
        // Pure2D renderiza directo al swapchain: usa formato de superficie (no HDR)
        // y sample_count=1 (MSAA no es compatible con presentación directa).
        let renderer_2d_pure = Renderer2d::new_with_cache(
            context.device.clone(),
            format,
            None,
            1,
            1024,
            context.pipeline_cache.as_deref(),
        );
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
            render_2d_config,
            frame_ms: 0.0,
            frame_ms_avg: 0.0,
            frame_count: 0,
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
            renderer_2d_pure,
            views_2d: Vec::new(),
            view_2d_open: false,
            shape_batcher,
            sprite_batcher_2d: ferrous_2d::render::SpriteBatcher::default(),
            draw_list_scratch: Vec::new(),
            sprite_flat_scratch: Vec::new(),
            shape_upload_scratch: Vec::new(),
            sprite_upload_scratch: Vec::new(),
            coalesced_2d_scratch: Vec::new(),
            static_2d_batches: Vec::new(),
            static_2d_free: Vec::new(),
            static_2d_order_scratch: Vec::new(),
            static_2d_runs_scratch: Vec::new(),
            sprite_bind_groups: std::collections::HashMap::new(),
            next_sprite_texture_id: 0,
            freed_sprite_texture_ids: Vec::new(),
            fonts_2d: HashMap::new(),
            next_font_2d_id: 0,
            freed_font_2d_ids: Vec::new(),
            text_batcher_2d: Vec::new(),
            text_layout_scratch: Vec::new(),
            text_upload_scratch: Vec::new(),
            readback_manager: None,
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
        self.aa_pass
            .on_resize(&self.context.device, new_width, new_height);

        #[cfg(feature = "gui")]
        {
            let (target_format, target_samples) =
                if self.mode == RendererMode::Pure2D || self.mode == RendererMode::Flat2D {
                    (self.format, 1)
                } else {
                    (crate::render_target::HdrTexture::FORMAT, self.sample_count)
                };
            self.ui_pass.update_output_config(
                &self.context.queue,
                new_width,
                new_height,
                target_format,
                target_samples,
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
        self.aa_pass
            .on_resize(&self.context.device, new_width, new_height);

        // Headless readback buffer must match the new resolution
        if let Some(ref mut rb) = self.readback_manager {
            if rb.width != new_width || rb.height != new_height {
                *rb = crate::resources::readback::ReadbackFrameManager::new(
                    &self.context.device,
                    new_width,
                    new_height,
                );
            }
        }

        // En Pure2D recalcular la ortho para mantener 1u=1px con el nuevo tamaño.
        if self.mode == RendererMode::Pure2D {
            self.camera_system
                .camera
                .set_mode_2d(true, Some(new_height as f32));
            self.camera_system
                .camera
                .set_aspect(new_width as f32 / new_height as f32);
        }
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

            // Sync UI output config: Pure2D/Flat2D render to swapchain (1 sample, surface format),
            // while Full3D renders to HDR target (MSAA sample_count, hdr format).
            let (target_format, target_samples) =
                if mode == RendererMode::Pure2D || mode == RendererMode::Flat2D {
                    (self.format, 1)
                } else {
                    (crate::render_target::HdrTexture::FORMAT, self.sample_count)
                };
            self.ui_pass.update_output_config(
                &self.context.queue,
                self.width,
                self.height,
                target_format,
                target_samples,
            );
        }
        self.mark_dirty();
    }

    /// Convenience helper to switch to Flat 2D mode with a specific background color.
    pub fn enable_flat_2d(&mut self, background_color: wgpu::Color) {
        self.set_mode(RendererMode::Flat2D);
        self.set_clear_color(background_color);
    }

    /// Activa el modo 2D puro: ortho pixel-perfect, sin pipeline 3D.
    ///
    /// La cámara se configura automáticamente:
    /// - 1 world-unit = 1 pixel.
    /// - Origen (0, 0) en el centro de la pantalla.
    /// - Eje Y positivo hacia arriba.
    /// - Rango X: [-width/2, +width/2], rango Y: [-height/2, +height/2].
    ///
    /// El `background` es el color de clear que se aplica cada frame.
    pub fn enable_pure_2d(&mut self, background: wgpu::Color) {
        self.set_mode(RendererMode::Pure2D);
        // Configurar cámara ortho pixel-perfect (1u = 1px)
        self.camera_system
            .camera
            .set_mode_2d(true, Some(self.height as f32));
        // Recalcular aspect ratio correcto post-set_mode_2d
        self.camera_system
            .camera
            .set_aspect(self.width as f32 / self.height as f32);
        self.set_clear_color(background);
        self.mark_dirty();
    }

    /// Vuelve al modo 3D completo (PBR / CelShaded / FlatShaded).
    ///
    /// Restaura la cámara perspectiva. El `eye` vuelve a `(0, 2, 5)`
    /// y se rehabilita el controlador WASD.
    pub fn enable_full_3d(&mut self) {
        self.set_mode(RendererMode::Full3D);
        self.camera_system.camera.set_mode_2d(false, None);
        self.camera_system
            .set_aspect(self.width as f32 / self.height as f32);
        self.mark_dirty();
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
        self.mark_dirty();
    }

    /// Explicitly sets the viewport rectangle and updates the camera aspect ratio.
    pub fn set_viewport(&mut self, vp: Viewport) {
        self.viewport = vp;
        self.camera_system
            .set_aspect(vp.width as f32 / vp.height as f32);
        self.mark_dirty();
    }

    /// Set the camera projection type (Perspective or Orthographic).
    pub fn set_projection(&mut self, proj: ferrous_core::scene::camera::Projection) {
        self.camera_system.camera.projection = proj;
        self.mark_dirty();
    }

    /// Set the vertical size for the orthographic projection.
    pub fn set_ortho_size(&mut self, size: f32) {
        let aspect = self.width as f32 / self.height as f32;
        let hh = size / 2.0;
        let hw = hh * aspect;
        self.camera_system.camera.projection =
            ferrous_core::scene::camera::Projection::Orthographic {
                left: -hw,
                right: hw,
                top: hh,
                bottom: -hh,
                z_near: -1000.0,
                z_far: 1000.0,
            };
        self.mark_dirty();
    }

    /// Set the background clear color and switch to Solid sky mode.
    pub fn set_background_color(&mut self, color: wgpu::Color) {
        self.set_clear_color(color);
        self.world_pass.sky_mode = SkyMode::Solid(color);
        self.mark_dirty();
    }

    /// Set the global ambient light for the scene.
    pub fn set_ambient_light(&mut self, color: [f32; 3], intensity: f32) {
        crate::renderer_api::set_ambient_light(
            &mut self.camera_system,
            &self.context.queue,
            color,
            intensity,
        );
        self.mark_dirty();
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
        self.render_2d_config.antialiasing = crate::render_config::Antialiasing2d::from_mode(mode);
        self.mark_dirty();
    }

    /// Returns the current runtime render/performance configuration.
    ///
    /// See [`Render2dConfig`].  Copy it, mutate it, and hand it back to
    /// [`Self::set_render_2d_config`] to A/B compare knobs at runtime.
    pub fn render_2d_config(&self) -> Render2dConfig {
        self.render_2d_config
    }

    /// Applies a runtime render/performance configuration.
    ///
    /// All knobs take effect on the next frame without rebuilding pipelines.
    /// The antialiasing selector is mirrored into the internal
    /// [`AntialiasingPass`](crate::passes::AntialiasingPass); the UI bind-group
    /// cache and scratch-reuse flags are forwarded to the UI pass and the two
    /// `Renderer2d` instances.  `cull_2d` / `cull_margin` are read directly by
    /// the Pure2D pass each frame.
    pub fn set_render_2d_config(&mut self, config: Render2dConfig) {
        self.render_2d_config = config;
        self.aa_pass.set_mode(config.antialiasing.to_mode());
        self.renderer_2d.set_scratch_reuse(config.scratch_reuse);
        self.renderer_2d_pure
            .set_scratch_reuse(config.scratch_reuse);
        #[cfg(feature = "gui")]
        self.ui_pass
            .set_bind_group_cache(config.ui_bind_group_cache);
        self.mark_dirty();
    }

    /// Sets the device-pixels-per-logical-pixel ratio used by the 2D shape
    /// shader to resolve a per-instance `min_px` (logical) into world units.
    ///
    /// Call this whenever the render scale / DPR changes. The value is stored
    /// in `Uniform2d::padding.x` for both 2D renderers; the shader clamps it to
    /// `>= 1.0` so the minimum stroke width always covers at least one device
    /// pixel.
    pub fn set_2d_pixel_ratio(&mut self, ratio: f32) {
        self.renderer_2d.set_pixel_ratio(ratio);
        self.renderer_2d_pure.set_pixel_ratio(ratio);
    }

    /// A/B switch that restores the pre-Phase-1 2D line behaviour: 4 px
    /// `smoothstep` AA and the thin-line stabiliser that faded sub-1.8 px
    /// strokes. Used by Koncreta's `?render=legacy` URL flag.
    pub fn set_2d_legacy(&mut self, legacy: bool) {
        self.renderer_2d.set_legacy_2d(legacy);
        self.renderer_2d_pure.set_legacy_2d(legacy);
    }

    /// Current 2D device-pixels-per-logical-pixel ratio.
    pub fn two_d_pixel_ratio(&self) -> f32 {
        self.renderer_2d_pure.pixel_ratio()
    }

    /// Enables / replaces / disables (`None`) the procedural background grid.
    ///
    /// The grid is a single full-screen triangle evaluated from the inverse
    /// view-projection: infinite, constant-cost, no CPU geometry. Widths are
    /// logical pixels. It is drawn only in the Pure2D path, right after Clear.
    pub fn set_2d_grid(&mut self, params: Option<ferrous_2d::render::Grid2dParams>) {
        self.renderer_2d_pure.set_grid(params);
    }

    /// Starts a 2D view segment: subsequent `draw_2d_shapes` calls until
    /// [`Self::end_2d_view`] are drawn with `view_proj`, clipped to `rect_px`
    /// (physical pixels, top-left origin).
    ///
    /// With one segment per frame the normal single-camera path is used; two or
    /// more segments activate the multi-viewport Pure2D path (per-view camera,
    /// scissor, grid and static batches).
    pub fn begin_2d_view(&mut self, rect_px: [f32; 4], view_proj: glam::Mat4) {
        self.push_2d_view(rect_px, view_proj, true);
    }

    /// Starts an **overlay-only** view segment: shapes are clipped to `rect_px`
    /// and use `view_proj`, but the grid and retained static batches are not
    /// redrawn (used for the editor overlay over the primary viewport).
    pub fn begin_2d_overlay_view(&mut self, rect_px: [f32; 4], view_proj: glam::Mat4) {
        self.push_2d_view(rect_px, view_proj, false);
    }

    fn push_2d_view(&mut self, rect_px: [f32; 4], view_proj: glam::Mat4, background: bool) {
        if self.view_2d_open {
            self.end_2d_view();
        }
        let start = self.shape_batcher.instances.len();
        self.views_2d.push(View2dCommand {
            rect: rect_px,
            view_proj,
            shape_start: start,
            shape_end: start,
            background,
        });
        self.view_2d_open = true;
    }

    /// Closes the current 2D view segment.
    pub fn end_2d_view(&mut self) {
        if !self.view_2d_open {
            return;
        }
        let end = self.shape_batcher.instances.len();
        if let Some(v) = self.views_2d.last_mut() {
            v.shape_end = end;
        }
        self.view_2d_open = false;
    }

    /// Registers a shader dash pattern for joined 2D segments.
    ///
    /// `id` is `1..=15` (`0` = continuous). Lengths alternate dash/gap in world
    /// units and are zero-terminated (up to 8 values). The table lives in the
    /// 2D camera uniform, so it applies to both 2D renderers.
    pub fn set_2d_dash_pattern(&mut self, id: u8, lengths: &[f32]) {
        self.renderer_2d.set_dash_pattern(id, lengths);
        self.renderer_2d_pure.set_dash_pattern(id, lengths);
    }

    /// Snapshot of the hardware-independent per-frame 2D counters for the
    /// **live Pure2D path** (`renderer_2d_pure`).
    ///
    /// Counters are reset at the start of every frame and accumulated by the
    /// 2D `prepare`/`render` methods.  Safe and cheap to call every frame; no
    /// allocation.  Returns zeroes before the first 2D frame.
    pub fn frame_2d_stats(&self) -> ferrous_2d::render::Frame2dCounters {
        self.renderer_2d_pure.frame_counters()
    }

    /// Visible world-space rect `(min, max)` for the current Pure2D camera,
    /// expanded by the effective culling margin.
    ///
    /// Computed by inverting the camera's view-projection and unprojecting the
    /// four NDC corners `(-1, -1) … (1, 1)` at `z = 0`.  For the Pure2D
    /// orthographic camera the `xy` of those points is exactly the world rect
    /// the camera can see (independent of the pan/zoom encoding), so it stays
    /// correct for `set_ortho_size`, `set_camera_eye` and camera-target pan.
    ///
    /// The margin is `max(cull_margin, 4 * world_units_per_pixel)`: the caller's
    /// world-space margin plus a 4-pixel floor that keeps the SDF anti-aliasing
    /// band and sub-pixel line widening from popping at the viewport edge.
    fn pure2d_cull_rect(&self) -> (glam::Vec2, glam::Vec2) {
        let vp = self.camera_system.view_proj();
        let inv = vp.inverse();
        let mut min = glam::Vec2::splat(f32::INFINITY);
        let mut max = glam::Vec2::splat(f32::NEG_INFINITY);
        for (x, y) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let p = inv.project_point3(glam::Vec3::new(x, y, 0.0));
            let xy = glam::Vec2::new(p.x, p.y);
            min = min.min(xy);
            max = max.max(xy);
        }
        // `shape_2d.wgsl` / `sprite_2d.wgsl` derive world_units_per_pixel as
        // `2 / (|view_proj[0][0]| * resolution.x)`.
        let axis = vp.x_axis.x.abs();
        let world_pixel_size = if axis > 1e-9 {
            2.0 / (axis * self.width.max(1) as f32)
        } else {
            0.0
        };
        // The canonical quad AABB does not include the shader's AA expansion
        // nor the per-instance minimum line width (`min_px`). Account for both:
        // 4 px covers the 1.5 px AA padding + slack, and MAX_MIN_PX_LOGICAL in
        // logical px (resolved through the same pixel ratio the shader uses)
        // covers the widest line floor.
        let ratio = self.renderer_2d_pure.pixel_ratio().max(1.0);
        let margin = self
            .render_2d_config
            .cull_margin
            .max(world_pixel_size * (4.0 + ferrous_2d::render::types::MAX_MIN_PX_LOGICAL * ratio));
        (
            min - glam::Vec2::splat(margin),
            max + glam::Vec2::splat(margin),
        )
    }

    // ── Retained static 2D batches ───────────────────────────────────────────

    /// Creates a retained static 2D batch from `shapes` and uploads it to GPU
    /// **once**.  Every subsequent frame draws it with a single instanced draw
    /// call and **no** per-frame upload; call [`Self::update_static_2d_batch`]
    /// when the geometry changes.
    ///
    /// Only the SDF shape pipeline is retained here (rect / circle / line /
    /// triangle / polygon_triangle).  Textured sprites stay on the immediate
    /// path.  The buffer is `VERTEX | COPY_DST` and grows to the next power of
    /// two, with a 128-instance minimum.  An empty slice is allowed (the batch
    /// keeps its slot and draws nothing until updated).
    pub fn create_static_2d_batch(
        &mut self,
        shapes: &[ferrous_2d::render::ShapeInstance],
    ) -> Static2dBatchId {
        let batch = Self::build_static_2d_buffer(&self.context.device, &self.context.queue, shapes);
        let index = match self.static_2d_free.pop() {
            Some(i) => i,
            None => {
                self.static_2d_batches.push(Static2dBatchSlot {
                    generation: 0,
                    batch: None,
                });
                (self.static_2d_batches.len() - 1) as u32
            }
        };
        let slot = &mut self.static_2d_batches[index as usize];
        let generation = slot.generation;
        slot.batch = Some(batch);
        Static2dBatchId { index, generation }
    }

    /// Replaces the contents of a retained batch, reusing its GPU buffer when
    /// the new slice fits; otherwise the buffer is recreated with amortized
    /// (next power of two) capacity.
    ///
    /// A stale or unknown `id` is a no-op (see [`Static2dBatchId`]).
    pub fn update_static_2d_batch(
        &mut self,
        id: Static2dBatchId,
        shapes: &[ferrous_2d::render::ShapeInstance],
    ) {
        let Some(slot) = self.static_2d_batches.get_mut(id.index as usize) else {
            return;
        };
        if slot.generation != id.generation {
            return;
        }
        let Some(batch) = slot.batch.as_mut() else {
            return;
        };
        let len = shapes.len() as u32;
        if len > batch.capacity {
            *batch =
                Self::build_static_2d_buffer(&self.context.device, &self.context.queue, shapes);
        } else {
            // Upload only the chunks that changed since the last upload: a
            // local edit (one wall) re-sends a few KB instead of the scene.
            let packed = crate::static_2d::pack_shapes(shapes);
            for (start, end) in crate::static_2d::dirty_ranges(&batch.cpu, &packed) {
                let offset = (start * std::mem::size_of::<ferrous_2d::render::ShapeGpu>()) as u64;
                self.context.queue.write_buffer(
                    &batch.buffer,
                    offset,
                    bytemuck::cast_slice(&packed[start..end]),
                );
            }
            batch.count = len;
            batch.z = crate::static_2d::batch_min_z(shapes);
            batch.chunk_aabbs = crate::static_2d::chunk_aabbs(shapes);
            batch.cpu = packed;
        }
    }

    /// Frees a retained batch's GPU buffer and invalidates its handle.  Unknown
    /// or stale ids are ignored.
    pub fn destroy_static_2d_batch(&mut self, id: Static2dBatchId) {
        let Some(slot) = self.static_2d_batches.get_mut(id.index as usize) else {
            return;
        };
        if slot.generation != id.generation || slot.batch.is_none() {
            return;
        }
        slot.batch = None;
        // Bump the generation so handles issued before destroy can never
        // address the slot once it is reused.
        slot.generation = slot.generation.wrapping_add(1);
        self.static_2d_free.push(id.index);
    }

    /// Number of live retained batches (including empty ones).  Cheap; for
    /// diagnostics.
    pub fn static_2d_batch_count(&self) -> usize {
        self.static_2d_batches
            .iter()
            .filter(|s| s.batch.is_some())
            .count()
    }

    /// Draws all active retained static 2D batches into a **new** render pass
    /// over `view` (`LoadOp::Load`, no depth attachment).
    ///
    /// Normally you do **not** need this: when
    /// [`Render2dConfig::static_batches`] is enabled the Pure2D pass draws the
    /// batches automatically, before the immediate shape/sprite stream.  This
    /// entry point is for consumers that drive their own encoder/compositing;
    /// combining it with the automatic Pure2D path draws the batches twice.
    pub fn draw_static_2d_batches(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Static2D Batches Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        self.emit_static_2d_batches(&mut rpass, None, false);
    }

    /// Emits the active static batches into an already-open render pass,
    /// ordered by representative `z` (then slot index).  Returns the number of
    /// batches drawn; no-op (0) when [`Render2dConfig::static_batches`] is off.
    fn emit_static_2d_batches(
        &mut self,
        rpass: &mut wgpu::RenderPass<'_>,
        bg_view: Option<usize>,
        no_cull: bool,
    ) -> u32 {
        if !self.render_2d_config.static_batches {
            return 0;
        }
        // Non-allocating ordering via the reusable scratch.
        let mut order = std::mem::take(&mut self.static_2d_order_scratch);
        order.clear();
        for (i, slot) in self.static_2d_batches.iter().enumerate() {
            if let Some(b) = &slot.batch {
                if b.count > 0 {
                    order.push(i as u32);
                }
            }
        }
        order.sort_unstable_by(|&a, &b| {
            let za = self.static_2d_batches[a as usize]
                .batch
                .as_ref()
                .map(|x| x.z)
                .unwrap_or(0.0);
            let zb = self.static_2d_batches[b as usize]
                .batch
                .as_ref()
                .map(|x| x.z)
                .unwrap_or(0.0);
            za.partial_cmp(&zb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cmp(&b))
        });
        // Chunk culling: only the runs of chunks that touch the visible rect
        // are drawn, in their original order (painter order is preserved).
        let cull = self.render_2d_config.cull_2d && !no_cull;
        let (rmin, rmax) = if cull {
            self.pure2d_cull_rect()
        } else {
            (
                glam::Vec2::splat(f32::NEG_INFINITY),
                glam::Vec2::splat(f32::INFINITY),
            )
        };
        let mut runs = std::mem::take(&mut self.static_2d_runs_scratch);
        let mut drawn = 0u32;
        let mut culled = 0u32;
        for &idx in &order {
            if let Some(b) = self.static_2d_batches[idx as usize].batch.as_ref() {
                runs.clear();
                let visible = if cull {
                    crate::static_2d::visible_runs(&b.chunk_aabbs, b.count, rmin, rmax, &mut runs)
                } else {
                    runs.push((0, b.count));
                    b.count
                };
                culled += b.count - visible;
                if runs.is_empty() {
                    continue;
                }
                for &(start, count) in &runs {
                    if let Some(i) = bg_view {
                        if let Some(bg) = self.renderer_2d_pure.view_shape_bg(i) {
                            self.renderer_2d_pure
                                .render_static_shapes_range_bg(rpass, bg, &b.buffer, start, count);
                        }
                    } else {
                        self.renderer_2d_pure
                            .render_static_shapes_range(rpass, &b.buffer, start, count);
                    }
                }
                self.renderer_2d_pure.record_static_batch();
                drawn += 1;
            }
        }
        self.renderer_2d_pure.add_culled_instances(culled);
        self.static_2d_runs_scratch = runs;
        self.static_2d_order_scratch = order;
        drawn
    }

    /// Allocates a persistent instance buffer and uploads `shapes` once.
    fn build_static_2d_buffer(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shapes: &[ferrous_2d::render::ShapeInstance],
    ) -> Static2dBatch {
        let len = shapes.len() as u32;
        let capacity = len.next_power_of_two().max(Static2dBatch::MIN_CAPACITY);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Static2D Batch Instance Buffer"),
            size: (capacity as usize * std::mem::size_of::<ferrous_2d::render::ShapeGpu>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cpu = crate::static_2d::pack_shapes(shapes);
        if len > 0 {
            queue.write_buffer(&buffer, 0, bytemuck::cast_slice(&cpu));
        }
        Static2dBatch {
            buffer,
            capacity,
            count: len,
            z: crate::static_2d::batch_min_z(shapes),
            cpu,
            chunk_aabbs: crate::static_2d::chunk_aabbs(shapes),
        }
    }

    /// Wall-clock duration of the last frame in milliseconds (engine runner
    /// only; `0.0` for a manually-driven renderer / headless harness).
    ///
    /// Same-machine relative delta only — never an absolute fps claim.
    pub fn frame_ms(&self) -> f32 {
        self.frame_ms
    }

    /// Rolling (EMA) average of [`Self::frame_ms`] in milliseconds.
    pub fn frame_ms_avg(&self) -> f32 {
        self.frame_ms_avg
    }

    /// Records the wall-clock duration of a frame, updating the rolling
    /// average.  Called by `ferrous_engine`'s runner; cheap and allocation-free.
    pub fn record_frame_time(&mut self, ms: f32) {
        self.frame_ms = ms;
        // EMA with a 60-frame time constant.  `alpha = 1/60`.
        const ALPHA: f32 = 1.0 / 60.0;
        if self.frame_count == 0 {
            self.frame_ms_avg = ms;
        } else {
            self.frame_ms_avg += (ms - self.frame_ms_avg) * ALPHA;
        }
        self.frame_count = self.frame_count.wrapping_add(1);
    }

    /// Human-readable name of the active wgpu backend: `"vulkan"`,
    /// `"webgpu"`, `"gl"`, `"metal"`, `"dx12"` or `"unknown"`.
    ///
    /// Useful for logging which backend a consumer is running on.
    pub fn backend_name(&self) -> &'static str {
        match self.context.backend {
            wgpu::Backend::Vulkan => "vulkan",
            wgpu::Backend::BrowserWebGpu => "webgpu",
            wgpu::Backend::Gl => "gl",
            wgpu::Backend::Metal => "metal",
            wgpu::Backend::Dx12 => "dx12",
            _ => "unknown",
        }
    }

    /// Handles input events, specifically camera control input.
    ///
    /// Delegates input handling to the camera system for orbit controls.
    pub fn handle_input(&mut self, input: &mut ferrous_core::input::InputState, dt: f32) {
        self.camera_system.handle_input(input, dt);
        self.mark_dirty();
    }

    pub fn begin_frame(&self) -> wgpu::CommandEncoder {
        self.context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
        self.mark_dirty();
    }

    /// Creates a GPU mesh from a list of vertices and indices.
    ///
    /// This method automatically calculates the mesh's AABB (bounding box)
    /// and generates tangent vectors for correct normal mapping.
    pub fn create_mesh(&self, name: &str, mut vertices: Vec<Vertex>, indices: Vec<u32>) -> Mesh {
        log::debug!(
            "[Mesh] Creating: {} (V:{}, I:{})",
            name,
            vertices.len(),
            indices.len()
        );

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
            vertex_buffer: crate::resources::buffer::create_vertex(
                &self.context.device,
                &format!("{} VB", name),
                &vertices,
            ),
            index_buffer: crate::resources::buffer::create_index(
                &self.context.device,
                &format!("{} IB", name),
                &indices,
            ),
            index_count: indices.len() as u32,
            vertex_count: vertices.len() as u32,
            index_format: wgpu::IndexFormat::Uint32,
            aabb,
        }
    }

    /// Sets the background sky mode (Solid, Cubemap, or Procedural).
    pub fn set_sky_mode(&mut self, mode: SkyMode) {
        self.world_pass.sky_mode = mode;
        self.mark_dirty();
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
        self.mark_dirty();
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
            let emitters =
                Query::<(&ferrous_core::scene::GlobalTransform, &ParticleEmitter)>::new(&world.ecs);
            let emitters_data: Vec<_> =
                emitters.iter().map(|(_, (t, e))| (*t, e.clone())).collect();

            if let Some((transform, emitter)) = emitters_data.first() {
                if let Some(ps) = &mut self.particle_system {
                    let (_, _, translation) = transform.0.to_scale_rotation_translation();
                    ps.update(
                        &self.context.queue,
                        0.016, // hardcoded for now, should use TimeSystem
                        [translation.x, translation.y, translation.z],
                        if emitter.active {
                            emitter.spawn_rate
                        } else {
                            0.0
                        },
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
                self.camera_system.camera.set_fov_degrees(cam3d.fov_deg);
                self.camera_system
                    .camera
                    .set_near_far(cam3d.near, cam3d.far);
            }
        }

        // 0c. Sync Material ECS components → MaterialDescriptors
        {
            use ferrous_core::scene::world::MaterialComponent;
            let mat_updates: Vec<(
                u64,
                ferrous_core::scene::MaterialHandle,
                ferrous_core::scene::MaterialDescriptor,
            )> = world
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
                    self.material_registry
                        .update_params(&self.context.queue, handle, &desc);
                    self.world_material_descs.insert(ecs_id, desc.clone());
                }
            }
        }

        // 1. Build frustum from current camera (using WGPU-corrected matrices)
        let camera_packet = crate::graph::frame_packet::CameraPacket {
            view_proj: self.camera_system.view_proj(),
            eye: self.camera_system.camera.eye,
        };
        let frustum = Frustum::from_view_proj(&camera_packet.view_proj);

        // 2. ECS query -> populate frame_builder world instanced caches
        {
            let world_pass_ref = &mut self.world_pass;
            let prepass_ref = &mut self.prepass;
            let vp = self.viewport;
            let vp_mat = camera_packet.view_proj;
            self.frame_builder.build_world_commands(
                world,
                &self.context.device,
                &frustum,
                self.camera_system.camera.eye,
                vp_mat,
                vp,
                &mut self.instance_buf,
                &self.instance_layout,
                &mut self.shadow_instance_buf,
                &mut |bg: std::sync::Arc<wgpu::BindGroup>,
                      shadow_bg: std::sync::Arc<wgpu::BindGroup>| {
                    world_pass_ref.set_instance_buffer(bg.clone());
                    world_pass_ref.set_shadow_instance_buffer(shadow_bg);
                    prepass_ref.set_instance_buffer(bg);
                },
                &self.context.queue,
            );
        }
        self.frame_builder.scene_dirty = true;
    }

    /// Renders the scene into an external texture view.
    ///
    /// Available with and without the `gui` feature; without it, `ui_batch`
    /// must be `None`.  The Pure2D branch does not need any GUI resource.
    pub fn render_to_view(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        ui_batch: Option<UiBatch>,
    ) {
        self.render_internal(encoder, RenderDest::View(view), ui_batch);
    }

    /// Renders the scene to its own internal [`RenderTarget`].
    /// Useful for headless mode or off-screen composition.
    pub fn render_to_internal_target(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        ui_batch: Option<UiBatch>,
    ) {
        self.render_internal(encoder, RenderDest::Target, ui_batch);
    }

    fn render_internal(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        dest: RenderDest,
        ui_batch: Option<UiBatch>,
    ) {
        // Frame start: reset the 2D counters so `frame_2d_stats()` reports the
        // counters accumulated by this frame's `prepare_*`/`render_*` calls
        // only.  Cheap (six relaxed atomic stores); done unconditionally so the
        // stats are consistent even when the mode changes between frames.
        self.renderer_2d_pure.reset_frame_counters();

        // Resolve target view. If internal, use a raw pointer to avoid conflicting
        // borrows with &mut self used by passes later.
        let target_view = match dest {
            RenderDest::View(v) => v,
            RenderDest::Target => unsafe {
                &*(self.render_target.color_view() as *const wgpu::TextureView)
            },
        };

        self.camera_system.sync_gpu(&self.context.queue);

        let camera_packet = CameraPacket {
            view_proj: self.camera_system.view_proj(),
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
        self.world_pass
            .set_material_table(&material_table, &self.material_registry);
        self.prepass
            .set_material_table(&material_table, &self.material_registry);
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

        #[cfg(feature = "gui")]
        if let Some(b) = ui_batch {
            packet.insert(b);
        }
        #[cfg(not(feature = "gui"))]
        let _ = ui_batch;

        // ── Pure2D fast path ───────────────────────────────────────────────
        // Pipeline mínima: Clear → Shapes SDF → Sprites → UI → Extra passes.
        // Sin Prepass, SSAO, WorldPass, Gizmos, Particles, PostFX ni tone-mapping.
        // NOTA: camera_system.sync_gpu() ya se llamó arriba, no repetir.
        if self.mode == RendererMode::Pure2D {
            // Subir la cámara al renderer 2D puro
            let vp = self.camera_system.view_proj();
            let resolution = glam::Vec2::new(self.width as f32, self.height as f32);
            self.renderer_2d_pure
                .update_camera(&self.context.queue, vp, resolution);

            // Procedural grid uniform (inverse view-projection + world/px).
            if self.renderer_2d_pure.has_grid() {
                let axis = vp.x_axis.x.abs();
                let world_per_px = if axis > 1e-9 {
                    2.0 / (axis * self.width.max(1) as f32)
                } else {
                    1.0
                };
                self.renderer_2d_pure.prepare_grid(
                    &self.context.queue,
                    vp.inverse(),
                    resolution,
                    world_per_px,
                );
            }

            // ── Pure2D painter-order batcher (audit H3) ─────────────────────
            // Build a painter-order command list **without mutating** the
            // batchers, then reorder the instances into reusable scratch
            // uploads so every (pipeline, texture) run is a contiguous GPU
            // range.  A run is merely a span of consecutive same-key commands
            // — their z may differ — so N distinct-z shapes collapse to one
            // draw call and a texture's sprites collapse to one draw per
            // contiguous span.  Global painter order is preserved exactly.
            //
            // This keeps the path allocation-free once the scratch buffers are
            // warm: `sort_unstable_by` on the total-order `(z, command)` key
            // allocates nothing (the command is a unique tie-break, so
            // stability is not needed), and every `Vec` below is reused across
            // frames when `render_2d_config.scratch_reuse` (default).  With it
            // disabled, fresh `Vec`s reproduce the pre-optimization behaviour
            // for A/B comparison.
            let n_shapes = self.shape_batcher.instances.len();
            let n_sprites = self.sprite_batcher_2d.total_instances();
            let n_text = self.text_batcher_2d.len();
            let total = n_shapes + n_sprites + n_text;
            let scratch_reuse = self.render_2d_config.scratch_reuse;
            // Two or more recorded views switch to the per-view camera path.
            let multi_view = self.views_2d.len() > 1;

            // ── 2D viewport culling rect ────────────────────────────────────
            // Instances whose world AABB is fully outside the visible camera
            // rect (expanded by the AA-safe margin) are dropped *before* the
            // sort and the run walk, so they cost neither sorting nor upload,
            // and surviving same-key runs that were separated by them
            // coalesce.  Gated by `Render2dConfig::cull_2d`.
            // Per-view culling is not applied yet; skip global culling when the
            // frame is split so shapes meant for another view are not dropped
            // using the primary camera.
            let cull = self.render_2d_config.cull_2d && !multi_view;
            let (rmin, rmax) = if cull {
                self.pure2d_cull_rect()
            } else {
                (
                    glam::Vec2::splat(f32::NEG_INFINITY),
                    glam::Vec2::splat(f32::INFINITY),
                )
            };

            let mut draw_list: Vec<(f32, DrawCommand2d)> = if scratch_reuse {
                let mut v = std::mem::take(&mut self.draw_list_scratch);
                v.clear();
                v
            } else {
                Vec::new()
            };
            // Bucket-order flat copy, used to resolve `Sprite(tex, flat_idx)`.
            let mut sprite_flat: Vec<ferrous_2d::render::SpriteInstance> = if scratch_reuse {
                let mut v = std::mem::take(&mut self.sprite_flat_scratch);
                v.clear();
                v.reserve(n_sprites);
                v
            } else {
                Vec::with_capacity(n_sprites)
            };
            let mut shape_upload: Vec<ferrous_2d::render::ShapeGpu> = if scratch_reuse {
                let mut v = std::mem::take(&mut self.shape_upload_scratch);
                v.clear();
                v.reserve(n_shapes);
                v
            } else {
                Vec::with_capacity(n_shapes)
            };
            let mut sprite_upload: Vec<ferrous_2d::render::SpriteInstance> = if scratch_reuse {
                let mut v = std::mem::take(&mut self.sprite_upload_scratch);
                v.clear();
                v.reserve(n_sprites);
                v
            } else {
                Vec::with_capacity(n_sprites)
            };
            let mut text_upload: Vec<TextInstance> = if scratch_reuse {
                let mut v = std::mem::take(&mut self.text_upload_scratch);
                v.clear();
                v.reserve(n_text);
                v
            } else {
                Vec::with_capacity(n_text)
            };
            let mut coalesced: Vec<Coalesced2dDraw> = if scratch_reuse {
                let mut v = std::mem::take(&mut self.coalesced_2d_scratch);
                v.clear();
                v
            } else {
                Vec::new()
            };

            // Fast path: shapes only, already in painter (z) order — the usual
            // CAD case (callers submit in layer order).  No command list and no
            // sort: cull and pack straight into the upload as a single run.
            // In multi-view the shape vector is already grouped per view in
            // submission order; never reorder across view boundaries.
            let shapes_in_order = multi_view
                || (n_sprites == 0
                    && n_text == 0
                    && self.shape_batcher.instances.windows(2).all(|w| {
                        w[1].transform_c3[2].partial_cmp(&w[0].transform_c3[2])
                            != Some(std::cmp::Ordering::Less)
                    }));

            if shapes_in_order {
                for shape in &self.shape_batcher.instances {
                    if cull {
                        let (amin, amax) = shape_world_aabb(shape);
                        if !aabb_intersects(amin, amax, rmin, rmax) {
                            continue;
                        }
                    }
                    shape_upload.push(ferrous_2d::render::ShapeGpu::pack(shape));
                }
                self.renderer_2d_pure
                    .add_culled_instances((n_shapes - shape_upload.len()) as u32);
                if !shape_upload.is_empty() {
                    coalesced.push(Coalesced2dDraw::Shapes {
                        start: 0,
                        count: shape_upload.len() as u32,
                    });
                }
            } else {
                // ── Painter-order batcher (audit H3) ────────────────────────
                // Build a painter-order command list without mutating the
                // batchers, then reorder the instances into the uploads so
                // every (pipeline, texture) run is a contiguous GPU range.
                // `sort_unstable_by` on the total-order `(z, command)` key is
                // deterministic (the command is a unique tie-break) and
                // allocation-free.
                draw_list.reserve(total);
                let mut culled = 0u32;
                for (i, shape) in self.shape_batcher.instances.iter().enumerate() {
                    if cull {
                        let (amin, amax) = shape_world_aabb(shape);
                        if !aabb_intersects(amin, amax, rmin, rmax) {
                            culled += 1;
                            continue;
                        }
                    }
                    draw_list.push((shape.transform_c3[2], DrawCommand2d::Shape(i)));
                }
                for (&tex_id, instances) in &self.sprite_batcher_2d.batches {
                    for inst in instances {
                        sprite_flat.push(*inst);
                        if cull {
                            let (amin, amax) = sprite_world_aabb(inst);
                            if !aabb_intersects(amin, amax, rmin, rmax) {
                                culled += 1;
                                continue;
                            }
                        }
                        draw_list.push((
                            inst.transform_c3[2],
                            DrawCommand2d::Sprite(tex_id, sprite_flat.len() - 1),
                        ));
                    }
                }
                for (i, (_, inst)) in self.text_batcher_2d.iter().enumerate() {
                    if cull {
                        let (amin, amax) = inst.world_aabb();
                        if !aabb_intersects(amin, amax, rmin, rmax) {
                            culled += 1;
                            continue;
                        }
                    }
                    draw_list.push((inst.depth(), DrawCommand2d::Text(i)));
                }
                self.renderer_2d_pure.add_culled_instances(culled);

                draw_list.sort_unstable_by(|a, b| {
                    a.0.partial_cmp(&b.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.1.cmp(&b.1))
                });

                // Split the painter-order stream into maximal same-key runs.
                let mut i = 0usize;
                while i < draw_list.len() {
                    match draw_list[i].1 {
                        DrawCommand2d::Shape(_) => {
                            let run_start = shape_upload.len() as u32;
                            while i < draw_list.len() {
                                match draw_list[i].1 {
                                    DrawCommand2d::Shape(idx) => {
                                        shape_upload.push(ferrous_2d::render::ShapeGpu::pack(
                                            &self.shape_batcher.instances[idx],
                                        ));
                                        i += 1;
                                    }
                                    _ => break,
                                }
                            }
                            coalesced.push(Coalesced2dDraw::Shapes {
                                start: run_start,
                                count: shape_upload.len() as u32 - run_start,
                            });
                        }
                        DrawCommand2d::Sprite(tex_id, _) => {
                            let run_start = sprite_upload.len() as u32;
                            while i < draw_list.len() {
                                match draw_list[i].1 {
                                    DrawCommand2d::Sprite(t, idx) if t == tex_id => {
                                        sprite_upload.push(sprite_flat[idx]);
                                        i += 1;
                                    }
                                    _ => break,
                                }
                            }
                            coalesced.push(Coalesced2dDraw::Sprites {
                                texture_id: tex_id,
                                start: run_start,
                                count: sprite_upload.len() as u32 - run_start,
                            });
                        }
                        DrawCommand2d::Text(first) => {
                            let font = self.text_batcher_2d[first].0;
                            let run_start = text_upload.len() as u32;
                            while i < draw_list.len() {
                                match draw_list[i].1 {
                                    DrawCommand2d::Text(idx)
                                        if self.text_batcher_2d[idx].0 == font =>
                                    {
                                        text_upload.push(self.text_batcher_2d[idx].1);
                                        i += 1;
                                    }
                                    _ => break,
                                }
                            }
                            coalesced.push(Coalesced2dDraw::Text {
                                font,
                                start: run_start,
                                count: text_upload.len() as u32 - run_start,
                            });
                        }
                    }
                }
            }

            // Upload the painter-ordered coalesced buffers (one write_buffer each).
            self.renderer_2d_pure
                .prepare_shape_gpu_slice(&self.context.queue, &shape_upload);
            self.renderer_2d_pure
                .prepare_sprite_slice(&self.context.queue, &sprite_upload);
            self.renderer_2d_pure
                .prepare_text_slice(&self.context.queue, &text_upload);

            // Multi-view: upload one camera + grid uniform per view (before the
            // pass; uniform buffers cannot be written while encoding).
            if multi_view {
                let views = self.views_2d.clone();
                for (i, v) in views.iter().enumerate() {
                    let rw = v.rect[2].max(1.0);
                    let world_per_px = if v.view_proj.x_axis.x.abs() > 1e-9 {
                        2.0 / (v.view_proj.x_axis.x.abs() * rw)
                    } else {
                        1.0
                    };
                    self.renderer_2d_pure.prepare_view(
                        i,
                        &self.context.queue,
                        v.view_proj,
                        v.rect,
                        v.view_proj.inverse(),
                        world_per_px,
                    );
                }
            }

            let shape_upload_len = shape_upload.len();

            // Un único render pass: Clear → Shapes → Sprites
            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Pure2D Main Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            // Clear con el color de fondo configurado
                            load: wgpu::LoadOp::Clear(self.world_pass.clear_color),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    // Sin depth en Pure2D: el Z-ordering es CPU-side vía z_index
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                if multi_view {
                    // Draw each view into its own scissor with its own camera.
                    let views = self.views_2d.clone();
                    for (i, v) in views.iter().enumerate() {
                        let x = v.rect[0].max(0.0) as u32;
                        let y = v.rect[1].max(0.0) as u32;
                        let w = (v.rect[2].max(1.0)) as u32;
                        let h = (v.rect[3].max(1.0)) as u32;
                        rpass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
                        rpass.set_scissor_rect(x, y, w, h);

                        if v.background {
                            if let Some(gb) = self.renderer_2d_pure.view_grid_bg(i) {
                                self.renderer_2d_pure.render_grid_bg(&mut rpass, gb);
                            }
                            self.emit_static_2d_batches(&mut rpass, Some(i), true);
                        }
                        if let Some(bg) = self.renderer_2d_pure.view_shape_bg(i) {
                            let start = v.shape_start.min(shape_upload_len) as u32;
                            let end = v.shape_end.min(shape_upload_len) as u32;
                            self.renderer_2d_pure.render_shapes_range_bg(
                                &mut rpass,
                                bg,
                                start,
                                end.saturating_sub(start),
                            );
                        }
                    }
                    // Sprites are not segmented by view yet; draw them once in
                    // the first view so they still appear.
                    if let Some(bg) = self.renderer_2d_pure.view_sprite_bg(0) {
                        let sprite_bgs = &self.sprite_bind_groups;
                        for draw in &coalesced {
                            if let Coalesced2dDraw::Sprites {
                                texture_id,
                                start,
                                count,
                            } = *draw
                            {
                                let bg_fn = |id: u32| -> Option<&wgpu::BindGroup> {
                                    sprite_bgs.get(&id).map(|b| b.as_ref())
                                };
                                self.renderer_2d_pure.render_sprites_range_bg(
                                    &mut rpass, bg, texture_id, start, count, bg_fn,
                                );
                            }
                        }
                    }
                    // Text is not segmented by view yet either; draw it once in
                    // the first view.
                    if let Some(cam_bg) = self.renderer_2d_pure.view_text_bg(0) {
                        for draw in &coalesced {
                            if let Coalesced2dDraw::Text { font, start, count } = *draw {
                                if let Some(entry) = self.fonts_2d.get(&font) {
                                    self.renderer_2d_pure.render_text_range_bg(
                                        &mut rpass,
                                        cam_bg,
                                        &entry.bind_group,
                                        start,
                                        count,
                                    );
                                }
                            }
                        }
                    }
                } else {
                    // Procedural background grid: one full-screen triangle right
                    // after Clear, before any retained/immediate geometry.
                    self.renderer_2d_pure.render_grid(&mut rpass);

                    // Retained static batches first: one instanced draw call each,
                    // no per-frame upload.  They are drawn before the immediate
                    // stream (the cross-layer z-ordering contract is documented in
                    // `brain/2d_static_batch.md`).  No-op when the config flag is
                    // off or when there are no active batches.
                    self.emit_static_2d_batches(&mut rpass, None, false);

                    // Emit the runs in painter order.  Texture bind groups are
                    // resolved per sprite run.
                    let sprite_bgs = &self.sprite_bind_groups;
                    for draw in &coalesced {
                        match *draw {
                            Coalesced2dDraw::Shapes { start, count } => {
                                self.renderer_2d_pure
                                    .render_shapes_range(&mut rpass, start, count);
                            }
                            Coalesced2dDraw::Sprites {
                                texture_id,
                                start,
                                count,
                            } => {
                                let bg_fn = |id: u32| -> Option<&wgpu::BindGroup> {
                                    sprite_bgs.get(&id).map(|bg| bg.as_ref())
                                };
                                self.renderer_2d_pure.render_sprites_range(
                                    &mut rpass, texture_id, start, count, bg_fn,
                                );
                            }
                            Coalesced2dDraw::Text { font, start, count } => {
                                if let Some(entry) = self.fonts_2d.get(&font) {
                                    self.renderer_2d_pure.render_text_range(
                                        &mut rpass,
                                        &entry.bind_group,
                                        start,
                                        count,
                                    );
                                }
                            }
                        }
                    }
                }
            }

            // Hand the scratch buffers back for reuse next frame.
            if scratch_reuse {
                self.draw_list_scratch = draw_list;
                self.sprite_flat_scratch = sprite_flat;
                self.shape_upload_scratch = shape_upload;
                self.sprite_upload_scratch = sprite_upload;
                self.text_upload_scratch = text_upload;
                self.coalesced_2d_scratch = coalesced;
            }

            // Limpiar el sprite batcher para el siguiente frame
            self.sprite_batcher_2d.clear();

            // UI Pass encima (texto, overlays, debug GUI)
            #[cfg(feature = "gui")]
            {
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
            }

            // Extra passes del usuario
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

        // ── Flat2D fast path ───────────────────────────────────────────────
        if self.mode == RendererMode::Flat2D {
            #[cfg(feature = "gui")]
            {
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
            }
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
            let v = self.camera_system.view_matrix();
            let p = self.camera_system.proj_matrix();
            self.prepass
                .update_camera(&self.context.queue, v, p, self.camera_system.camera.eye);
            self.prepass
                .prepare(&self.context.device, &self.context.queue, &packet);
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
            self.ssao_resources
                .update_params(&self.context.queue, ssao_w, ssao_h, p, p.inverse());

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
                    mag_filter: wgpu::FilterMode::Nearest,
                    min_filter: wgpu::FilterMode::Nearest,
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

        let (scene_view, scene_rt) =
            if let Some(m_view) = &self.world_pass.hdr_texture.multisampled_view {
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
            RenderStyle::CelShaded {
                toon_levels,
                outline_width,
            } => {
                let toon_levels = *toon_levels;
                let outline_width = *outline_width;
                packet.insert(crate::passes::CelFrameData {
                    light: self.current_dir_light,
                    toon_levels,
                    outline_width,
                });
                if outline_width > 0.0 {
                    packet.insert(crate::passes::OutlineFrameData {
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
                        scene_view,
                        scene_rt,
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
                            scene_view,
                            scene_rt,
                            Some(&self.render_target.depth.view),
                            &packet,
                        );
                    }
                }
            }
            RenderStyle::FlatShaded => {
                packet.insert(crate::passes::FlatFrameData {
                    light: self.current_dir_light,
                });
                if let Some(p) = &mut self.flat_pass {
                    p.prepare(&self.context.device, &self.context.queue, &packet);
                    p.execute(
                        &self.context.device,
                        &self.context.queue,
                        encoder,
                        scene_view,
                        scene_rt,
                        Some(&self.render_target.depth.view),
                        &packet,
                    );
                }
            }
            RenderStyle::Pbr => {}
        }

        // -- 6. Gizmo Pass -----------------------------------------------------
        for line in self.debug_lines.drain(..) {
            self.gizmo_system.draw_line(line);
        }

        self.gizmo_system.execute(
            &self.context.device,
            encoder,
            scene_view,
            scene_rt,
            &self.render_target.depth.view,
            &self.camera_system.gpu.bind_group,
        );

        // -- 6a. Technical 2D Pass (Walls, etc.) -------------------------------
        // The 2D overlay is a screen-space pass: it must receive an independent
        // orthographic projection (origin centred, 1 world-unit = 1 pixel, y up),
        // identical to the one `set_mode_2d` produces in Pure2D.  Feeding the 3D
        // perspective view-proj here produces a wrong `world_pixel_size` in
        // shape_2d.wgsl (world_pixel_size = 2 / |view_proj[0][0]| * res.x, only
        // valid for ortho) which breaks the anti-aliasing width.
        // TODO(overlay-camera): support dedicated 2D pan/zoom on the overlay by
        // pre-multiplying a 2D camera transform; for now the projection is fixed.
        self.renderer_2d.update_camera(
            &self.context.queue,
            self.overlay_2d_view_proj(),
            glam::Vec2::new(self.width as f32, self.height as f32),
        );

        // Sync components from the world if we are in a mode that supports it
        // (This is a bridge for newer Path2d components)
        // Note: For now we only sync if we have a source of truth for the world.
        // In the full game loop, this happens during sync_world, but we can do it here too
        // if we want to support immediate-mode style draw_2d_path.

        self.renderer_2d
            .prepare_shapes(&self.context.queue, &self.shape_batcher);

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
            self.renderer_2d
                .render_shapes(&mut rpass, &self.shape_batcher);
        }

        // -- 6b. UI Pass (Hardware MSAA) ---------------------------------------
        // By rendering UI to the MSAA target before resolve, we get perfect edges.
        #[cfg(feature = "gui")]
        {
            self.ui_pass
                .prepare(&self.context.device, &self.context.queue, &packet);
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

        self.aa_pass
            .update_params(&self.context.queue, self.width, self.height);
        self.aa_pass
            .run_aa(&self.context.device, encoder, &self.world_pass.hdr_texture);

        // -- 7. Post-Process (Tone Mapping) ------------------------------------
        // Route through AA output when a mode is active; fall back to raw HDR.
        use crate::passes::AntialiasingMode;
        match self.aa_pass.mode {
            AntialiasingMode::None => {
                self.post_process_pass.render(
                    &self.context.device,
                    encoder,
                    &self.world_pass.hdr_texture,
                    target_view,
                    &self.camera_system.gpu.bind_group,
                );
            }
            _ => {
                // aa_pass.output() borrows self.aa_pass and self.world_pass.hdr_texture.
                // We need to pass the views by raw ptr to avoid conflicting borrows
                // with the encoder.  This is sound because the textures outlive
                // the render pass scope and are never moved during the blit.
                let aa_view = self.aa_pass.output(&self.world_pass.hdr_texture) as *const _;
                let aa_sampler =
                    self.aa_pass.output_sampler(&self.world_pass.hdr_texture) as *const _;
                // SAFETY: views & samplers are GPU-side descriptors stored in
                // self.aa_pass / self.world_pass which cannot be deallocated while
                // self is alive.
                let aa_view = unsafe { &*aa_view };
                let aa_sampler = unsafe { &*aa_sampler };
                self.post_process_pass.render_with_view(
                    &self.context.device,
                    encoder,
                    aa_view,
                    aa_sampler,
                    &self.world_pass.hdr_texture,
                    target_view,
                    &self.camera_system.gpu.bind_group,
                );
            }
        }

        // -- 9. Extra Passes ---------------------------------------------------
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

    pub fn register_texture_linear(
        &mut self,
        w: u32,
        h: u32,
        pixels: &[u8],
    ) -> crate::resources::TextureHandle {
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

    pub fn register_texture(
        &mut self,
        w: u32,
        h: u32,
        pixels: &[u8],
    ) -> crate::resources::TextureHandle {
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

    pub fn create_material(
        &mut self,
        desc: &ferrous_core::scene::MaterialDescriptor,
    ) -> ferrous_core::scene::MaterialHandle {
        crate::renderer_api::create_material(
            &mut self.material_registry,
            &self.context.device,
            &self.context.queue,
            &mut self.world_pass,
            desc,
        )
    }

    pub fn update_material_params(
        &mut self,
        handle: ferrous_core::scene::MaterialHandle,
        desc: &ferrous_core::scene::MaterialDescriptor,
    ) {
        crate::renderer_api::update_material_params(
            &mut self.material_registry,
            &self.context.queue,
            handle,
            desc,
        );
        self.mark_dirty();
    }

    pub fn register_mesh(&mut self, key: &str, mesh: crate::geometry::Mesh) {
        // Unconditional — routes to procedural_mesh_cache (always available).
        crate::renderer_api::register_mesh(&mut self.frame_builder, key, mesh);
        self.mark_dirty();
    }

    pub fn set_clear_color(&mut self, color: wgpu::Color) {
        crate::renderer_api::set_clear_color(
            &mut self.world_pass,
            #[cfg(feature = "gui")]
            &mut self.ui_pass,
            self.mode,
            color,
        );
        self.mark_dirty();
    }

    /// Configure global atmosphere settings (fog and exposure).
    pub fn set_exposure(&mut self, exposure: f32) {
        crate::renderer_api::set_exposure(&mut self.camera_system, &self.context.queue, exposure);
        self.mark_dirty();
    }

    pub fn set_fog(&mut self, color: [f32; 3], density: f32) {
        crate::renderer_api::set_fog(&mut self.camera_system, &self.context.queue, color, density);
        self.mark_dirty();
    }

    pub fn set_ssao_params(&mut self, radius: f32, bias: f32, intensity: f32, power: f32) {
        self.ssao_resources.radius = radius;
        self.ssao_resources.bias = bias;
        self.ssao_resources.intensity = intensity;
        self.ssao_resources.power = power;

        // Sync to GPU immediately so the next frame uses new params
        let proj = self.camera_system.proj_matrix();
        self.ssao_resources.update_params(
            &self.context.queue,
            self.width,
            self.height,
            proj,
            proj.inverse(),
        );
        self.mark_dirty();
    }

    pub fn set_font_atlas(&mut self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) {
        #[cfg(feature = "gui")]
        crate::renderer_api::set_font_atlas(&mut self.ui_pass, view, sampler);
        self.mark_dirty();
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
        self.mark_dirty();
    }

    pub fn add_pass<P: crate::graph::RenderPass + 'static>(&mut self, pass: P) {
        self.extra_passes.push(Box::new(pass));
        self.mark_dirty();
    }

    pub fn camera_mut(&mut self) -> &mut Camera {
        self.mark_dirty();
        &mut self.camera_system.camera
    }

    /// Push a technical 2D shape for rendering this frame.
    pub fn draw_2d_shape(&mut self, instance: ferrous_2d::render::types::ShapeInstance) {
        self.shape_batcher.push_shape(instance);
    }

    /// Push a slice of technical 2D shapes for rendering this frame.
    ///
    /// Equivalent to calling [`Self::draw_2d_shape`] once per element, but
    /// appends with a single `extend_from_slice` instead of N method calls and
    /// N capacity checks. Order and therefore painter order are identical, and
    /// the Pure2D H3 coalescing pass sees the same flat `ShapeBatcher` it
    /// already consumes, so the resulting draw calls/pixels are unchanged.
    ///
    /// This is the fast path for consumers that approximate a filled polygon
    /// with many instances (e.g. a triangle fan or a scan-line fill).
    pub fn draw_2d_shapes(&mut self, shapes: &[ferrous_2d::render::types::ShapeInstance]) {
        self.shape_batcher.instances.extend_from_slice(shapes);
    }

    /// Push a technical 2D path for rendering this frame.
    /// Dibuja un sprite texturizdo en Pure2D mode.
    ///
    /// `texture_id` debe obtenerse de `register_sprite_texture()`.
    /// El sprite se dibujará centrado en la posición del transform.
    ///
    /// # Ejemplo
    /// ```rust,ignore
    /// let tex_id = renderer.register_sprite_texture(64, 64, &rgba_data);
    /// renderer.draw_sprite_2d(tex_id, ferrous_2d::render::SpriteInstance { ... });
    /// ```
    pub fn draw_sprite_2d(
        &mut self,
        texture_id: u32,
        instance: ferrous_2d::render::SpriteInstance,
    ) {
        self.sprite_batcher_2d.push_sprite(texture_id, instance);
    }

    /// Dibuja un path 2D (líneas, polígonos) para este frame.
    pub fn draw_2d_path(&mut self, path: &ferrous_2d::components::Path2d) {
        let mut cursor = ferrous_core::glam::Vec2::ZERO;

        for cmd in &path.commands {
            match cmd {
                ferrous_2d::components::PathCommand::MoveTo(pos) => {
                    cursor = *pos;
                }
                ferrous_2d::components::PathCommand::LineTo(target) => {
                    let start = cursor;
                    let end = *target;
                    let delta = end - start;
                    let length = delta.length();

                    if length > 0.0001 {
                        let center = (start + end) * 0.5;
                        let rotation = delta.y.atan2(delta.x);

                        // For a capsule/line with round caps to hit the exact start/end points,
                        // the total geometry length must be length + width.
                        let stroke_width = path.stroke_width;
                        let geom_length = length + stroke_width;

                        let model = ferrous_core::glam::Mat4::from_scale_rotation_translation(
                            ferrous_core::glam::Vec3::new(geom_length, stroke_width, 1.0),
                            ferrous_core::glam::Quat::from_rotation_z(rotation),
                            ferrous_core::glam::Vec3::new(center.x, center.y, path.z_index),
                        );

                        self.shape_batcher
                            .push_shape(ferrous_2d::render::types::ShapeInstance {
                                transform_c0: model.x_axis.into(),
                                transform_c1: model.y_axis.into(),
                                transform_c2: model.z_axis.into(),
                                transform_c3: model.w_axis.into(),
                                color: path.stroke_color,
                                params: [
                                    stroke_width,
                                    stroke_width * 0.5, // Perfect round caps center at start/end
                                    2.0,                // Slightly smoother AA
                                    1.0,
                                ],
                                ext0: [0.0; 4],
                                ext1: 0.0,
                            });
                    }
                    cursor = end;
                }
            }
        }
    }

    /// Push a fully-assembled SceneData to the renderer for this frame.
    pub fn set_scene(&mut self, scene: &SceneData) {
        if let Some(cam) = &scene.camera {
            self.camera_system.camera.eye = cam.eye;
            self.camera_system.camera.target = cam.target;
            self.camera_system
                .camera
                .set_fov_degrees(cam.fov_y.to_degrees());
            self.camera_system
                .camera
                .set_near_far(cam.z_near, cam.z_far);
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
        self.mark_dirty();
    }

    /// Forces the next frame to completely rebuild from the ECS world.
    pub fn mark_dirty(&mut self) {
        self.frame_builder.scene_dirty = true;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DebugLine {
    pub start: ferrous_core::glam::Vec3,
    pub end: ferrous_core::glam::Vec3,
    pub color: [f32; 3],
}

impl Renderer {
    pub fn draw_line(
        &mut self,
        start: ferrous_core::glam::Vec3,
        end: ferrous_core::glam::Vec3,
        color: Color,
    ) {
        let [r, g, b, _] = color.to_array();
        self.debug_lines.push(DebugLine {
            start,
            end,
            color: [r, g, b],
        });
        self.mark_dirty();
    }

    pub fn viewport_size(&self) -> ferrous_core::glam::Vec2 {
        ferrous_core::glam::Vec2::new(
            self.render_target.width as f32,
            self.render_target.height as f32,
        )
    }

    /// Builds the screen-space view-projection used by the 2D overlay pass in
    /// Full3D.  It mirrors the ortho configured by `Camera::set_mode_2d(true,
    /// Some(height))` + `set_aspect(width/height)` in Pure2D, including the
    /// OpenGL→WGPU depth correction, so the overlay renders with the exact same
    /// pixel mapping as Pure2D (origin centred, 1 world-unit = 1 pixel, y up).
    ///
    /// This is kept independent from `camera_system` on purpose: in Full3D the
    /// camera holds a perspective projection which the 2D shader cannot use to
    /// derive `world_pixel_size`.
    fn overlay_2d_view_proj(&self) -> ferrous_core::glam::Mat4 {
        use ferrous_core::glam::{Mat4, Vec3};

        let hw = self.width as f32 * 0.5;
        let hh = self.height as f32 * 0.5;

        // Same eye/target/up as `set_mode_2d(true, ...)`.
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y);
        let proj = Mat4::orthographic_rh(-hw, hw, -hh, hh, -100.0, 100.0);
        let corrected = crate::resources::camera::OPENGL_TO_WGPU_MATRIX * proj;
        corrected * view
    }

    /// Activa el modo de exportación headless o video instanciando el readback buffer.
    pub fn enable_export_mode(&mut self) {
        self.readback_manager = Some(crate::resources::readback::ReadbackFrameManager::new(
            &self.context.device,
            self.width,
            self.height,
        ));
    }

    /// Registra una textura RGBA8 para uso en sprites 2D (Pure2D mode).
    ///
    /// Devuelve un `texture_id` (u32) que debe asignarse a `Sprite::texture_id`.
    ///
    /// El sampler usa `FilterMode::Nearest` por defecto para pixel-art perfecto.
    /// Para texturas de alta resolución con suavizado, usa `register_sprite_texture_linear`.
    pub fn register_sprite_texture(&mut self, width: u32, height: u32, data: &[u8]) -> u32 {
        self.register_sprite_texture_inner(width, height, data, wgpu::FilterMode::Nearest)
    }

    /// Igual que `register_sprite_texture` pero con filtro bilineal (para sprites HD).
    pub fn register_sprite_texture_linear(&mut self, width: u32, height: u32, data: &[u8]) -> u32 {
        self.register_sprite_texture_inner(width, height, data, wgpu::FilterMode::Linear)
    }

    fn register_sprite_texture_inner(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
        filter: wgpu::FilterMode,
    ) -> u32 {
        use wgpu::util::DeviceExt;

        let texture = self.context.device.create_texture_with_data(
            &self.context.queue,
            &wgpu::TextureDescriptor {
                label: Some("Sprite2D Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            data,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = self
            .context
            .device
            .create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Sprite2D Sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: filter,
                min_filter: filter,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });

        let bind_group = std::sync::Arc::new(self.context.device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("Sprite2D Bind Group"),
                layout: &self.renderer_2d_pure.pipeline.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            },
        ));

        let id = match self.freed_sprite_texture_ids.pop() {
            Some(recycled) => recycled,
            None => {
                let fresh = self.next_sprite_texture_id;
                self.next_sprite_texture_id += 1;
                fresh
            }
        };
        self.sprite_bind_groups.insert(id, bind_group);
        id
    }

    /// Libera una textura de sprite previamente registrada.
    ///
    /// Las entidades `Sprite` que referencien este `texture_id` dejarán de renderizarse.
    ///
    /// Además de borrar el bind group, se elimina la entrada del
    /// `SpriteBatcher` (si la hubiera) y el id vuelve a una free-list que
    /// `register_sprite_texture*` reutiliza, evitando que ids y claves
    /// huérfanas crezcan indefinidamente al recargar planos.
    pub fn free_sprite_texture(&mut self, texture_id: u32) {
        let existed = self.sprite_bind_groups.remove(&texture_id).is_some();
        self.sprite_batcher_2d.remove_texture(texture_id);
        if existed && !self.freed_sprite_texture_ids.contains(&texture_id) {
            self.freed_sprite_texture_ids.push(texture_id);
        }
    }

    /// Bake a font MSDF atlas and register it for Pure2D text.
    ///
    /// The atlas is built once; every [`Self::draw_text_2d`] call with the
    /// returned [`FontId2d`] reuses it.  Free it with [`Self::free_font_2d`].
    /// An empty `charset` falls back to `ferrous_font::charset::latin_western`.
    pub fn register_font_2d<I>(&mut self, bytes: &[u8], charset: I) -> Result<FontId2d, FontError>
    where
        I: IntoIterator<Item = char>,
    {
        let font = ferrous_font::Font::try_load_bytes(
            bytes,
            &self.context.device,
            &self.context.queue,
            charset,
        )?;
        let bind_group = std::sync::Arc::new(
            self.context
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("2D Font Bind Group"),
                    layout: &self
                        .renderer_2d_pure
                        .text_pipeline
                        .texture_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&font.atlas.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&font.atlas.sampler),
                        },
                    ],
                }),
        );
        let id = match self.freed_font_2d_ids.pop() {
            Some(recycled) => recycled,
            None => {
                let fresh = self.next_font_2d_id;
                self.next_font_2d_id += 1;
                fresh
            }
        };
        let font_id = FontId2d(id);
        self.fonts_2d
            .insert(font_id, Font2dEntry { font, bind_group });
        Ok(font_id)
    }

    /// Free a 2D font and recycle its handle.
    ///
    /// Pending glyphs of that font are dropped from the current frame's text
    /// batch; later draws with the stale handle are ignored with a warning.
    pub fn free_font_2d(&mut self, font_id: FontId2d) {
        if self.fonts_2d.remove(&font_id).is_some() {
            self.text_batcher_2d.retain(|(f, _)| *f != font_id);
            if !self.freed_font_2d_ids.contains(&font_id.0) {
                self.freed_font_2d_ids.push(font_id.0);
            }
        }
    }

    /// Number of live 2D fonts.
    pub fn font_count_2d(&self) -> usize {
        self.fonts_2d.len()
    }

    /// Lay out and queue one line of Pure2D text for this frame.
    ///
    /// Each visible glyph becomes one [`TextInstance`] whose model matrix is
    /// `transform × quad placement`, so rotation, non-uniform scale and parent
    /// transforms compose exactly like shapes and sprites.  Depth is
    /// `transform`'s translation `z`, shared by every glyph of the call; glyphs
    /// keep submission order for equal `z`, so alternate fonts do not reorder.
    ///
    /// Layout uses a reusable scratch buffer, so steady-state calls allocate
    /// nothing.
    pub fn draw_text_2d(
        &mut self,
        font_id: FontId2d,
        text: &str,
        transform: glam::Mat4,
        size_px: f32,
        align: TextAlign,
        color: [f32; 4],
    ) {
        let Some(entry) = self.fonts_2d.get(&font_id) else {
            log::warn!(
                "draw_text_2d called with an unknown FontId2d({})",
                font_id.0
            );
            return;
        };

        let mut layout = std::mem::take(&mut self.text_layout_scratch);
        entry
            .font
            .atlas
            .layout_line(text, size_px, align, &mut layout);
        let px_range = entry.font.atlas.px_range;

        for quad in &layout {
            let [left, bottom, right, top] = quad.bounds;
            let width = (right - left).max(0.0);
            let height = (top - bottom).max(0.0);
            let placement = glam::Mat4::from_translation(glam::Vec3::new(left, bottom, 0.0))
                * glam::Mat4::from_scale(glam::Vec3::new(width, height, 1.0));
            let model = transform * placement;
            self.text_batcher_2d
                .push((font_id, TextInstance::new(model, quad.uv, color, px_range)));
        }

        self.text_layout_scratch = layout;
    }

    /// Measure one line of Pure2D text without drawing it.
    ///
    /// Returns the line's total advance, vertical metrics and glyph count.
    /// Measurement allocates a small local buffer; the draw path itself reuses
    /// its scratch buffer instead.
    pub fn measure_text_2d(
        &self,
        font_id: FontId2d,
        text: &str,
        size_px: f32,
        align: TextAlign,
    ) -> Option<ferrous_font::LineMetrics> {
        let entry = self.fonts_2d.get(&font_id)?;
        let mut quads = Vec::new();
        Some(
            entry
                .font
                .atlas
                .layout_line(text, size_px, align, &mut quads),
        )
    }

    /// Clears 2D batchers (shapes, sprites and text). Call this at the start of
    /// every frame to avoid "ghosting" (accumulation of geometry).
    pub fn clear_2d(&mut self) {
        self.shape_batcher.clear();
        self.sprite_batcher_2d.clear();
        self.text_batcher_2d.clear();
        self.views_2d.clear();
        self.view_2d_open = false;
    }
}
