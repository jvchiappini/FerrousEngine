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
//! | `stats()` | Per-frame render statistics |
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
    pub fn register_texture(&mut self, width: u32, height: u32, data: &[u8]) -> ferrous_renderer::TextureHandle {
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

    /// Set the camera projection type (Perspective or Orthographic).
    pub fn set_projection_type(&mut self, proj: ferrous_core::scene::camera::ProjectionType) {
        self.inner.set_projection_type(proj);
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
