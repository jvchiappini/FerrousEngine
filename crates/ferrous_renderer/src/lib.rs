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
pub mod camera_system;
pub mod context;
pub mod frame_builder;
pub mod geometry;
pub mod graph;
pub mod materials;
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
pub use camera_system::CameraSystem;
pub use ferrous_core::input::{KeyCode, MouseButton};
pub use frame_builder::FrameBuilder;
pub use geometry::{Mesh, Vertex};
pub use graph::frame_packet::Viewport;
pub use graph::{FramePacket, InstancedDrawCommand, RenderPass};
pub use pipeline::InstancingPipeline;
pub use render_stats::RenderStats;
pub use render_target::HdrTexture;
pub use render_target::RenderTarget;
pub use resources::InstanceBuffer;
pub use scene::{Aabb, Frustum, RenderObject};

use materials::MaterialRegistry;
// re-export material types for API consumers
// re-export material primitives from `ferrous_core` directly so that they
// remain public even though the local `materials` module imports them
// privately.  this avoids the privacy errors encountered when the compiler
// treated the earlier exports as private imports.
pub use ferrous_core::scene::{AlphaMode, MaterialDescriptor, MaterialHandle, MATERIAL_DEFAULT};

// texture handles/constants are owned by the texture registry but we expose
// them here so end users can construct `MaterialDescriptor` values without
// digging into submodules.
pub use resources::texture_registry::{
    TextureHandle, TEXTURE_BLACK, TEXTURE_NORMAL, TEXTURE_WHITE,
};

#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;

// -- Internal imports ---------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;

use ferrous_gui::TextBatch;

use camera::controller::OrbitState;
use graph::frame_packet::{CameraPacket, DrawCommand};
use passes::{PostProcessPass, PrePass, SsaoBlurPass, SsaoPass, UiPass, WorldPass};
use pipeline::GizmoPipeline;
use pipeline::{PbrPipeline, PipelineLayouts};
use resources::ModelBuffer;
use resources::SsaoResources;

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
    /// Post-process pass: tone mapping + gamma correction.
    pub post_process_pass: PostProcessPass,
    /// Additional user-supplied passes executed after the built-ins.
    pub extra_passes: Vec<Box<dyn RenderPass>>,

    // -- Camera (Fase 3: delegated to CameraSystem) ---------------------------
    /// All camera state: CPU camera, orbit controller, GPU uniform.
    pub camera_system: CameraSystem,

    // -- Scene (O(1) lookup by id) --------------------------------------------
    /// Legacy manual objects (started at u64::MAX and descending)
    legacy_objects: HashMap<u64, RenderObject>,
    /// World objects mirrored from `ferrous_core::scene::World` (indices match World IDs)
    world_objects: Vec<Option<RenderObject>>,
    /// CPU-side copy of each object�s material descriptor.  Used during
    /// `sync_world` to detect when the core has modified a material and
    /// therefore we need to call `material_registry.update_material_params`.
    world_material_descs: Vec<Option<ferrous_core::scene::MaterialDescriptor>>,
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
    /// A copy of the pipeline bind-group layouts; needed when creating
    /// new materials or other GPU resources that rely on them.

    /// Shared cube mesh � lazily created on first World spawn so that every
    /// cube RenderObject carries the same Arc<Buffer> pointers, enabling
    /// instanced grouping by vertex-buffer pointer in build_base_packet.
    shared_cube_mesh: Option<geometry::Mesh>,
    /// Shared quad mesh, used for all quads irrespective of size.
    shared_quad_mesh: Option<geometry::Mesh>,
    /// Shared sphere mesh plus the lat/long subdivisions used to create it.
    /// Lazily populated on first sphere spawn.
    shared_sphere_mesh: Option<(geometry::Mesh, u32, u32)>,

    /// Cache of meshes that have been registered by asset loaders.
    mesh_cache: std::collections::HashMap<String, geometry::Mesh>,

    /// Pipeline used for drawing gizmos (lines); created once at startup.
    gizmo_pipeline: GizmoPipeline,

    /// Queued gizmos for the current frame.
    gizmo_draws: Vec<scene::GizmoDraw>,

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
        let ui_pass = UiPass::new(ui_renderer);
        let mut post_process_pass = PostProcessPass::new();
        // on_attach builds the bloom pipelines (and the tone-mapping pipeline
        // keyed to the swapchain format); must be called before on_resize.
        post_process_pass.on_attach(device, &context.queue, format, sample_count);

        // Create the shared dynamic model buffer and register it with WorldPass.
        let model_buf = ModelBuffer::new(&context.device, &layouts.model, 64);
        let instance_buf = InstanceBuffer::new(&context.device, &layouts.instance, 64);
        // Separate instance buffer for shadow casters (all objects, not camera-culled).
        let shadow_instance_buf = InstanceBuffer::new(&context.device, &layouts.instance, 64);
        let mut world_pass_init = world_pass;
        world_pass_init.set_model_buffer(model_buf.bind_group.clone(), model_buf.stride);
        world_pass_init.set_instance_buffer(instance_buf.bind_group.clone());
        world_pass_init.set_shadow_instance_buffer(shadow_instance_buf.bind_group.clone());

        // gizmo pipeline writes to the HDR texture so that gizmos composite
        // over scene geometry *before* tone mapping.
        let gizmo_pipeline =
            GizmoPipeline::new(device, hdr_format, rt.sample_count(), layouts.clone());

        // -- SSAO: build passes before the Self literal consumes the buffers --
        let mut prepass = PrePass::new(
            device,
            layouts.model.clone(),
            layouts.instance.clone(),
            width,
            height,
        );
        prepass.set_model_buffer(model_buf.bind_group.clone(), model_buf.stride);
        prepass.set_instance_buffer(instance_buf.bind_group.clone());
        let ssao_pass = SsaoPass::new(device, width, height);
        let ssao_blur_pass = SsaoBlurPass::new(device, width, height);
        let ssao_resources = SsaoResources::new(device, &context.queue);

        Self {
            context,
            render_target: rt,
            world_pass: world_pass_init,
            ui_pass,
            post_process_pass,
            extra_passes: Vec::new(),
            camera_system,
            legacy_objects: HashMap::new(),
            world_objects: Vec::new(),
            world_material_descs: Vec::new(),
            next_manual_id: u64::MAX,
            model_buf,
            next_slot: 0,
            model_layout: layouts.model.clone(),
            instance_buf,
            instance_layout: layouts.instance.clone(),
            shared_cube_mesh: None,
            shared_quad_mesh: None,
            shared_sphere_mesh: None,
            mesh_cache: std::collections::HashMap::new(),
            gizmo_pipeline,
            gizmo_draws: Vec::new(),
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
            render_stats: RenderStats::default(),
            prepass,
            ssao_pass,
            ssao_blur_pass,
            ssao_resources,
            ssao_enabled: true,
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
        handle
    }

    /// Register a mesh under a string key so that world elements can refer to
    /// it later.  This simply inserts the mesh into the renderer's internal
    /// cache; calling `sync_world` will cause any `ElementKind::Mesh`
    /// elements referencing `key` to use the provided geometry.  If a mesh
    /// already existed at that key it is overwritten.
    pub fn register_mesh(&mut self, key: &str, mesh: geometry::Mesh) {
        self.mesh_cache.insert(key.to_string(), mesh);
    }

    /// Remove a previously-registered mesh.  Any world elements still
    /// referring to the key will fall back to the cube primitive when the
    /// next `sync_world` runs.
    pub fn free_mesh(&mut self, key: &str) {
        self.mesh_cache.remove(key);
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
        // delegate to world pass so that the buffer is encapsulated
        self.world_pass.update_light(&self.context.queue, uniform);
    }

    /// Upload an explicit list of point lights to the GPU storage buffer.
    ///
    /// Call this if you manage lights outside of the `World` ECS (e.g. from a
    /// custom system).  For the automatic path see `sync_world`.
    pub fn set_point_lights(&mut self, lights: &[crate::resources::PointLightUniform]) {
        self.world_pass
            .update_point_lights(&self.context.device, &self.context.queue, lights);
    }

    /// Set the material for a previously-spawned legacy object.
    pub fn set_object_material(&mut self, id: u64, material: MaterialHandle) {
        if let Some(obj) = self.legacy_objects.get_mut(&id) {
            obj.material_slot = material.0 as usize;
        }
    }

    /// Set the material for a world object by index (matching the world ID).
    pub fn set_world_object_material(&mut self, index: usize, material: MaterialHandle) {
        // layouts no longer stored; registry keeps its own copy
        if let Some(Some(obj)) = self.world_objects.get_mut(index) {
            obj.material_slot = material.0 as usize;
        }
    }

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
    ///
    /// `double_sided` indicates whether the object should be rendered with
    /// face culling disabled.  `false` is the traditional behaviour.
    /// Spawn a mesh instance at `pos` using the given material.
    ///
    /// Returns a stable u64 handle that may be used with the legacy
    /// `set_object_*` helpers.  `double_sided` controls culling as before.
    pub fn add_object(
        &mut self,
        mesh: Mesh,
        pos: glam::Vec3,
        double_sided: bool,
        material: MaterialHandle,
    ) -> u64 {
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
            self.prepass
                .set_model_buffer(self.model_buf.bind_group.clone(), self.model_buf.stride);
        }
        self.model_buf.write(&self.context.queue, slot, &matrix);

        let obj = RenderObject::new(
            &self.context.device,
            id,
            mesh,
            matrix,
            slot,
            double_sided,
            material.0 as usize,
        );
        self.legacy_objects.insert(id, obj);
        self.frame_builder.scene_dirty = true;
        id
    }

    /// Moves an existing object (GPU write). No-op if id is unknown.
    pub fn set_object_position(&mut self, id: u64, pos: glam::Vec3) {
        if let Some(obj) = self.legacy_objects.get_mut(&id) {
            let matrix = glam::Mat4::from_translation(pos);
            obj.set_matrix(matrix);
            self.model_buf.write(&self.context.queue, obj.slot, &matrix);
            self.frame_builder.scene_dirty = true;
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
            self.frame_builder.scene_dirty = true;
        }
    }

    /// Synchronises a `ferrous_core::scene::World` with the renderer's object map.
    pub fn sync_world(&mut self, world: &ferrous_core::scene::World) {
        let mutated = scene::sync_world(
            world,
            &mut self.world_objects,
            &self.context.device,
            &mut self.shared_cube_mesh,
            &mut self.shared_quad_mesh,
            &mut self.shared_sphere_mesh,
            &mut self.mesh_cache,
        );
        if mutated {
            self.frame_builder.scene_dirty = true;
        }

        // ensure our descriptor cache is large enough
        if self.world_material_descs.len() != world.capacity() {
            self.world_material_descs
                .resize_with(world.capacity(), || None);
        }

        // iterate over every entity and propagate any descriptor changes
        for element in world.iter() {
            let idx = element.id as usize;
            let desc = &element.material.descriptor;
            let needs_update = match &self.world_material_descs[idx] {
                Some(prev) => prev != desc,
                None => true,
            };
            if needs_update {
                // push new parameters to GPU; `create_material` already wrote
                // the initial state so this is safe even if the handle is
                // MATERIAL_DEFAULT.
                self.material_registry.update_params(
                    &self.context.queue,
                    element.material.handle,
                    desc,
                );
                self.world_material_descs[idx] = Some(desc.clone());
                self.frame_builder.scene_dirty = true;
            }
        }

        // -- Collect point lights from World entities ----------------------
        // Gather every element that has a PointLightComponent and convert it
        // into a PointLightUniform using the entity's world-space position.
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
    /// [`execute_gizmo_pass`](Self::execute_gizmo_pass) runs, so there is no
    /// need to manage lifetime manually.
    pub fn queue_gizmo(&mut self, gizmo: scene::GizmoDraw) {
        self.gizmo_draws.push(gizmo);
        // mark scene dirty so that the world pass will rebuild the packet; the
        // gizmos are drawn separately but the packet cache logic should reset
        // when an unrelated draw request arrives.
        self.frame_builder.scene_dirty = true;
    }

    /// Builds vertex data for all queued [`GizmoDraw`] instances and emits a
    /// dedicated line-list render pass on top of the world pass.
    ///
    /// The pass uses `LoadOp::Load` on both the colour and depth attachments so
    /// gizmos composite correctly over the 3-D scene.  Depth writes are enabled
    /// so that gizmos respect scene occlusion.
    ///
    /// After drawing, `gizmo_draws` is cleared ready for the next frame.
    fn execute_gizmo_pass(&mut self, encoder: &mut wgpu::CommandEncoder, _dest: &RenderDest<'_>) {
        // Gizmos compose over the HDR scene (before tone mapping), so we always
        // render into world_pass.hdr_texture � never the swapchain surface.
        let depth_view = self.render_target.depth_view();
        use wgpu::util::DeviceExt;
        use wgpu::{
            LoadOp, Operations, RenderPassColorAttachment, RenderPassDepthStencilAttachment,
            RenderPassDescriptor, StoreOp,
        };

        // build a flat list of vertices; each pair forms one line segment.
        let mut vertices: Vec<Vertex> = Vec::new();
        // helper for gizmo lines: provide a dummy normal and default tangent
        let mut push_line = |pos: [f32; 3], col: [f32; 3]| {
            let mut vert = Vertex::new(pos, [0.0, 0.0, 1.0], [0.0, 0.0]);
            vert.color = col;
            vertices.push(vert);
        };
        for gizmo in &self.gizmo_draws {
            use ferrous_core::scene::{GizmoMode, Plane};
            let st = &gizmo.style;

            // -- Derived sizes from style -----------------------------------
            let arm = st.arm_length;
            let p_off = st.plane_offset();
            let p_size = st.plane_size();
            let arr_len = st.arrow_length();
            let arr_half = st.arrow_half_angle_deg.to_radians();

            let m = gizmo.transform;

            match gizmo.mode {
                GizmoMode::Translate | GizmoMode::Scale => {
                    // -- Axis arms + optional arrowheads ---------------------------
                    for &(axis_vec, axis_enum) in &[
                        (glam::Vec3::X, ferrous_core::scene::Axis::X),
                        (glam::Vec3::Y, ferrous_core::scene::Axis::Y),
                        (glam::Vec3::Z, ferrous_core::scene::Axis::Z),
                    ] {
                        let c = if gizmo.highlighted_axis == Some(axis_enum) {
                            st.axis_highlight(axis_enum)
                        } else {
                            st.axis_color(axis_enum)
                        };

                        let p0 = m.transform_point3(glam::Vec3::ZERO);
                        let p1 = m.transform_point3(axis_vec * arm);

                        // Shaft line
                        push_line(p0.into(), c);
                        push_line(p1.into(), c);

                        // Arrowhead
                        if st.show_arrows && arr_len > 1e-4 {
                            let perp = if axis_vec.dot(glam::Vec3::Y).abs() < 0.9 {
                                axis_vec.cross(glam::Vec3::Y).normalize()
                            } else {
                                axis_vec.cross(glam::Vec3::X).normalize()
                            };
                            let base_local = axis_vec * (arm - arr_len);
                            let up2 = perp;
                            let side = axis_vec.cross(perp).normalize();
                            for &fin_dir in &[up2, -up2, side, -side] {
                                let fin_tip = axis_vec * arm;
                                let fin_base = base_local + fin_dir * (arr_len * arr_half.tan());
                                push_line(m.transform_point3(fin_tip).into(), c);
                                push_line(m.transform_point3(fin_base).into(), c);
                            }
                        }
                    }

                    // -- Plane square outlines -------------------------------------
                    if st.show_planes {
                        for &plane in &[Plane::XY, Plane::XZ, Plane::YZ] {
                            let rgba = if gizmo.highlighted_plane == Some(plane) {
                                st.plane_highlight(plane)
                            } else {
                                st.plane_color(plane)
                            };
                            let c = [rgba[0], rgba[1], rgba[2]];
                            let (a, b) = plane.axes();
                            let c0 = a * p_off + b * p_off;
                            let c1 = a * (p_off + p_size) + b * p_off;
                            let c2 = a * (p_off + p_size) + b * (p_off + p_size);
                            let c3 = a * p_off + b * (p_off + p_size);
                            let corners = [
                                m.transform_point3(c0),
                                m.transform_point3(c1),
                                m.transform_point3(c2),
                                m.transform_point3(c3),
                            ];
                            for i in 0..4 {
                                let j = (i + 1) % 4;
                                push_line(corners[i].into(), c);
                                push_line(corners[j].into(), c);
                            }
                        }
                    }
                }

                GizmoMode::Rotate => {
                    // -- Rotation arc rings � one full circle per axis --------------
                    // Each ring lives in the plane perpendicular to the axis.
                    const ARC_SEGS: usize = 48;
                    let origin = m.transform_point3(glam::Vec3::ZERO);

                    for &(axis_vec, axis_enum) in &[
                        (glam::Vec3::X, ferrous_core::scene::Axis::X),
                        (glam::Vec3::Y, ferrous_core::scene::Axis::Y),
                        (glam::Vec3::Z, ferrous_core::scene::Axis::Z),
                    ] {
                        let c = if gizmo.highlighted_axis == Some(axis_enum) {
                            st.axis_highlight(axis_enum)
                        } else {
                            st.axis_color(axis_enum)
                        };

                        // Two stable perpendiculars in the ring's plane.
                        let perp1 = if axis_vec.dot(glam::Vec3::Y).abs() < 0.9 {
                            axis_vec.cross(glam::Vec3::Y).normalize()
                        } else {
                            axis_vec.cross(glam::Vec3::X).normalize()
                        };
                        let perp2 = axis_vec.cross(perp1).normalize();

                        // Generate ring vertices in world space (m is translation-only).
                        let mut ring: Vec<[f32; 3]> = Vec::with_capacity(ARC_SEGS);
                        for i in 0..ARC_SEGS {
                            let theta = (i as f32 / ARC_SEGS as f32) * std::f32::consts::TAU;
                            let local = (perp1 * theta.cos() + perp2 * theta.sin()) * arm;
                            ring.push((origin + local).into());
                        }

                        // Emit line segments forming the closed ring.
                        for i in 0..ARC_SEGS {
                            let j = (i + 1) % ARC_SEGS;
                            push_line(ring[i], c);
                            push_line(ring[j], c);
                        }
                    }

                    // Small dot (cross) at the pivot origin so users can see it.
                    let dot_size = arm * 0.06;
                    let pivot_c = [1.0_f32, 1.0, 0.4];
                    for &dir in &[
                        glam::Vec3::X,
                        glam::Vec3::NEG_X,
                        glam::Vec3::Y,
                        glam::Vec3::NEG_Y,
                        glam::Vec3::Z,
                        glam::Vec3::NEG_Z,
                    ] {
                        push_line(origin.into(), pivot_c);
                        push_line((origin + dir * dot_size).into(), pivot_c);
                    }
                }
            }
        }

        // upload vertex buffer
        let vb = self
            .context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("gizmo vertex buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        // begin a second render pass that loads existing contents.
        // Gizmos render into the HDR texture (same as the world pass) so they
        // are tone-mapped along with the rest of the scene.
        // We need an owned wgpu::TextureView; recreate it from the underlying
        // texture so that we avoid a split-borrow across self fields.
        let hdr_view_owned = self
            .world_pass
            .hdr_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Gizmo Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &hdr_view_owned,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        rpass.set_pipeline(&self.gizmo_pipeline.inner);
        // bind the camera uniform from the shared GpuCamera, not the layout
        rpass.set_bind_group(0, &*self.camera_system.gpu.bind_group, &[]);
        rpass.set_vertex_buffer(0, vb.slice(..));
        let vertex_count = vertices.len() as u32;
        if vertex_count > 0 {
            rpass.draw(0..vertex_count, 0..1);
        }

        // clear for next frame
        self.gizmo_draws.clear();
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
        self.ui_pass.set_font_atlas(view, sampler);
    }

    // -- Input ----------------------------------------------------------------

    /// Applies keyboard/mouse input to the orbit camera. `dt` is seconds elapsed.
    pub fn handle_input(&mut self, input: &mut ferrous_core::input::InputState, dt: f32) {
        self.camera_system.handle_input(input, dt);
    }

    // -- Private helpers ------------------------------------------------------

    fn do_render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        dest: RenderDest<'_>,
        ui_batch: Option<GuiBatch>,
        text_batch: Option<TextBatch>,
    ) {
        self.camera_system.sync_gpu(&self.context.queue);

        let mut packet = self.build_base_packet();
        if let Some(b) = ui_batch {
            packet.insert(b);
        }
        if let Some(b) = text_batch {
            packet.insert(b);
        }

        // Shared TextureView placeholder: some RenderPass::execute signatures require
        // a color_view even when the pass ignores it (e.g. PrePass, WorldPass writes to
        // its own HDR target).  We create a single dummy view once per frame.
        let dummy_view = self
            .render_target
            .color
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // -- 1. Depth-Normal Prepass (required by SSAO) ------------------------
        // Sync the prepass camera with the current main camera matrices.
        {
            let view = glam::Mat4::look_at_rh(
                self.camera_system.camera.eye,
                self.camera_system.camera.target,
                self.camera_system.camera.up,
            );
            let proj = glam::Mat4::perspective_rh(
                self.camera_system.camera.fovy,
                self.camera_system.camera.aspect,
                self.camera_system.camera.znear,
                self.camera_system.camera.zfar,
            );
            self.prepass.update_camera(
                &self.context.queue,
                view,
                proj,
                self.camera_system.camera.eye,
            );
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
            // Upload camera matrices and screen dimensions to the params buffer.
            let proj = glam::Mat4::perspective_rh(
                self.camera_system.camera.fovy,
                self.camera_system.camera.aspect,
                self.camera_system.camera.znear,
                self.camera_system.camera.zfar,
            );
            let inv_proj = proj.inverse();
            let ssao_w = self.ssao_pass.ssao_texture.width;
            let ssao_h = self.ssao_pass.ssao_texture.height;
            self.ssao_resources
                .update_params(&self.context.queue, ssao_w, ssao_h, proj, inv_proj);

            // Generate raw SSAO
            self.ssao_pass.run(
                &self.context.device,
                encoder,
                &self.ssao_resources,
                &self.prepass.normal_depth,
            );

            // Blur raw SSAO
            self.ssao_blur_pass.run(
                &self.context.device,
                encoder,
                &self.ssao_pass.ssao_texture,
                &self.prepass.normal_depth,
            );

            // Plug the blurred SSAO texture into the environment bind group
            // so the PBR shader samples it this frame.
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

        // -- 3. World pass: renders the 3-D scene into the HDR texture ---------
        // color_view is ignored by WorldPass (it writes to hdr_texture internally),
        // but the trait signature still requires one.
        let depth_view = self.render_target.depth_view();

        self.world_pass
            .prepare(&self.context.device, &self.context.queue, &packet);
        self.world_pass.execute(
            &self.context.device,
            &self.context.queue,
            encoder,
            &dummy_view,
            None,
            Some(depth_view),
            &packet,
        );

        // -- 4. Gizmo pass (if any): also writes into the HDR texture ---------
        if !self.gizmo_draws.is_empty() {
            self.execute_gizmo_pass(encoder, &dest);
        }

        // -- 5. Post-process pass: tone-maps the HDR texture ? final surface ---
        // Build a fresh TextureView for the HDR source (avoids split-borrow).
        let hdr_view_pp = self
            .world_pass
            .hdr_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        // Determine the destination view for the post-process output.
        let owned_pp_target: Option<wgpu::TextureView>;
        let pp_target_view: &wgpu::TextureView = match &dest {
            RenderDest::Target => {
                owned_pp_target = Some(
                    self.render_target
                        .color
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                );
                owned_pp_target.as_ref().unwrap()
            }
            RenderDest::View(v) => {
                owned_pp_target = None;
                let _ = &owned_pp_target; // suppress unused_assignments lint
                v
            }
        };

        // 3a. run bloom chain and obtain the level-0 view containing the
        // accumulated bloom contribution.
        let bloom_view = self.post_process_pass.run_bloom(
            &self.context.device,
            encoder,
            &self.world_pass.hdr_texture,
        );

        {
            let pipeline = self
                .post_process_pass
                .pipeline
                .as_ref()
                .expect("PostProcessPass not initialised � missing on_attach");
            let bgl = self
                .post_process_pass
                .bind_group_layout
                .as_ref()
                .expect("PostProcessPass BGL not initialised");

            use wgpu::{BindGroupDescriptor, BindGroupEntry, BindingResource};
            let bind_group = self.context.device.create_bind_group(&BindGroupDescriptor {
                label: Some("PostProcess BindGroup"),
                layout: bgl,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(&hdr_view_pp),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Sampler(&self.world_pass.hdr_texture.sampler),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: BindingResource::TextureView(bloom_view),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: BindingResource::Sampler(
                            &self
                                .post_process_pass
                                .bloom_textures
                                .as_ref()
                                .unwrap()
                                .sampler,
                        ),
                    },
                ],
            });

            use wgpu::{
                LoadOp, Operations, RenderPassColorAttachment, RenderPassDescriptor, StoreOp,
            };
            let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Post-Process Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: pp_target_view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(wgpu::Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.draw(0..3, 0..1);
        }

        // -- 4. UI pass and extra passes: composite on top of the final surface -
        // These passes use the same swapchain/render-target view as the post-process output.
        let (ui_color_view, ui_resolve_target, ui_depth_view) = match &dest {
            RenderDest::Target => {
                let (cv, rt) = self.render_target.color_views();
                (cv, rt, self.render_target.depth_view())
            }
            RenderDest::View(v) => {
                if self.render_target.sample_count() > 1 {
                    (
                        self.render_target.color.msaa_view.as_ref().unwrap(),
                        Some(*v),
                        self.render_target.depth_view(),
                    )
                } else {
                    (*v, None, self.render_target.depth_view())
                }
            }
        };

        self.ui_pass
            .prepare(&self.context.device, &self.context.queue, &packet);
        self.ui_pass.execute(
            &self.context.device,
            &self.context.queue,
            encoder,
            ui_color_view,
            ui_resolve_target,
            None,
            &packet,
        );

        for pass in &mut self.extra_passes {
            pass.prepare(&self.context.device, &self.context.queue, &packet);
            pass.execute(
                &self.context.device,
                &self.context.queue,
                encoder,
                ui_color_view,
                ui_resolve_target,
                Some(ui_depth_view),
                &packet,
            );
        }

        // Reclaim the Vec<DrawCommand> allocation back into cache for next frame.
        self.reclaim_packet_cache(packet);
    }

    fn build_base_packet(&mut self) -> FramePacket {
        let camera_packet = CameraPacket {
            view_proj: glam::Mat4::from_cols_array_2d(&self.camera_system.gpu.uniform.view_proj),
            eye: self.camera_system.camera.eye,
        };

        // If the scene hasn't mutated and the camera hasn't moved, we can reuse the cached draw commands.
        if !self.frame_builder.scene_dirty
            && self.frame_builder.prev_view_proj == Some(camera_packet.view_proj)
        {
            let mut packet = FramePacket::new(Some(self.viewport), camera_packet);
            std::mem::swap(
                &mut packet.scene_objects,
                &mut self.frame_builder.draw_commands_cache,
            );
            std::mem::swap(
                &mut packet.instanced_objects,
                &mut self.frame_builder.instanced_commands_cache,
            );
            std::mem::swap(
                &mut packet.shadow_scene_objects,
                &mut self.frame_builder.shadow_scene_cache,
            );
            std::mem::swap(
                &mut packet.shadow_instanced_objects,
                &mut self.frame_builder.shadow_instanced_cache,
            );
            return packet;
        }

        self.frame_builder.scene_dirty = false;
        self.frame_builder.prev_view_proj = Some(camera_packet.view_proj);

        self.frame_builder.draw_commands_cache.clear();
        self.frame_builder.instanced_commands_cache.clear();
        self.frame_builder.instance_matrix_scratch.clear();
        self.frame_builder.shadow_scene_cache.clear();
        self.frame_builder.shadow_instanced_cache.clear();
        self.frame_builder.shadow_matrix_scratch.clear();

        let frustum = scene::Frustum::from_view_proj(&camera_packet.view_proj);

        // Shadow casters: legacy objects � include ALL (no camera frustum test).
        // Their matrices are already in the ModelBuffer from spawn/move time.
        for obj in self.legacy_objects.values() {
            self.frame_builder.shadow_scene_cache.push(DrawCommand {
                vertex_buffer: obj.mesh.vertex_buffer.clone(),
                index_buffer: obj.mesh.index_buffer.clone(),
                index_count: obj.mesh.index_count,
                vertex_count: obj.mesh.vertex_count,
                index_format: obj.mesh.index_format,
                model_slot: obj.slot,
                double_sided: obj.double_sided,
                material_slot: obj.material_slot,
                distance_sq: 0.0,
            });
        }

        for obj in self.legacy_objects.values() {
            // Legacy manual object.
            if frustum.intersects_aabb(&obj.world_aabb()) {
                let pos = obj.matrix.w_axis.truncate();
                let diff = pos - camera_packet.eye;
                let dist_sq = diff.length_squared();
                self.frame_builder.draw_commands_cache.push(DrawCommand {
                    vertex_buffer: obj.mesh.vertex_buffer.clone(),
                    index_buffer: obj.mesh.index_buffer.clone(),
                    index_count: obj.mesh.index_count,
                    vertex_count: obj.mesh.vertex_count,
                    index_format: obj.mesh.index_format,
                    model_slot: obj.slot,
                    double_sided: obj.double_sided,
                    material_slot: obj.material_slot,
                    distance_sq: dist_sq,
                });
            }
        }

        // Multi-mesh DOD frustum culling + grouping
        // On desktop we use rayon for parallel culling; on wasm32 we fall back
        // to a sequential iterator since the browser has a single JS thread.
        use std::collections::HashMap;

        #[cfg(not(target_arch = "wasm32"))]
        let visible_mesh_groups: HashMap<
            (usize, usize, bool),
            (geometry::Mesh, usize, Vec<glam::Mat4>),
        > = self
            .world_objects
            .par_iter()
            .flatten()
            .filter(|obj| frustum.intersects_aabb(&obj.world_aabb()))
            .fold(
                || HashMap::new(),
                |mut map: HashMap<
                    (usize, usize, bool),
                    (geometry::Mesh, usize, Vec<glam::Mat4>),
                >,
                 obj| {
                    let key = (
                        Arc::as_ptr(&obj.mesh.vertex_buffer) as usize,
                        obj.material_slot,
                        obj.double_sided,
                    );
                    map.entry(key)
                        .or_insert_with(|| (obj.mesh.clone(), obj.material_slot, Vec::new()))
                        .2
                        .push(obj.matrix);
                    map
                },
            )
            .reduce(
                || HashMap::new(),
                |mut a, b| {
                    for (k, (mesh, mat_slot, mats)) in b {
                        a.entry(k)
                            .or_insert_with(|| (mesh.clone(), mat_slot, Vec::new()))
                            .2
                            .extend(mats);
                    }
                    a
                },
            );

        #[cfg(target_arch = "wasm32")]
        let visible_mesh_groups: HashMap<
            (usize, usize, bool),
            (geometry::Mesh, usize, Vec<glam::Mat4>),
        > = self
            .world_objects
            .iter()
            .flatten()
            .filter(|obj| frustum.intersects_aabb(&obj.world_aabb()))
            .fold(HashMap::new(), |mut map, obj| {
                let key = (
                    Arc::as_ptr(&obj.mesh.vertex_buffer) as usize,
                    obj.material_slot,
                    obj.double_sided,
                );
                map.entry(key)
                    .or_insert_with(|| (obj.mesh.clone(), obj.material_slot, Vec::new()))
                    .2
                    .push(obj.matrix);
                map
            });

        let mut total_visible = 0;
        for (_, (_, _, mats)) in &visible_mesh_groups {
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
            self.frame_builder
                .instance_matrix_scratch
                .reserve(total_visible);

            for ((_key, _mat, double_sided), (mesh, material_slot, mats)) in visible_mesh_groups {
                let count = mats.len() as u32;
                self.frame_builder
                    .instance_matrix_scratch
                    .extend_from_slice(&mats);

                // compute a representative distance for the entire batch; we
                // use the maximum squared distance of any matrix in the group
                // to ensure that the farthest geometry is drawn first.
                let mut max_dist_sq = 0.0;
                for m in &mats {
                    let pos = m.w_axis.truncate();
                    let diff = pos - camera_packet.eye;
                    let d = diff.length_squared();
                    if d > max_dist_sq {
                        max_dist_sq = d;
                    }
                }

                self.frame_builder
                    .instanced_commands_cache
                    .push(InstancedDrawCommand {
                        vertex_buffer: mesh.vertex_buffer.clone(),
                        index_buffer: mesh.index_buffer.clone(),
                        index_count: mesh.index_count,
                        vertex_count: mesh.vertex_count,
                        index_format: mesh.index_format,
                        first_instance: offset,
                        instance_count: count,
                        double_sided,
                        material_slot,
                        distance_sq: max_dist_sq,
                    });

                offset += count;
            }

            self.instance_buf.write_slice(
                &self.context.queue,
                0,
                &self.frame_builder.instance_matrix_scratch,
            );
        }

        // -- Shadow-caster instanced list (World objects, no camera cull) ----
        // Group ALL world objects by mesh (same key as camera path) and write
        // their matrices into the separate shadow_instance_buf.
        {
            use std::collections::HashMap;
            let mut shadow_groups: HashMap<
                (usize, usize, bool),
                (geometry::Mesh, usize, Vec<glam::Mat4>),
            > = HashMap::new();
            for obj in self.world_objects.iter().flatten() {
                let key = (
                    Arc::as_ptr(&obj.mesh.vertex_buffer) as usize,
                    obj.material_slot,
                    obj.double_sided,
                );
                shadow_groups
                    .entry(key)
                    .or_insert_with(|| (obj.mesh.clone(), obj.material_slot, Vec::new()))
                    .2
                    .push(obj.matrix);
            }

            let total_shadow = shadow_groups
                .values()
                .map(|(_, _, m)| m.len())
                .sum::<usize>();
            if total_shadow > 0 {
                let prev_bg = self.shadow_instance_buf.bind_group.clone();
                self.shadow_instance_buf.reserve(
                    &self.context.device,
                    &self.instance_layout,
                    total_shadow,
                );
                if !Arc::ptr_eq(&prev_bg, &self.shadow_instance_buf.bind_group) {
                    self.world_pass
                        .set_shadow_instance_buffer(self.shadow_instance_buf.bind_group.clone());
                }

                let mut offset = 0u32;
                for ((_ptr, _mat, double_sided), (mesh, material_slot, mats)) in shadow_groups {
                    let count = mats.len() as u32;
                    self.frame_builder
                        .shadow_matrix_scratch
                        .extend_from_slice(&mats);
                    self.frame_builder
                        .shadow_instanced_cache
                        .push(InstancedDrawCommand {
                            vertex_buffer: mesh.vertex_buffer.clone(),
                            index_buffer: mesh.index_buffer.clone(),
                            index_count: mesh.index_count,
                            vertex_count: mesh.vertex_count,
                            index_format: mesh.index_format,
                            first_instance: offset,
                            instance_count: count,
                            double_sided,
                            material_slot,
                            distance_sq: 0.0,
                        });
                    offset += count;
                }

                self.shadow_instance_buf.write_slice(
                    &self.context.queue,
                    0,
                    &self.frame_builder.shadow_matrix_scratch,
                );
            }
        }

        // -- Compute render statistics ----------------------------------------
        let mut stats = RenderStats::default();
        for cmd in &self.frame_builder.draw_commands_cache {
            stats.vertex_count += cmd.vertex_count as u64;
            stats.triangle_count += (cmd.index_count / 3) as u64;
            stats.draw_calls += 1;
        }
        for cmd in &self.frame_builder.instanced_commands_cache {
            let inst = cmd.instance_count as u64;
            stats.vertex_count += cmd.vertex_count as u64 * inst;
            stats.triangle_count += (cmd.index_count / 3) as u64 * inst;
            stats.draw_calls += 1;
        }
        self.render_stats = stats;

        let mut packet = FramePacket::new(Some(self.viewport), camera_packet);
        std::mem::swap(
            &mut packet.scene_objects,
            &mut self.frame_builder.draw_commands_cache,
        );
        std::mem::swap(
            &mut packet.instanced_objects,
            &mut self.frame_builder.instanced_commands_cache,
        );
        std::mem::swap(
            &mut packet.shadow_scene_objects,
            &mut self.frame_builder.shadow_scene_cache,
        );
        std::mem::swap(
            &mut packet.shadow_instanced_objects,
            &mut self.frame_builder.shadow_instanced_cache,
        );
        packet
    }

    /// Called at end of `do_render` to reclaim the Vec allocations back into
    /// the caches so they are reused next frame.
    #[inline]
    fn reclaim_packet_cache(&mut self, mut packet: FramePacket) {
        // Swap the (now-empty after execute) Vecs back into the caches.
        std::mem::swap(
            &mut self.frame_builder.draw_commands_cache,
            &mut packet.scene_objects,
        );
        std::mem::swap(
            &mut self.frame_builder.instanced_commands_cache,
            &mut packet.instanced_objects,
        );
        std::mem::swap(
            &mut self.frame_builder.shadow_scene_cache,
            &mut packet.shadow_scene_objects,
        );
        std::mem::swap(
            &mut self.frame_builder.shadow_instanced_cache,
            &mut packet.shadow_instanced_objects,
        );
        // `packet` is dropped here; Arc clones inside are cheap.
    }
}
