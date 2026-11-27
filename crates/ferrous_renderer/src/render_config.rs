//! Runtime-settable render/performance configuration.
//!
//! Exposes a small, documented [`Render2dConfig`] that a consumer can
//! read/toggle at runtime to A/B compare optimizations and to pick an
//! antialiasing policy without recompiling.
//!
//! All knobs default to the safest/most optimized behaviour.  Toggling any
//! knob takes effect on the **next frame**; no GPU pipelines are rebuilt.

/// Post-process antialiasing policy for the 2D render path.
///
/// This is a deliberately small, consumer-facing enum.  It maps 1:1 onto the
/// internal [`crate::passes::AntialiasingMode`] (which also carries FXAA
/// quality parameters):
///
/// | [`Antialiasing2d`] | Internal mode |
/// |--------------------|---------------|
/// | `Off`              | `AntialiasingMode::None` |
/// | `Fxaa`             | `AntialiasingMode::Fxaa(FxaaParams::default())` |
/// | `Smaa`             | `AntialiasingMode::Smaa` |
///
/// ## Pure2D note
///
/// [`AntialiasingPass`](crate::passes::AntialiasingPass) only runs in
/// `Full3D` (it reads the HDR scene texture, which `Pure2D` does not create).
/// In `Pure2D`/`Flat2D` the shape SDF already performs analytic AA in the
/// fragment shader, so `Off` is the correct Pure2D policy and this knob is a
/// no-op there.  Defaulting to `Off` removes the redundant
/// SDF-AA + FXAA-over-linear-HDR + optional MSAA stack of audit finding **M4**
/// (FXAA is calibrated for gamma-space input, not the linear HDR buffer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Antialiasing2d {
    /// No post-process AA.  SDF shapes still get analytic in-shader AA.
    #[default]
    Off,
    /// FXAA post-process (Full3D only).  Very low cost.
    Fxaa,
    /// SMAA 1x post-process (Full3D only).  Sharpest, three sub-passes.
    Smaa,
}

impl Antialiasing2d {
    /// Convert to the internal [`crate::passes::AntialiasingMode`].
    #[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
    pub fn to_mode(self) -> crate::passes::AntialiasingMode {
        use crate::passes::AntialiasingMode;
        match self {
            Antialiasing2d::Off => AntialiasingMode::None,
            Antialiasing2d::Fxaa => AntialiasingMode::Fxaa(crate::passes::FxaaParams::default()),
            Antialiasing2d::Smaa => AntialiasingMode::Smaa,
        }
    }

    /// Collapse the internal [`crate::passes::AntialiasingMode`] to this enum.
    #[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
    pub fn from_mode(mode: crate::passes::AntialiasingMode) -> Self {
        use crate::passes::AntialiasingMode;
        match mode {
            AntialiasingMode::None => Antialiasing2d::Off,
            AntialiasingMode::Fxaa(_) => Antialiasing2d::Fxaa,
            AntialiasingMode::Smaa => Antialiasing2d::Smaa,
        }
    }
}

/// Runtime render/performance configuration.
///
/// Obtain/set it through [`Renderer::render_2d_config`](crate::Renderer::render_2d_config)
/// and [`Renderer::set_render_2d_config`](crate::Renderer::set_render_2d_config)
/// (also surfaced on `ferrous_engine::RenderContext` as
/// `render_2d_config()` / `set_render_2d_config(...)`).
///
/// ```rust,ignore
/// let mut cfg = ctx.render.render_2d_config();
/// cfg.scratch_reuse = false;      // A/B compare the scratch-reuse optimization
/// cfg.antialiasing = Antialiasing2d::Smaa;
/// ctx.render.set_render_2d_config(cfg);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Render2dConfig {
    /// Post-process antialiasing policy.  Default [`Antialiasing2d::Off`]
    /// (avoids the redundant AA stack; see [`Antialiasing2d`]).
    pub antialiasing: Antialiasing2d,
    /// Cache the UI image bind group and rebuild it only when the texture set
    /// changes.  Default `true` (audit **H8**).
    pub ui_bind_group_cache: bool,
    /// Reuse per-frame scratch buffers instead of allocating per frame.
    /// Default `true` (audit **H8/H3**).
    pub scratch_reuse: bool,
    /// CPU-side viewport culling for the Pure2D path.  When `true` (default),
    /// instances whose world-space AABB does not intersect the visible camera
    /// rect (expanded by the effective margin) are skipped before upload and
    /// draw, so they cost neither bytes nor draw calls.  Set to `false` to
    /// restore the old "upload and draw everything" behaviour for A/B
    /// comparison; culled instances are then reported as `0`.
    pub cull_2d: bool,
    /// Extra world-space margin added around the visible camera rect before the
    /// culling AABB test, in **world units** (pixels at Pure2D zoom 1).  It
    /// exists so shapes whose anti-aliasing band / sub-pixel line widening
    /// pokes into the viewport are never popped.
    ///
    /// The effective margin is `max(cull_margin, 4 * world_units_per_pixel)`,
    /// i.e. at least a 4-pixel safety band at any zoom, plus whatever larger
    /// margin the caller requests.  Raise it if a consumer observes edge pop.
    pub cull_margin: f32,
    /// Draw the retained static 2D batches (created with
    /// [`Renderer::create_static_2d_batch`](crate::Renderer::create_static_2d_batch))
    /// during the Pure2D pass.  Default `true`.  Set to `false` to skip them for
    /// A/B comparison against pushing the same geometry through the immediate
    /// path (the batches are still alive and their buffers are not touched).
    pub static_batches: bool,
    /// Resolution multiplier for the offscreen layer and effect textures of the
    /// layered Pure2D path.  `1.0` (default) is the sentinel that lets
    /// [`layer_scale_adaptive`](Self::layer_scale_adaptive) choose per layer;
    /// any other finite positive value is an explicit override applied to every
    /// layer.  A value in `(0.0, 1.0)` trades sharpness for VRAM and fill rate,
    /// and values above `1.0` supersample.
    ///
    /// The dimensions are derived with [`scaled_extent`], so the scale is
    /// clamped to a finite positive value and every axis stays at least one
    /// texel.  Changing it recreates the layer textures on the next layered
    /// frame.
    pub layer_resolution_scale: f32,
    /// Use a per-layer adaptive resolution factor instead of the single
    /// [`layer_resolution_scale`](Self::layer_resolution_scale).  Default
    /// `true`.
    ///
    /// A layer whose effect chain starts with a [`Blur`](ferrous_2d::render::Effect2d::Blur)
    /// renders at `min(1, k / radius)` (see
    /// [`ferrous_2d::render::adaptive_layer_scale`]); every other layer keeps
    /// `1.0`.  Calling [`with_layer_resolution_scale`](Self::with_layer_resolution_scale)
    /// turns this off: an explicit override wins and applies to every layer.
    pub layer_scale_adaptive: bool,
    /// Allocate and use the GUI pass depth attachment for Early-Z.  Default
    /// `true`, which is the current behaviour.  When `false`, a consumer may
    /// skip the full-size GUI depth texture and its per-frame clear; the visual
    /// result is unchanged for the normal painter-ordered UI.
    pub ui_depth: bool,
    /// Restrict the full-target blur, effect-composite and mask passes to the
    /// active layer rectangle instead of clearing and drawing the whole
    /// surface.  Default `false` (the whole surface is covered, reproducing
    /// today's pixels).  Enabling it is only correct when the scaled layer
    /// rectangle is used as the pass viewport/scissor; see
    /// [`layer_resolution_scale`](Self::layer_resolution_scale).
    pub effect_scissor: bool,
    /// Draw the UI with the overdraw debug pipelines (prompt 18 P5).  Default
    /// `false`: normal shading.  When `true` every fragment adds a fixed heat
    /// colour so overlap is visible; it changes the picture on purpose and is
    /// never part of a golden.
    pub overdraw_enabled: bool,
}

impl Render2dConfig {
    /// Sets [`layer_resolution_scale`](Self::layer_resolution_scale) as an
    /// explicit override for every layer and disables
    /// [`layer_scale_adaptive`](Self::layer_scale_adaptive).
    ///
    /// Non-finite and non-positive values are treated as `1.0` when the layer
    /// pool is built, so this method stores the value as given and leaves the
    /// clamping to [`scaled_extent`].
    #[must_use]
    pub fn with_layer_resolution_scale(mut self, scale: f32) -> Self {
        self.layer_resolution_scale = scale;
        self.layer_scale_adaptive = false;
        self
    }

    /// Enables or disables the per-layer adaptive factor.
    ///
    /// Disabling it makes [`layer_resolution_scale`](Self::layer_resolution_scale)
    /// the explicit override for every layer.  Enabling it restores the `1.0`
    /// sentinel, so the per-layer factor applies again.
    #[must_use]
    pub fn with_adaptive_layer_scale(mut self, adaptive: bool) -> Self {
        self.layer_scale_adaptive = adaptive;
        if adaptive {
            self.layer_resolution_scale = 1.0;
        }
        self
    }

    /// The scale a layer's offscreen textures should use.
    ///
    /// With [`layer_scale_adaptive`](Self::layer_scale_adaptive) the leading
    /// blur factor applies as long as `layer_resolution_scale` is the default
    /// `1.0`.  Any non-default explicit value, or
    /// `layer_scale_adaptive == false`, makes the explicit scale win for every
    /// layer.  Invalid explicit values fall back to `1.0`.
    #[must_use]
    pub fn effective_layer_scale(&self, effects: &[ferrous_2d::render::Effect2d]) -> f32 {
        let default_scale = (self.layer_resolution_scale - 1.0).abs() <= 1e-6;
        if self.layer_scale_adaptive && default_scale {
            ferrous_2d::render::adaptive_layer_scale(effects)
        } else if self.layer_resolution_scale.is_finite() && self.layer_resolution_scale > 0.0 {
            self.layer_resolution_scale
        } else {
            1.0
        }
    }

    /// Sets [`ui_depth`](Self::ui_depth).
    #[must_use]
    pub fn with_ui_depth(mut self, enabled: bool) -> Self {
        self.ui_depth = enabled;
        self
    }

    /// Sets [`effect_scissor`](Self::effect_scissor).
    #[must_use]
    pub fn with_effect_scissor(mut self, enabled: bool) -> Self {
        self.effect_scissor = enabled;
        self
    }

    /// Sets [`overdraw_enabled`](Self::overdraw_enabled).
    #[must_use]
    pub fn with_overdraw(mut self, enabled: bool) -> Self {
        self.overdraw_enabled = enabled;
        self
    }
}

/// Scales `width x height` by `scale`, clamping every axis to at least one
/// texel.
///
/// The helper is pure and shared by the layer/effect allocation sites and by
/// callers that need to know the effective intermediate extent.  A non-finite
/// or non-positive scale is treated as `1.0`, so a bad configuration cannot
/// produce a zero-sized texture (which wgpu rejects).  Rounding is
/// `round-half-away-from-zero` through `f32::round`; at the default `1.0` the
/// result is exactly `(width, height)`, so default rendering is pixel
/// identical.
#[must_use]
pub fn scaled_extent(width: u32, height: u32, scale: f32) -> (u32, u32) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let axis = |value: u32| -> u32 {
        let scaled = (value as f32 * scale).round();
        if scaled.is_finite() && scaled >= 1.0 {
            scaled as u32
        } else {
            1
        }
    };
    (axis(width), axis(height))
}

impl Default for Render2dConfig {
    fn default() -> Self {
        Self {
            antialiasing: Antialiasing2d::Off,
            ui_bind_group_cache: true,
            scratch_reuse: true,
            cull_2d: true,
            cull_margin: 4.0,
            static_batches: true,
            layer_resolution_scale: 1.0,
            layer_scale_adaptive: true,
            ui_depth: true,
            effect_scissor: false,
            overdraw_enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_optimized_policy() {
        let cfg = Render2dConfig::default();
        assert_eq!(cfg.antialiasing, Antialiasing2d::Off);
        assert!(cfg.ui_bind_group_cache);
        assert!(cfg.scratch_reuse);
        assert!(cfg.cull_2d);
        assert!(cfg.cull_margin >= 4.0, "default margin must be AA-safe");
        assert!(cfg.static_batches);
        assert_eq!(cfg.layer_resolution_scale, 1.0);
        assert!(cfg.layer_scale_adaptive);
        assert!(cfg.ui_depth);
        assert!(!cfg.effect_scissor);
        assert!(!cfg.overdraw_enabled);
    }

    #[test]
    fn scaled_extent_default_is_the_identity() {
        assert_eq!(scaled_extent(1600, 900, 1.0), (1600, 900));
        assert_eq!(scaled_extent(1, 1, 1.0), (1, 1));
    }

    #[test]
    fn scaled_extent_scales_and_rounds() {
        assert_eq!(scaled_extent(1600, 900, 0.5), (800, 450));
        assert_eq!(scaled_extent(1920, 1080, 0.75), (1440, 810));
        assert_eq!(scaled_extent(100, 100, 1.5), (150, 150));
    }

    #[test]
    fn scaled_extent_clamps_to_one_texel() {
        assert_eq!(scaled_extent(1600, 900, 0.0), (1600, 900));
        assert_eq!(scaled_extent(0, 0, 1.0), (1, 1));
        assert_eq!(scaled_extent(10, 10, 0.01), (1, 1));
        assert_eq!(scaled_extent(10, 10, f32::NAN), (10, 10));
        assert_eq!(scaled_extent(10, 10, f32::INFINITY), (10, 10));
        assert_eq!(scaled_extent(10, 10, -2.0), (10, 10));
    }

    #[test]
    fn antialiasing_round_trips_through_internal_mode() {
        for aa in [
            Antialiasing2d::Off,
            Antialiasing2d::Fxaa,
            Antialiasing2d::Smaa,
        ] {
            assert_eq!(Antialiasing2d::from_mode(aa.to_mode()), aa);
        }
    }

    #[test]
    fn an_explicit_layer_scale_overrides_the_adaptive_factor() {
        use ferrous_2d::render::Effect2d;

        let blur = [Effect2d::Blur { radius: 36.0 }];
        let adaptive = Render2dConfig::default();
        // ADAPTIVE_LAYER_SCALE_K = 9: a 36 px leading blur renders at 0.25.
        assert_eq!(adaptive.effective_layer_scale(&blur), 0.25);

        let forced_one = adaptive.with_layer_resolution_scale(1.0);
        assert!(!forced_one.layer_scale_adaptive);
        assert_eq!(forced_one.effective_layer_scale(&blur), 1.0);

        let forced_half = adaptive.with_layer_resolution_scale(0.5);
        assert_eq!(forced_half.effective_layer_scale(&blur), 0.5);

        // A direct non-default assignment is an override too (the default 1.0
        // must remain the "use the adaptive factor" sentinel).
        let mut direct = adaptive;
        direct.layer_resolution_scale = 0.75;
        assert_eq!(direct.effective_layer_scale(&blur), 0.75);

        // Invalid explicit values keep the identity.
        let invalid = adaptive.with_layer_resolution_scale(f32::NAN);
        assert_eq!(invalid.effective_layer_scale(&blur), 1.0);

        // Restoring the adaptive factor resets the 1.0 sentinel.
        let adaptive_again = forced_half.with_adaptive_layer_scale(true);
        assert_eq!(adaptive_again.effective_layer_scale(&blur), 0.25);
    }
}
