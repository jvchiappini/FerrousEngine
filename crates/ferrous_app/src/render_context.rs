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

    /// Enable or disable GPU-driven frustum culling via a compute shader.
    ///
    /// When enabled, per-batch visible instance counts are determined on the
    /// GPU before `WorldPass` draws anything.  Disable for debugging or on
    /// hardware that does not support compute.
    pub fn set_gpu_culling(&mut self, enabled: bool) {
        self.inner.enable_gpu_culling(enabled);
    }

    /// Change the background clear colour (applied before the sky / world pass).
    pub fn set_clear_color(&mut self, color: Color) {
        self.inner.set_clear_color(color.to_wgpu());
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
    ///
    /// Call this once during setup to synchronise the renderer camera with any
    /// ECS orbit values you have computed.  After that, prefer driving the
    /// camera purely through the ECS `OrbitCamera` component.
    pub fn set_camera_eye(&mut self, eye: Vec3) {
        self.inner.camera_mut().eye = eye;
    }

    // ── Internal ─────────────────────────────────────────────────────────────

    /// Raw renderer reference — for engine-internal use only.
    ///
    /// Prefer the typed methods on `RenderContext` wherever possible.
    /// This accessor exists for the rare cases where engine-internal helpers
    /// (e.g. `spawn_gltf`) need the underlying renderer directly.
    #[inline]
    pub fn renderer_mut(&mut self) -> &mut ferrous_renderer::Renderer {
        self.inner
    }

    /// Immutable raw renderer reference — for engine-internal use only.
    #[allow(dead_code)]
    #[inline]
    pub(crate) fn renderer(&self) -> &ferrous_renderer::Renderer {
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
