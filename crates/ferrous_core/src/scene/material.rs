//! PBR material types shared between core and renderer.
//!
//! These definitions were originally part of the renderer crate but are now
//! owned by `ferrous_core` so that the scene layer can refer to them without
//! introducing a cyclic dependency.  The renderer re-exports the same types
//! for convenience.

/// Opaque handle referencing a material slot in the renderer's material
/// registry.  Internally this is just a small integer index, but wrapping it
/// in a newtype prevents misuse and makes the intention explicit in the
/// core crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialHandle(pub u32);
/// The well‑known default material slot that the renderer guarantees will
/// always exist.  It corresponds to a neutral white opaque PBR material.
pub const MATERIAL_DEFAULT: MaterialHandle = MaterialHandle(0);

/// How the material handles transparency.
#[derive(Clone, Debug, PartialEq)]
pub enum AlphaMode {
    /// fully opaque; no blending or alpha-test
    Opaque,
    /// alpha-tested mask.  fragments with alpha below `cutoff` are
    /// discarded in the shader.
    Mask { cutoff: f32 },
    /// standard alpha blending.
    Blend,
}

/// Describes every parameter required to build a PBR material.  This is the
/// ergonomic, serialisable type that engine clients will typically construct
/// on the CPU; the renderer converts it into a GPU bind group.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialDescriptor {
    // scalar parameters ------------------------------------------------------
    pub base_color: [f32; 4],
    pub emissive: [f32; 3],
    pub emissive_strength: f32,
    pub metallic: f32,
    pub roughness: f32,
    pub normal_scale: f32,
    pub ao_strength: f32,

    // texture slots ----------------------------------------------------------
    // the fields are renderer-local indices; we avoid pulling the actual
    // `TextureHandle` type into core to keep the dependency graph acyclic.
    // callers will typically write `Some(my_tex_handle.0)` when using the
    // descriptor, and the renderer will reinterpret the `u32` appropriately.
    pub albedo_tex: Option<u32>,
    pub normal_tex: Option<u32>,
    pub metallic_roughness_tex: Option<u32>,
    pub emissive_tex: Option<u32>,
    pub ao_tex: Option<u32>,

    // render state flags -----------------------------------------------------
    pub alpha_mode: AlphaMode,
    pub double_sided: bool,
    /// Per-material shading style override.  When `Some`, the renderer uses
    /// this style instead of the global `Renderer::render_style`.  Ignored
    /// by the PBR world pass — the style-specific passes read it directly.
    pub style_override: Option<RenderStyle>,
}

impl Default for MaterialDescriptor {
    fn default() -> Self {
        Self {
            base_color: [1.0, 1.0, 1.0, 1.0],
            emissive: [0.0, 0.0, 0.0],
            emissive_strength: 0.0,
            metallic: 0.0,
            roughness: 0.5,
            normal_scale: 1.0,
            ao_strength: 1.0,
            albedo_tex: None,
            normal_tex: None,
            metallic_roughness_tex: None,
            emissive_tex: None,
            ao_tex: None,
            alpha_mode: AlphaMode::Opaque,
            double_sided: false,
            style_override: None,
        }
    }
}

// ── Render Style ─────────────────────────────────────────────────────────────

/// Selects the shading model used when rendering a material (or globally when
/// set on the `Renderer`).
///
/// Each variant maps to a different shader and render-pass combination:
/// - `Pbr` — Cook-Torrance BRDF + IBL (default).
/// - `CelShaded` — toon ramp with flat colour bands; directional light only.
/// - `FlatShaded` — face normals derived with `dpdx`/`dpdy`; solid colours.
///
/// `Clone + Copy + PartialEq` so it can live inside `MaterialDescriptor`
/// without extra complexity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderStyle {
    /// Standard physically-based rendering (PBR) with IBL.  Default.
    Pbr,
    /// Toon / cel-shaded.  `toon_levels` controls how many discrete colour
    /// bands the ramp produces (2 = binary, 4 = cartoon look).
    CelShaded {
        /// Number of discrete toon shading bands (clamped to 2–8).
        toon_levels: u32,
        /// World-space outline thickness used by the paired `OutlinePass`.
        /// `0.0` disables the outline.
        outline_width: f32,
    },
    /// Flat / low-poly shading.  Uses face normals — no interpolation.
    FlatShaded,
}

impl Default for RenderStyle {
    fn default() -> Self {
        RenderStyle::Pbr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_style_default_is_pbr() {
        assert_eq!(RenderStyle::default(), RenderStyle::Pbr);
    }

    #[test]
    fn render_style_copy_clone() {
        let original = RenderStyle::CelShaded {
            toon_levels: 4,
            outline_width: 0.02,
        };
        let copied = original;
        let cloned = original.clone();
        assert_eq!(copied, original);
        assert_eq!(cloned, original);
    }

    #[test]
    fn render_style_flat_shaded_equality() {
        assert_eq!(RenderStyle::FlatShaded, RenderStyle::FlatShaded);
        assert_ne!(RenderStyle::FlatShaded, RenderStyle::Pbr);
    }

    #[test]
    fn render_style_cel_shaded_params() {
        let style = RenderStyle::CelShaded {
            toon_levels: 3,
            outline_width: 0.01,
        };
        if let RenderStyle::CelShaded {
            toon_levels,
            outline_width,
        } = style
        {
            assert_eq!(toon_levels, 3);
            assert!((outline_width - 0.01).abs() < 1e-6);
        } else {
            panic!("Expected CelShaded");
        }
    }

    #[test]
    fn material_descriptor_default_style_is_none() {
        let desc = MaterialDescriptor::default();
        assert!(desc.style_override.is_none());
    }

    #[test]
    fn material_descriptor_style_override_roundtrip() {
        let mut desc = MaterialDescriptor::default();
        desc.style_override = Some(RenderStyle::FlatShaded);
        assert_eq!(desc.style_override, Some(RenderStyle::FlatShaded));
    }
}
