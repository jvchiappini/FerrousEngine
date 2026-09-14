//! Runtime-settable render/performance configuration.
//!
//! Exposes a small, documented [`Render2dConfig`] that a consumer (e.g.
//! Koncreta) can read/toggle at runtime to A/B compare optimizations and to
//! pick an antialiasing policy without recompiling.
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
    pub fn to_mode(self) -> crate::passes::AntialiasingMode {
        use crate::passes::AntialiasingMode;
        match self {
            Antialiasing2d::Off => AntialiasingMode::None,
            Antialiasing2d::Fxaa => AntialiasingMode::Fxaa(crate::passes::FxaaParams::default()),
            Antialiasing2d::Smaa => AntialiasingMode::Smaa,
        }
    }

    /// Collapse the internal [`crate::passes::AntialiasingMode`] to this enum.
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
}

impl Default for Render2dConfig {
    fn default() -> Self {
        Self {
            antialiasing: Antialiasing2d::Off,
            ui_bind_group_cache: true,
            scratch_reuse: true,
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
    }

    #[test]
    fn antialiasing_round_trips_through_internal_mode() {
        for aa in [Antialiasing2d::Off, Antialiasing2d::Fxaa, Antialiasing2d::Smaa] {
            assert_eq!(Antialiasing2d::from_mode(aa.to_mode()), aa);
        }
    }
}
