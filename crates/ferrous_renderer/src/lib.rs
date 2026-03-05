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

pub use ferrous_gui::{GuiBatch, GuiQuad};
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

// -- Internal imports ---------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;

use ferrous_gui::TextBatch;

use camera::controller::OrbitState;
use graph::frame_packet::{CameraPacket, DrawCommand};
use passes::{PostProcessPass, PrePass, SsaoBlurPass, SsaoPass, UiPass, WorldPass};
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

        // gizmo system: owns the GPU pipeline and the per-frame draw queue.
        let gizmo_system = GizmoSystem::new(device, hdr_format, rt.sample_count(), layouts.clone());

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

        let camera_packet = CameraPacket {
            view_proj: self.camera_system.camera.build_view_projection_matrix(),
            eye: self.camera_system.camera.eye,
        };
        let (mut packet, stats) = {
            let device = &self.context.device;
            let queue = &self.context.queue;
            let viewport = self.viewport;
            let legacy_objects = &self.legacy_objects;
            let world_objects = &self.world_objects;
            let instance_layout = &self.instance_layout;

            let world_pass_ref = &mut self.world_pass;
            let prepass_ref = &mut self.prepass;

            self.frame_builder.build(
                device,
                queue,
                viewport,
                camera_packet,
                legacy_objects,
                world_objects,
                &mut self.instance_buf,
                instance_layout,
                &mut self.shadow_instance_buf,
                &mut |bg, shadow_bg| {
                    world_pass_ref.set_instance_buffer(bg.clone());
                    world_pass_ref.set_shadow_instance_buffer(shadow_bg);
                    prepass_ref.set_instance_buffer(bg);
                },
            )
        };
        self.render_stats = stats;

        if let Some(b) = ui_batch {
            packet.insert(b);
        }
        if let Some(b) = text_batch {
            packet.insert(b);
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

        // -- 3. World Pass (Opaque + Blended) ----------------------------------
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
