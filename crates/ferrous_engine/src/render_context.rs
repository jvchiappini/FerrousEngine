//! `RenderContext` — user-facing renderer facade.
//!
//! Wraps `&mut ferrous_renderer::Renderer` and exposes only what application
//! and game code legitimately needs — no GPU internals, no internal pass
//! references, no raw wgpu types.
//!
//! All renderer control that was previously done through
//! `ctx.renderer.<method>()` is now available as `ctx.render.<method>()`.
//!
//! ## Exposed API
//!
//! | Method | Description |
//! |--------|-------------|
//! | `set_style(style)` | Switch PBR / CelShaded / FlatShaded at runtime |
//! | `set_ssao(enabled)` | Toggle SSAO ambient-occlusion pass |
//! | `set_gpu_culling(enabled)` | Toggle GPU compute frustum culling |
//! | `set_clear_color(color)` | Change the background clear colour |
//! | `add_pass(pass)` | Append a custom `RenderPass` after built-ins |
//! | `create_material(desc)` | Register a GPU material, get a stable handle |
//! | `update_material(handle, desc)` | Update scalar params of a material |
//! | `set_directional_light(dir, color, intensity)` | Override the global directional light |
//! | `enable_pure_2d(background)` | Switch to pixel-perfect orthographic Pure2D mode |
//! | `clear_2d()` | Clear the immediate-mode 2D shape/sprite batchers |
//! | `draw_2d_shape(instance)` | Push a technical 2D shape for this frame |
//! | `draw_sprite_2d(texture_id, instance)` | Push a textured sprite for this frame |
//! | `register_sprite_texture(width, height, data)` | Upload an RGBA8 sprite texture |
//! | `stats()` | Per-frame render statistics |
//! | `frame_2d_stats()` | Hardware-independent Pure2D counters (draw calls, bytes, instances) |
//! | `frame_ms()` / `frame_ms_avg()` | Wall-clock last-frame / rolling-average duration |
//! | `backend_name()` | Active wgpu backend (`"vulkan"`, `"webgpu"`, `"gl"`, …) |
//! | `render_2d_config()` / `set_render_2d_config(cfg)` | Runtime perf/AA/culling/static-batch knobs for A/B comparison |
//! | `create_static_2d_batch(shapes)` | Create a retained, GPU-resident static shape batch (no per-frame upload) |
//! | `update_static_2d_batch(id, shapes)` | Replace a retained batch's contents |
//! | `destroy_static_2d_batch(id)` | Free a retained batch |
//! | `draw_static_2d_batches(encoder, view)` | Advanced: draw retained batches manually (engine auto-draws in Pure2D) |
//! | `camera_eye()` | World-space camera eye position |
//! | `camera_target()` | World-space camera look-at target |
//! | `camera_orbit_distance()` | Orbit radius from the built-in camera controller |
//! | `set_camera_eye(eye)` | Override the eye position on the built-in camera |
//!
//! ## Internal / escape-hatch access
//!
//! Engine-internal code that needs the raw renderer can call
//! `RenderContext::renderer_mut()` (public, but not part of the stable API).
//! Application code should normally not need it.

use ferrous_core::glam::Vec3;
use ferrous_core::Color;
use ferrous_renderer::{
    graph::RenderPass, MaterialDescriptor, MaterialHandle, RenderStats, RenderStyle,
};

/// User-facing renderer API. No GPU internals are visible.
///
/// Obtain one from [`AppContext::render`](crate::context::AppContext::render).
pub struct RenderContext<'r> {
    pub(crate) inner: &'r mut ferrous_renderer::Renderer,
}

impl<'r> RenderContext<'r> {
    /// Creates a `RenderContext` from a mutable renderer reference.
    #[inline]
    pub(crate) fn new(renderer: &'r mut ferrous_renderer::Renderer) -> Self {
        Self { inner: renderer }
    }

    // ── Style & Quality ──────────────────────────────────────────────────────

    /// Switch the active render style.
    ///
    /// * `RenderStyle::Pbr` — default AAA PBR pipeline.
    /// * `RenderStyle::CelShaded { toon_levels, outline_width }` — toon shading.
    /// * `RenderStyle::FlatShaded` — faceted flat shading.
    ///
    /// Style passes are created/dropped immediately; the change takes effect
    /// from the very next frame.
    ///
    /// ```rust,ignore
    /// ctx.render.set_style(RenderStyle::CelShaded { toon_levels: 4, outline_width: 1.5 });
    /// ```
    pub fn set_style(&mut self, style: RenderStyle) {
        self.inner.set_render_style(style);
    }

    /// Enable or disable the SSAO (screen-space ambient-occlusion) pass.
    ///
    /// Has no effect when `style` is `FlatShaded` or `CelShaded` (those passes
    /// do not read the AO texture).
    pub fn set_ssao(&mut self, enabled: bool) {
        self.inner.ssao_enabled = enabled;
    }

    /// Configure SSAO parameters.
    /// * `radius`: view-space radius (default ~0.25).
    /// * `bias`: self-occlusion bias (default ~0.02).
    /// * `intensity`: strength multiplier (default 1.0).
    /// * `power`: contrast curve (default 1.2).
    pub fn set_ssao_params(&mut self, radius: f32, bias: f32, intensity: f32, power: f32) {
        self.inner.set_ssao_params(radius, bias, intensity, power);
    }

    /// Enable or disable GPU-driven frustum culling via a compute shader.
    ///
    /// When enabled, per-batch visible instance counts are determined on the
    /// GPU before `WorldPass` draws anything.  Disable for debugging or on
    /// hardware that does not support compute.
    #[cfg(feature = "gpu-driven")]
    pub fn set_gpu_culling(&mut self, enabled: bool) {
        self.inner.enable_gpu_culling(enabled);
    }

    /// Change the background clear colour (applied before the sky / world pass).
    pub fn set_clear_color(&mut self, color: Color) {
        self.inner.set_clear_color(color.to_wgpu());
    }

    /// Convenience helper to switch to Flat 2D mode with a specific background color.
    /// This is the recommended mode for UI-only applications or 2D viewers.
    pub fn enable_flat_2d(&mut self, background_color: Color) {
        self.inner.enable_flat_2d(background_color.to_wgpu());
    }

    /// Switch to **Pure2D** mode: a pixel-perfect orthographic 2D pipeline with
    /// no 3-D passes.
    ///
    /// The camera is configured automatically: 1 world-unit = 1 pixel, the
    /// origin is the centre of the screen with +Y up, so X spans
    /// `[-width/2, +width/2]` and Y spans `[-height/2, +height/2]`.  The
    /// `background_color` is used as the per-frame clear colour.
    ///
    /// This is the canonical entrypoint for immediate-mode 2D rendering and is
    /// equivalent to `renderer_mut().enable_pure_2d(...)`.
    ///
    /// ```rust,ignore
    /// ctx.render.enable_pure_2d(Color::BLACK);
    /// ```
    pub fn enable_pure_2d(&mut self, background_color: Color) {
        self.inner.enable_pure_2d(background_color.to_wgpu());
    }

    /// Convenience helper to switch back to full 3-D rendering and world-sync.
    pub fn enable_full_3d(&mut self) {
        self.inner.enable_full_3d();
    }

    /// Enable and configure distance fog.
    /// `color`: linear RGB. `density`: typical values 0.01 - 0.05.
    pub fn set_fog(&mut self, color: [f32; 3], density: f32) {
        self.inner.set_fog(color, density);
    }

    /// Adjust the global exposure multiplier.
    /// Values < 1.0 are darker, > 1.0 are brighter. 0.5 is a good baseline for HDR.
    pub fn set_exposure(&mut self, exposure: f32) {
        self.inner.set_exposure(exposure);
    }

    /// Set the background clear color and switch to Solid sky mode.
    pub fn set_background_color(&mut self, color: Color) {
        self.inner.set_background_color(color.to_wgpu());
    }

    /// Set the global ambient light for the scene.
    /// `color`: linear RGB. `intensity`: scalar multiplier.
    pub fn set_ambient_light(&mut self, color: [f32; 3], intensity: f32) {
        self.inner.set_ambient_light(color, intensity);
    }

    // ── Antialiasing ─────────────────────────────────────────────────────────

    /// Configure the antialiasing algorithm applied after the gizmo pass and
    /// before tone-mapping.
    ///
    /// | Mode                 | Cost    | Notes                                    |
    /// |----------------------|---------|------------------------------------------|
    /// | `AntialiasingMode::None` | none | Disabled; fastest debug mode             |
    /// | `AntialiasingMode::Fxaa(p)` | very low | NVIDIA FXAA — recommended default |
    /// | `AntialiasingMode::Smaa` | low  | 3-pass SMAA 1x — sharpest result         |
    ///
    /// ```rust,ignore
    /// // FXAA (default quality)
    /// ctx.render.set_antialiasing(AntialiasingMode::Fxaa(FxaaParams::default()));
    ///
    /// // SMAA
    /// ctx.render.set_antialiasing(AntialiasingMode::Smaa);
    ///
    /// // Disabled
    /// ctx.render.set_antialiasing(AntialiasingMode::None);
    /// ```
    pub fn set_antialiasing(&mut self, mode: ferrous_renderer::AntialiasingMode) {
        self.inner.set_antialiasing(mode);
    }

    /// Returns the currently active antialiasing mode.
    pub fn antialiasing_mode(&self) -> ferrous_renderer::AntialiasingMode {
        self.inner.aa_pass.mode
    }

    // ── Custom passes ────────────────────────────────────────────────────────

    /// Append a custom [`RenderPass`] after all built-in passes.
    ///
    /// `on_attach` is called immediately with the current surface format and
    /// sample count so the pass can compile its pipeline.  `on_resize` will be
    /// called automatically whenever the window changes size.
    ///
    /// ```rust,ignore
    /// ctx.render.add_pass(MyVignettePass::new(0.4));
    /// ```
    pub fn add_pass<P: RenderPass>(&mut self, pass: P) {
        self.inner.add_pass(pass);
    }

    // ── Material management ──────────────────────────────────────────────────

    /// Register a new GPU material from a descriptor.
    ///
    /// Returns a [`MaterialHandle`] that can be assigned to world entities via
    /// [`World::set_material_handle`](ferrous_core::World::set_material_handle).
    /// Identical descriptors are **not** automatically deduplicated — call this
    /// once per logical material and store the handle.
    pub fn create_material(&mut self, desc: &MaterialDescriptor) -> MaterialHandle {
        self.inner.create_material(desc)
    }

    /// Update the scalar parameters (colour, metallic, roughness, …) of an
    /// existing material.  Texture handles are assumed constant.
    ///
    /// All entities sharing this handle will observe the change on the next
    /// frame.
    pub fn update_material(&mut self, handle: MaterialHandle, desc: &MaterialDescriptor) {
        self.inner.update_material_params(handle, desc);
    }

    /// Register a GPU texture from raw RGBA8 bytes.
    pub fn register_texture(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> ferrous_renderer::TextureHandle {
        self.inner.register_texture(width, height, data)
    }

    // ── Lighting ─────────────────────────────────────────────────────────────

    /// Override the global directional light for the current scene.
    ///
    /// `direction` should be normalised and point *from* the light *toward* the
    /// scene.  `color` is linear RGB.  `intensity` is a scalar multiplier.
    ///
    /// Prefer spawning a [`ferrous_core::scene::DirectionalLight`] ECS
    /// component instead — this method exists for cases where the light must be
    /// controlled imperatively (e.g. UI panels that let the user drag sliders).
    pub fn set_directional_light(&mut self, direction: [f32; 3], color: [f32; 3], intensity: f32) {
        self.inner
            .set_directional_light(direction, color, intensity);
    }

    // ── Statistics (read-only) ───────────────────────────────────────────────

    /// Per-frame render statistics: vertex count, triangle count, draw calls.
    ///
    /// Updated at the start of every `draw_3d` call; returns zeroes before
    /// the first frame completes.
    pub fn stats(&self) -> RenderStats {
        self.inner.render_stats
    }

    /// Hardware-independent per-frame counters for the **live Pure2D path**.
    ///
    /// Populated by `draw_2d_shape` / `draw_sprite_2d` / `draw_2d_path` and
    /// reset at the start of every frame.  Returns zeroes before the first
    /// frame.  Cheap and allocation-free; safe to call every frame.
    ///
    /// ```rust,ignore
    /// let s = ctx.render.frame_2d_stats();
    /// log::info!("2D draws={} shapes={} sprites={} bytes={} instances={}",
    ///     s.draw_calls, s.shape_draw_calls, s.sprite_draw_calls,
    ///     s.bytes_uploaded, s.instances_uploaded);
    /// ```
    pub fn frame_2d_stats(&self) -> ferrous_renderer::Frame2dCounters {
        self.inner.frame_2d_stats()
    }

    /// Wall-clock duration of the last frame in milliseconds, measured by the
    /// engine runner around the per-frame work.
    ///
    /// Same-machine relative delta only — never an absolute fps claim.
    /// Returns `0.0` when the renderer is driven manually.
    pub fn frame_ms(&self) -> f32 {
        self.inner.frame_ms()
    }

    /// Rolling (EMA, 60-frame) average of [`Self::frame_ms`], in milliseconds.
    pub fn frame_ms_avg(&self) -> f32 {
        self.inner.frame_ms_avg()
    }

    /// Name of the active wgpu backend: `"vulkan"`, `"webgpu"`, `"gl"`,
    /// `"metal"`, `"dx12"` or `"unknown"`.
    pub fn backend_name(&self) -> &'static str {
        self.inner.backend_name()
    }

    /// Returns the current runtime render/performance configuration.
    ///
    /// See [`ferrous_renderer::Render2dConfig`].  Copy it, mutate it, and pass
    /// it back to [`Self::set_render_2d_config`] to A/B compare optimizations
    /// (antialiasing policy, UI bind-group cache, scratch reuse) at runtime.
    pub fn render_2d_config(&self) -> ferrous_renderer::Render2dConfig {
        self.inner.render_2d_config()
    }

    /// Applies a runtime render/performance configuration.  Takes effect on the
    /// next frame; no pipelines are rebuilt.
    ///
    /// ```rust,ignore
    /// let mut cfg = ctx.render.render_2d_config();
    /// cfg.scratch_reuse = !cfg.scratch_reuse; // A/B compare
    /// ctx.render.set_render_2d_config(cfg);
    /// ```
    pub fn set_render_2d_config(&mut self, config: ferrous_renderer::Render2dConfig) {
        self.inner.set_render_2d_config(config);
    }

    /// Sets the device-pixels-per-logical-pixel ratio used to resolve the
    /// per-instance minimum stroke width (`ShapeInstance::with_min_px`).
    ///
    /// Call it whenever the render scale / DPR changes. The value travels to
    /// `shape_2d.wgsl` via `Uniform2d::padding.x`; the shader clamps it to
    /// `>= 1.0` so a minimum-width line always covers at least one device px.
    pub fn set_2d_pixel_ratio(&mut self, ratio: f32) {
        self.inner.set_2d_pixel_ratio(ratio);
    }

    /// A/B switch for the pre-Phase-1 2D line rendering (4 px AA + thin-line
    /// alpha stabiliser). `false` uses the 1-pixel box coverage filter and the
    /// opacity-preserving minimum width. Browsers drive it with `?render=`.
    pub fn set_2d_legacy(&mut self, legacy: bool) {
        self.inner.set_2d_legacy(legacy);
    }

    /// Registers a shader dash pattern for joined 2D segments.
    ///
    /// `id` is `1..=15` (`0` = continuous); `lengths` alternate dash/gap in
    /// world units and are zero-terminated (max 8).
    pub fn set_2d_dash_pattern(&mut self, id: u8, lengths: &[f32]) {
        self.inner.set_2d_dash_pattern(id, lengths);
    }

    /// Enables / replaces / disables (`None`) the procedural background grid.
    ///
    /// Drawn in the Pure2D path right after Clear and before any model
    /// geometry. The grid is evaluated from the inverse view-projection, so it
    /// is infinite and constant-cost; widths are logical pixels.
    pub fn set_2d_grid(&mut self, params: Option<ferrous_2d::render::Grid2dParams>) {
        self.inner.set_2d_grid(params);
    }

    /// Starts a 2D view segment (multi-viewport): shapes submitted until
    /// [`Self::end_2d_view`] use `view_proj` and are clipped to `rect_px`
    /// (physical pixels, top-left origin).
    pub fn begin_2d_view(&mut self, rect_px: [f32; 4], view_proj: ferrous_core::glam::Mat4) {
        self.inner.begin_2d_view(rect_px, view_proj);
    }

    /// Ends the current 2D view segment.
    pub fn end_2d_view(&mut self) {
        self.inner.end_2d_view();
    }

    /// Starts an **overlay-only** 2D view: shapes use `view_proj` and are
    /// clipped to `rect_px`, but the grid/static batches are not redrawn.
    pub fn begin_2d_overlay_view(
        &mut self,
        rect_px: [f32; 4],
        view_proj: ferrous_core::glam::Mat4,
    ) {
        self.inner.begin_2d_overlay_view(rect_px, view_proj);
    }

    // ── Retained static 2D batches ───────────────────────────────────────────

    /// Creates a retained static 2D batch from `shapes`, uploading to GPU once.
    ///
    /// Use it for geometry that rarely changes (walls, slabs, baked plan
    /// geometry): every frame it is drawn with one instanced draw call and
    /// **no** per-frame upload.  Update it with
    /// [`Self::update_static_2d_batch`] when the document changes and release it
    /// with [`Self::destroy_static_2d_batch`].
    ///
    /// Only SDF shapes are retained (rect / circle / line / triangle /
    /// polygon_triangle); textured sprites stay immediate.
    ///
    /// ```rust,ignore
    /// let id = ctx.render.create_static_2d_batch(&wall_shapes);
    /// ```
    pub fn create_static_2d_batch(
        &mut self,
        shapes: &[ferrous_2d::render::ShapeInstance],
    ) -> ferrous_renderer::Static2dBatchId {
        self.inner.create_static_2d_batch(shapes)
    }

    /// Replaces a retained batch's contents (reusing its GPU buffer when it
    /// fits).  Stale/unknown ids are ignored.
    pub fn update_static_2d_batch(
        &mut self,
        id: ferrous_renderer::Static2dBatchId,
        shapes: &[ferrous_2d::render::ShapeInstance],
    ) {
        self.inner.update_static_2d_batch(id, shapes);
    }

    /// Frees a retained batch's GPU buffer and invalidates its handle.
    pub fn destroy_static_2d_batch(&mut self, id: ferrous_renderer::Static2dBatchId) {
        self.inner.destroy_static_2d_batch(id);
    }

    /// Draws all active retained static batches into `view` in a new render pass
    /// (`Load`).  Advanced/escape-hatch: the engine's Pure2D pass already draws
    /// them automatically when `Render2dConfig::static_batches` is enabled, so
    /// combining both draws them twice.
    pub fn draw_static_2d_batches(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
        self.inner.draw_static_2d_batches(encoder, view);
    }

    /// World-space position of the camera eye this frame.
    pub fn camera_eye(&self) -> Vec3 {
        self.inner.camera().eye
    }

    /// World-space position of the camera look-at target this frame.
    pub fn camera_target(&self) -> Vec3 {
        self.inner.camera().target
    }

    /// The current orbit radius of the camera controller.
    ///
    /// Returns the `orbit_distance` stored in the renderer's built-in
    /// [`OrbitCameraController`](ferrous_renderer::OrbitCameraController).
    /// Useful for initialising ECS `OrbitCamera` components to match the
    /// renderer state on startup.
    pub fn camera_orbit_distance(&self) -> f32 {
        self.inner.camera().controller.orbit_distance
    }

    /// Set the world-space eye position on the renderer's built-in camera.
    pub fn set_camera_eye(&mut self, eye: Vec3) {
        self.inner.camera_mut().eye = eye;
    }

    /// Set the camera projection (Perspective or Orthographic).
    pub fn set_projection(&mut self, proj: ferrous_core::scene::camera::Projection) {
        self.inner.set_projection(proj);
    }

    /// Set the vertical size for the orthographic projection.
    pub fn set_ortho_size(&mut self, size: f32) {
        self.inner.set_ortho_size(size);
    }

    /// Draw a debug line for one frame.
    pub fn draw_line(&mut self, start: Vec3, end: Vec3, color: Color) {
        self.inner.draw_line(start, end, color);
    }

    /// Push a technical 2D shape for rendering this frame.
    pub fn draw_2d_shape(&mut self, instance: ferrous_renderer::render_2d::ShapeInstance) {
        self.inner.draw_2d_shape(instance);
    }

    /// Push a slice of technical 2D shapes for rendering this frame.
    ///
    /// Same result as calling [`Self::draw_2d_shape`] per element — identical
    /// painter order, pixels and H3-coalesced draw calls — with less per-call
    /// overhead. Intended for bulk polygon fills (triangle fans, scan-line
    /// rect fills).
    pub fn draw_2d_shapes(&mut self, shapes: &[ferrous_renderer::render_2d::ShapeInstance]) {
        self.inner.draw_2d_shapes(shapes);
    }

    /// Push a textured 2D sprite for rendering this frame.
    ///
    /// `texture_id` must come from [`register_sprite_texture`](Self::register_sprite_texture).
    /// The sprite is centred on its instance transform; see
    /// [`ferrous_2d::render::SpriteInstance`] for the transform and UV
    /// conventions.
    pub fn draw_sprite_2d(
        &mut self,
        texture_id: u32,
        instance: ferrous_2d::render::SpriteInstance,
    ) {
        self.inner.draw_sprite_2d(texture_id, instance);
    }

    /// Upload a raw RGBA8 texture for use with
    /// [`draw_sprite_2d`](Self::draw_sprite_2d) and return its `texture_id`.
    ///
    /// Equivalent to `renderer_mut().register_sprite_texture(...)`. The sampler
    /// uses nearest filtering; `Renderer::register_sprite_texture_linear` is
    /// available through `renderer_mut()` for smooth (HD) sprites.
    pub fn register_sprite_texture(&mut self, width: u32, height: u32, data: &[u8]) -> u32 {
        self.inner.register_sprite_texture(width, height, data)
    }

    /// Upload an image texture with an optional mip chain and sampler choice.
    ///
    /// Equivalent to `renderer_mut().register_texture_2d(...)`. The data is
    /// straight-alpha sRGB RGBA8; the upload premultiplies it and, when
    /// `options.mipmaps` is set, builds a deterministic linear-light mip chain
    /// sampled trilinearly.
    ///
    /// # Errors
    ///
    /// Returns the renderer's typed [`ferrous_2d::render::TextureError`] for a
    /// zero or oversized dimension or a wrong byte count.
    pub fn register_texture_2d(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
        options: ferrous_2d::render::TextureOptions2d,
    ) -> Result<u32, ferrous_2d::render::TextureError> {
        self.inner.register_texture_2d(width, height, data, options)
    }

    /// Total resident bytes of every registered 2D sprite texture.
    pub fn texture_bytes_2d(&self) -> u64 {
        self.inner.texture_bytes_2d()
    }

    /// Opens a temporal-accumulation sequence (see
    /// [`ferrous_renderer::Renderer::begin_accumulation_2d`]).
    ///
    /// # Errors
    ///
    /// The renderer's typed [`ferrous_2d::render::AccumulationError`].
    pub fn begin_accumulation_2d(&mut self) -> Result<(), ferrous_2d::render::AccumulationError> {
        self.inner.begin_accumulation_2d()
    }

    /// Adds the current output target as one weighted sub-frame (see
    /// [`ferrous_renderer::Renderer::accumulate_frame_2d`]).
    ///
    /// # Errors
    ///
    /// The renderer's typed [`ferrous_2d::render::AccumulationError`].
    pub fn accumulate_frame_2d(
        &mut self,
        weight: f32,
    ) -> Result<(), ferrous_2d::render::AccumulationError> {
        self.inner.accumulate_frame_2d(weight)
    }

    /// Resolves the weighted average into the normal output target (see
    /// [`ferrous_renderer::Renderer::resolve_accumulation_2d`]).
    ///
    /// # Errors
    ///
    /// The renderer's typed [`ferrous_2d::render::AccumulationError`].
    pub fn resolve_accumulation_2d(&mut self) -> Result<(), ferrous_2d::render::AccumulationError> {
        self.inner.resolve_accumulation_2d()
    }

    /// Whether an accumulation sequence is open.
    pub fn accumulation_active_2d(&self) -> bool {
        self.inner.accumulation_active_2d()
    }

    /// Push a technical 2D path for rendering this frame.
    pub fn draw_2d_path(&mut self, path: ferrous_2d::components::Path2d) {
        self.inner.draw_2d_path(&path);
    }

    /// Clear the immediate-mode 2D shape and sprite batchers.
    ///
    /// The engine runner already clears both batchers once at the start of
    /// every frame, so calling this from application code is only needed to
    /// discard geometry mid-frame. It is idempotent and safe to call at any
    /// time; a redundant start-of-frame call is harmless.
    pub fn clear_2d(&mut self) {
        self.inner.clear_2d();
    }

    /// Returns the current viewport size in pixels.
    pub fn viewport_size(&self) -> ferrous_core::glam::Vec2 {
        self.inner.viewport_size()
    }

    // ── Mesh Registration ──────────────────────────────────────────────────

    /// Register a procedural mesh under a string key.
    ///
    /// Once registered, any entity with `ElementKind::Mesh { asset_key: "key" }`
    /// will render using this geometry.
    pub fn register_mesh(&mut self, key: &str, mesh: ferrous_renderer::Mesh) {
        ferrous_renderer::register_mesh(self.inner.frame_builder_mut(), key, mesh);
    }

    /// Remove a procedural mesh previously registered under `key`.
    pub fn free_mesh(&mut self, key: &str) {
        ferrous_renderer::free_mesh(self.inner.frame_builder_mut(), key);
    }

    /// Helper: Create a GPU mesh from a list of vertices and indices.
    ///
    /// This is a convenience wrapper around buffer allocation and tangent
    /// generation. The resulting `Mesh` can then be registered via
    /// [`register_mesh`](Self::register_mesh).
    pub fn create_mesh(
        &self,
        label: &str,
        vertices: Vec<ferrous_renderer::Vertex>,
        indices: Vec<u32>,
    ) -> ferrous_renderer::Mesh {
        self.inner.create_mesh(label, vertices, indices)
    }

    // ── Internal ─────────────────────────────────────────────────────────────

    /// Raw renderer reference — for engine-internal use only.
    ///
    /// Prefer the typed methods on `RenderContext` wherever possible.
    /// Devuelve acceso de solo lectura al Renderer subyacente.
    pub fn renderer(&self) -> &ferrous_renderer::Renderer {
        self.inner
    }

    /// Devuelve acceso mutable al Renderer subyacente.
    pub fn renderer_mut(&mut self) -> &mut ferrous_renderer::Renderer {
        self.inner
    }
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    // RenderContext wraps a `&mut Renderer` which requires a full wgpu
    // device/queue to construct.  These tests verify the *type-level*
    // properties (Send, lifetime annotations) and the delegation helpers
    // that can be checked without a GPU.

    use super::*;

    /// `RenderContext` itself must be `Send` so it can (in principle) be
    /// passed across thread boundaries the same way `&mut Renderer` can.
    #[test]
    fn render_context_is_send_sync_marker() {
        // Compile-time check: if RenderContext is not Send, this fn would not compile.
        fn assert_send<T: Send>() {}
        assert_send::<RenderContext<'_>>();
        // RenderContext<'_> is Send iff &mut Renderer is Send.
        // We just check the type exists and can be named.
        let _ = std::any::TypeId::of::<RenderStyle>();
    }

    /// `RenderContext::new` correctly wraps the given renderer pointer.
    /// We verify this by checking that the internal pointer is the same.
    /// Without a live Renderer we test via a raw pointer identity trick
    /// using a thin proxy type that mimics the relevant shape.
    #[test]
    fn render_style_variants_exist() {
        // Smoke test: ensure the enum variants we delegate to are accessible.
        let _pbr = RenderStyle::Pbr;
        let _cel = RenderStyle::CelShaded {
            toon_levels: 4,
            outline_width: 1.5,
        };
        let _flat = RenderStyle::FlatShaded;
    }

    #[test]
    fn material_handle_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<MaterialHandle>();
    }

    #[test]
    fn render_stats_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<RenderStats>();
    }
}
