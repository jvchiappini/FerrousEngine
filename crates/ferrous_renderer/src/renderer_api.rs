/// Renderer API Methods
///
/// This module contains the public API methods for the Renderer.
/// These methods provide high-level control over rendering operations.

use crate::camera::Camera;
use crate::camera_system::CameraSystem;
use crate::geometry::Mesh;
use crate::gizmo_system::GizmoSystem;
use crate::graph::RenderPass;
use crate::graph::frame_packet::Viewport;
use crate::materials::MaterialRegistry;
use crate::pipeline::PipelineLayouts;
use crate::render_target::RenderTarget;
use crate::resources::InstanceBuffer;
use crate::scene::{SceneData, GizmoDraw};
use ferrous_core::scene::{MaterialDescriptor, MaterialHandle, RenderStyle};
use ferrous_core::input::InputState;
use crate::resources::texture_registry::TextureHandle;
use crate::camera::controller::OrbitState;
use crate::context::EngineContext;
use crate::RendererMode;
use wgpu;

// -- Camera Accessors --------------------------------------------------------

/// Direct reference to the CPU camera state (read-only).
#[inline]
pub fn camera(camera_system: &CameraSystem) -> &Camera {
    &camera_system.camera
}

/// Mutable reference to the CPU camera state.
#[inline]
pub fn camera_mut(camera_system: &mut CameraSystem) -> &mut Camera {
    &mut camera_system.camera
}

/// Mutable reference to the orbit controller.
#[inline]
pub fn orbit_mut(camera_system: &mut CameraSystem) -> &mut OrbitState {
    &mut camera_system.orbit
}

// -- Material Management -----------------------------------------------------

/// Create a material using a full descriptor. Returns a
/// [`MaterialHandle`] that may be stored by the application.
pub fn create_material(
    material_registry: &mut MaterialRegistry,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    world_pass: &mut crate::passes::WorldPass,
    desc: &MaterialDescriptor,
) -> MaterialHandle {
    let handle = material_registry.create(device, queue, desc);
    // sync the world pass table so newly-created slot is available to
    // shaders immediately; forgetting this leads to panics when the
    // pass tries to index past the end of its internal array.
    world_pass.set_material_table(
        &material_registry.bind_group_table(),
        material_registry,
    );
    handle
}

/// Free a material slot so that the corresponding bind group may be
/// reused later. The slot is overwritten with a clone of
/// [`MATERIAL_DEFAULT`], ensuring that any draw commands referencing the
/// old handle will simply render using the default material instead of
/// crashing the GPU.
pub fn free_material(
    material_registry: &mut MaterialRegistry,
    world_pass: &mut crate::passes::WorldPass,
    handle: MaterialHandle,
) {
    material_registry.free(handle);
    world_pass.set_material_table(
        &material_registry.bind_group_table(),
        material_registry,
    );
}

/// Update the scalar parameters of an existing material. Only the
/// uniform buffer is rewritten; texture handles are assumed to remain
/// constant. This is useful for tweaking colour/roughness/etc without
/// reallocating the material.
pub fn update_material_params(
    material_registry: &mut MaterialRegistry,
    queue: &wgpu::Queue,
    handle: MaterialHandle,
    desc: &MaterialDescriptor,
) {
    material_registry.update_params(queue, handle, desc);
}

// -- Mesh Management ---------------------------------------------------------

/// Register a procedural mesh under a string key so that world elements
/// can refer to it later.  Unlike the asset-file cache this path is always
/// compiled in (no feature gate) so WASM / non-asset builds can still use
/// runtime-generated geometry (terrain, sculpted meshes, etc.).
///
/// If a mesh already existed at that key it is overwritten; any
/// `ElementKind::Mesh` entities that reference it will pick up the new
/// geometry on the very next `sync_world` call.
pub fn register_mesh(
    frame_builder: &mut crate::frame_builder::FrameBuilder,
    key: &str,
    mesh: Mesh,
) {
    frame_builder.procedural_mesh_cache.insert(key.to_string(), mesh);
}

/// Remove a previously-registered procedural mesh.
///
/// Any world elements still referring to the key will fall back to the cube
/// primitive when the next `sync_world` runs.
pub fn free_mesh(
    frame_builder: &mut crate::frame_builder::FrameBuilder,
    key: &str,
) {
    frame_builder.procedural_mesh_cache.remove(key);
}

/// Register an asset-file-loaded mesh under a string key.
/// Only available with the `assets` feature.
#[cfg(feature = "assets")]
pub fn register_asset_mesh(
    frame_builder: &mut crate::frame_builder::FrameBuilder,
    key: &str,
    mesh: Mesh,
) {
    frame_builder.mesh_cache.insert(key.to_string(), mesh);
}

/// Remove a previously-registered asset-file mesh.
#[cfg(feature = "assets")]
pub fn free_asset_mesh(
    frame_builder: &mut crate::frame_builder::FrameBuilder,
    key: &str,
) {
    frame_builder.mesh_cache.remove(key);
}

// -- Texture Management ------------------------------------------------------

/// Register a GPU texture from raw RGBA8 bytes and return a
/// [`TextureHandle`]. Use this for color data such as albedo textures.
pub fn register_texture(
    material_registry: &mut MaterialRegistry,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    world_pass: &mut crate::passes::WorldPass,
    width: u32,
    height: u32,
    data: &[u8],
) -> TextureHandle {
    let handle = material_registry.register_texture_rgba8(
        device,
        queue,
        width,
        height,
        data,
    );
    // maintain the old behaviour of refreshing the world-pass table
    // even though the material bind groups are unchanged by adding a
    // standalone texture.  this keeps external code from implicitly
    // depending on the side-effect.
    world_pass.set_material_table(
        &material_registry.bind_group_table(),
        material_registry,
    );
    handle
}

/// Register a GPU texture from raw RGBA8 **linear** bytes and return a
/// [`TextureHandle`]. Use this for non-color data such as normal maps,
/// metallic-roughness and AO textures — the GPU will NOT apply gamma
/// correction when sampling these.
pub fn register_texture_linear(
    material_registry: &mut MaterialRegistry,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    world_pass: &mut crate::passes::WorldPass,
    width: u32,
    height: u32,
    data: &[u8],
) -> TextureHandle {
    let handle = material_registry.register_texture_rgba8_linear(
        device,
        queue,
        width,
        height,
        data,
    );
    world_pass.set_material_table(
        &material_registry.bind_group_table(),
        material_registry,
    );
    handle
}

/// Free a texture previously registered with [`register_texture`]. If
/// the provided handle corresponds to one of the three built-in
/// fallbacks this function is a no-op. Freed slots are recycled by the
/// registry on future registrations.
pub fn free_texture(
    material_registry: &mut MaterialRegistry,
    handle: TextureHandle,
) {
    material_registry.free_texture(handle);
}

/// Overwrite the contents of a texture handle with new RGBA8 data. This
/// is the hot-reload API; it does not allocate a new GPU texture but
/// instead writes directly into the existing one. Materials already
/// pointing at the handle will observe the update automatically.
pub fn update_texture_data(
    material_registry: &mut MaterialRegistry,
    queue: &wgpu::Queue,
    handle: TextureHandle,
    width: u32,
    height: u32,
    data: &[u8],
) {
    material_registry.update_texture_data(
        queue,
        handle,
        width,
        height,
        data,
    );
}

// -- Lighting Methods --------------------------------------------------------

/// Adjust the global directional light used by the PBR shaders.
///
/// The direction should be a normalized vector pointing *from* the light
/// toward the scene. Colour is linear RGB and `intensity` is a scalar
/// multiplier.
pub fn set_directional_light(
    world_pass: &mut crate::passes::WorldPass,
    queue: &wgpu::Queue,
    current_dir_light: &mut crate::resources::DirectionalLightUniform,
    direction: [f32; 3],
    color: [f32; 3],
    intensity: f32,
) {
    // start from the default so we automatically populate
    // `light_view_proj` (it will be recalculated again by
    // `WorldPass::update_light` below).
    let mut uniform = crate::resources::light::DirectionalLightUniform::default();
    uniform.direction = direction;
    uniform.color = color;
    uniform.intensity = intensity;
    // Cache for style-pass frame data injection.
    *current_dir_light = uniform;
    // delegate to world pass so that the buffer is encapsulated
    world_pass.update_light(queue, uniform);
}

/// Upload an explicit list of point lights to the GPU storage buffer.
///
/// Call this if you manage lights outside of the `World` ECS (e.g. from a
/// custom system). For the automatic path see `sync_world`.
pub fn set_point_lights(
    world_pass: &mut crate::passes::WorldPass,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    lights: &[crate::resources::PointLightUniform],
) {
    world_pass.update_point_lights(device, queue, lights);
}

/// Configure global atmosphere settings (fog and exposure).
pub fn set_exposure(camera_system: &mut CameraSystem, queue: &wgpu::Queue, exposure: f32) {
    camera_system.set_exposure(queue, exposure);
}

pub fn set_fog(camera_system: &mut CameraSystem, queue: &wgpu::Queue, color: [f32; 3], density: f32) {
    camera_system.set_fog(queue, color, density);
}

pub fn set_ambient_light(camera_system: &mut CameraSystem, queue: &wgpu::Queue, color: [f32; 3], intensity: f32) {
    camera_system.set_ambient_light(queue, color, intensity);
}

// -- Pass Management ---------------------------------------------------------

/// Appends a custom pass after the built-in ones.
/// `on_attach` is called immediately with the current surface format.
pub fn add_pass<P: RenderPass>(
    extra_passes: &mut Vec<Box<dyn RenderPass>>,
    pass: P,
    context: &EngineContext,
    format: wgpu::TextureFormat,
    sample_count: u32,
) {
    let mut pass = pass;
    pass.on_attach(
        &context.device,
        &context.queue,
        format,
        sample_count,
    );
    extra_passes.push(Box::new(pass));
}

/// Removes all user-supplied passes. Built-in passes are NOT removed.
pub fn clear_extra_passes(extra_passes: &mut Vec<Box<dyn RenderPass>>) {
    extra_passes.clear();
}

// -- Render Style Management -------------------------------------------------

/// Switch to a new render style.
///
/// * `RenderStyle::Pbr` — drops all style passes; the default `WorldPass` is used.
/// * `RenderStyle::CelShaded` — creates `CelShadedPass` + optional `OutlinePass`.
/// * `RenderStyle::FlatShaded` — creates `FlatShadedPass`.
///
/// The method propagates the current instance buffer and material table to
/// newly-created passes immediately so they are ready to draw on the next frame.
pub fn set_render_style(
    render_style: &mut RenderStyle,
    cel_pass: &mut Option<crate::passes::CelShadedPass>,
    outline_pass: &mut Option<crate::passes::OutlinePass>,
    flat_pass: &mut Option<crate::passes::FlatShadedPass>,
    pipeline_layouts: &PipelineLayouts,
    camera_system: &CameraSystem,
    device: &wgpu::Device,
    _format: wgpu::TextureFormat,
    sample_count: u32,
    material_registry: &MaterialRegistry,
    instance_buf: &InstanceBuffer,
) {
    let hdr_format = crate::render_target::HdrTexture::FORMAT;
    match &*render_style {
        RenderStyle::Pbr => {
            *cel_pass = None;
            *outline_pass = None;
            *flat_pass = None;
        }
        RenderStyle::CelShaded {
            toon_levels,
            outline_width,
        } => {
            let toon_levels = *toon_levels;
            let outline_width = *outline_width;

            let mut cp = crate::passes::CelShadedPass::new(
                device,
                pipeline_layouts,
                camera_system.gpu.bind_group.clone(),
                hdr_format,
                sample_count,
                toon_levels,
                outline_width,
            );
            cp.set_instance_buffer(instance_buf.bind_group.clone());
            cp.set_material_table(&material_registry.bind_group_table());
            *cel_pass = Some(cp);

            if outline_width > 0.0 {
                let mut op = crate::passes::OutlinePass::new(
                    device,
                    pipeline_layouts,
                    camera_system.gpu.bind_group.clone(),
                    hdr_format,
                    sample_count,
                    toon_levels,
                    outline_width,
                    [0.0, 0.0, 0.0, 1.0],
                );
                op.set_instance_buffer(instance_buf.bind_group.clone());
                op.set_material_table(&material_registry.bind_group_table());
                *outline_pass = Some(op);
            } else {
                *outline_pass = None;
            }
            *flat_pass = None;
        }
        RenderStyle::FlatShaded => {
            let mut fp = crate::passes::FlatShadedPass::new(
                device,
                pipeline_layouts,
                camera_system.gpu.bind_group.clone(),
                hdr_format,
                sample_count,
            );
            fp.set_instance_buffer(instance_buf.bind_group.clone());
            fp.set_material_table(&material_registry.bind_group_table());
            *flat_pass = Some(fp);
            *cel_pass = None;
            *outline_pass = None;
        }
    }
}

// -- Mode Switching ----------------------------------------------------------

/// Switch between the full 3-D pipeline and the lightweight 2-D/GUI-only
/// pipeline.
///
/// In `Flat2D` mode the world pass, render-style passes, gizmos, and
/// post-process pass are all skipped. The UI pass clears the surface to
/// `world_pass.clear_color` instead of compositing on top of a rendered
/// scene, so the background colour is preserved.
pub fn set_mode(
    mode: &mut RendererMode,
    new_mode: RendererMode,
    world_pass: &mut crate::passes::WorldPass,
    #[cfg(feature = "gui")] ui_pass: &mut crate::passes::UiPass,
) {
    if *mode == new_mode {
        return;
    }
    *mode = new_mode;
    #[cfg(feature = "gui")]
    {
        let clear = if new_mode == RendererMode::Flat2D {
            Some(world_pass.clear_color)
        } else {
            None
        };
        ui_pass.set_clear_color(clear);
    }
}

// -- GPU-Driven Culling ------------------------------------------------------

/// Enables or disables GPU-driven frustum culling.
///
/// When `true`, `sync_world` uploads per-instance cull data to the GPU and
/// `do_render` dispatches the cull compute shader before `WorldPass`.
/// `WorldPass` will then use `draw_indexed_indirect` instead of `draw_indexed`.
///
/// Disabling reverts to the CPU `draw_indexed` path using `instance_buf`.
#[cfg(feature = "gpu-driven")]
pub fn enable_gpu_culling(
    gpu_culling_enabled: &mut bool,
    cull_pass: &mut Option<crate::passes::CullPass>,
    world_pass: &mut crate::passes::WorldPass,
    device: &wgpu::Device,
    pipeline_layouts: &PipelineLayouts,
    enabled: bool,
) {
    *gpu_culling_enabled = enabled;
    if enabled && cull_pass.is_none() {
        *cull_pass = Some(crate::passes::CullPass::new(device, pipeline_layouts));
    }
    if !enabled {
        world_pass.clear_indirect_buffer();
    }
}

/// Returns visible-instance counts per batch from the most recent GPU cull pass.
///
/// Performs a **synchronous** device poll + staging buffer readback.
/// Call this *after* rendering a frame to obtain per-batch culling statistics.
///
/// Returns an empty `Vec` if GPU culling is disabled or no batches were drawn.
#[cfg(feature = "gpu-driven")]
pub fn cull_visible_counts(
    cull_pass: &Option<crate::passes::CullPass>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Vec<u32> {
    if let Some(cp) = cull_pass {
        cp.sync_patch_indirect(device, queue)
    } else {
        vec![]
    }
}

// -- Scene Synchronization ---------------------------------------------------

/// Push a fully-assembled [`SceneData`] to the renderer for this frame.
///
/// This is the **preferred** entry point going forward. The application
/// layer (e.g. `ferrous_app`) queries the ECS, builds a [`SceneData`],
/// and calls this method — keeping the renderer free of ECS knowledge.
///
/// `sync_world` is kept for backward compatibility; it converts ECS state
/// into an equivalent `SceneData` and delegates here.
pub fn set_scene(
    scene: &SceneData,
    camera_system: &mut CameraSystem,
    world_pass: &mut crate::passes::WorldPass,
    current_dir_light: &mut crate::resources::DirectionalLightUniform,
    frame_builder: &mut crate::frame_builder::FrameBuilder,
    queue: &wgpu::Queue,
) {
    // 1. Apply camera if provided
    if let Some(cam) = &scene.camera {
        camera_system.camera.eye = cam.eye;
        camera_system.camera.target = cam.target;
        camera_system.camera.fovy = cam.fov_y;
        camera_system.camera.znear = cam.z_near;
        camera_system.camera.zfar = cam.z_far;
    }

    // 2. Apply directional light if provided
    if let Some(light) = &scene.directional_light {
        set_directional_light(
            world_pass,
            queue,
            current_dir_light,
            light.direction.to_array(),
            light.color.to_array(),
            light.intensity,
        );
    }

    // 3. Mark scene dirty so frame_builder rebuilds next frame.
    //    Instance uploads are still handled by sync_world / build_world_commands
    //    until the full ECS→SceneData migration is complete (Phase 3 step 2).
    if !scene.instances.is_empty() {
        frame_builder.scene_dirty = true;
    }
}

// -- Gizmo Management --------------------------------------------------------

/// Queue a gizmo for rendering this frame.
pub fn queue_gizmo(
    gizmo_system: &mut GizmoSystem,
    gizmo: GizmoDraw,
) {
    gizmo_system.queue(gizmo);
}

// -- Resize / Viewport -------------------------------------------------------

/// Recreates GPU textures when the window changes size.
/// Notifies every pass via `on_resize` -- no downcast needed.
pub fn resize(
    render_target: &mut RenderTarget,
    context: &EngineContext,
    viewport: &mut Viewport,
    camera_system: &mut CameraSystem,
    width: &mut u32,
    height: &mut u32,
    new_width: u32,
    new_height: u32,
    world_pass: &mut crate::passes::WorldPass,
    prepass: &mut crate::passes::PrePass,
    ssao_pass: &mut crate::passes::SsaoPass,
    ssao_blur_pass: &mut crate::passes::SsaoBlurPass,
    post_process_pass: &mut crate::passes::PostProcessPass,
    #[cfg(feature = "gui")] ui_pass: &mut crate::passes::UiPass,
    extra_passes: &mut Vec<Box<dyn RenderPass>>,
) {
    if new_width == *width && new_height == *height {
        return;
    }
    render_target.resize(&context.device, new_width, new_height);

    if viewport.width == *width && viewport.height == *height {
        viewport.width = new_width;
        viewport.height = new_height;
        camera_system.set_aspect(new_width as f32 / new_height as f32);
    }

    *width = new_width;
    *height = new_height;

    // Built-in passes
    world_pass.on_resize(
        &context.device,
        &context.queue,
        new_width,
        new_height,
    );
    // SSAO passes
    prepass.on_resize(
        &context.device,
        &context.queue,
        new_width,
        new_height,
    );
    ssao_pass.on_resize(
        &context.device,
        new_width,
        new_height,
    );
    ssao_blur_pass.on_resize(
        &context.device,
        new_width,
        new_height,
    );
    // Post-process pass
    post_process_pass.on_resize(
        &context.device,
        &context.queue,
        new_width,
        new_height,
    );
    // UI pass
    #[cfg(feature = "gui")]
    ui_pass.on_resize(
        &context.device,
        &context.queue,
        new_width,
        new_height,
    );
    // Extra passes
    for pass in extra_passes.iter_mut() {
        pass.on_resize(
            &context.device,
            &context.queue,
            new_width,
            new_height,
        );
    }
}

/// Sets the viewport used for rendering.
pub fn set_viewport(viewport: &mut Viewport, new_viewport: Viewport) {
    *viewport = new_viewport;
}

// -- Configuration Helpers ----------------------------------------------------

/// Sets the clear color for the render target.
pub fn set_clear_color(
    world_pass: &mut crate::passes::WorldPass,
    #[cfg(feature = "gui")] ui_pass: &mut crate::passes::UiPass,
    mode: crate::RendererMode,
    color: wgpu::Color,
) {
    world_pass.clear_color = color;
    #[cfg(feature = "gui")]
    if mode == crate::RendererMode::Flat2D {
        ui_pass.set_clear_color(Some(color));
    }
}

/// Sets the font atlas texture view and sampler for UI rendering.
#[cfg(feature = "gui")]
pub fn set_font_atlas(
    ui_pass: &mut crate::passes::UiPass,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) {
    ui_pass.set_font_atlas(view, sampler);
}

// -- Input Handling ----------------------------------------------------------

/// Handles input events for camera control and other interactions.
pub fn handle_input(
    camera_system: &mut CameraSystem,
    input: &mut InputState,
    dt: f32,
) {
    // Handle camera input through the camera system
    camera_system.handle_input(input, dt);
}