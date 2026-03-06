//! PBR material types shared between core and renderer.
#![cfg(feature = "ecs")]
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

// ── Render Quality ────────────────────────────────────────────────────────────

/// Coarse render-quality preset.  Controls which passes are active and at what
/// resolution they run.
///
/// Quality presets are orthogonal to [`RenderStyle`]: you can run cel-shaded
/// at `Ultra` quality or PBR at `Low` quality.
///
/// | Preset  | SSAO | Bloom | Shadows | IBL | MSAA |
/// |---------|------|-------|---------|-----|------|
/// | Ultra   | ✅   | ✅    | ✅ 2048 | ✅  | 4x   |
/// | High    | ✅   | ✅    | ✅ 1024 | ✅  | 2x   |
/// | Medium  | ❌   | ✅    | ✅ 512  | ❌  | 1x   |
/// | Low     | ❌   | ❌    | ❌      | ❌  | 1x   |
/// | Minimal | ❌   | ❌    | ❌      | ❌  | 1x (depth only) |
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RenderQuality {
    /// Full PBR + SSAO + Bloom + 4x MSAA — maximum visual fidelity.
    Ultra,
    /// Full PBR + SSAO + Bloom + 2x MSAA — high fidelity, lower GPU cost.
    High,
    /// PBR without SSAO; bloom active; no IBL; 1x MSAA — balanced.
    Medium,
    /// Simple diffuse + directional light only; no post-processing.
    Low,
    /// Depth-only (headless/server builds or extreme performance budgets).
    Minimal,
}

impl Default for RenderQuality {
    fn default() -> Self {
        RenderQuality::High
    }
}

impl RenderQuality {
    /// Returns `true` if screen-space ambient occlusion should be enabled.
    pub fn ssao_enabled(self) -> bool {
        matches!(self, RenderQuality::Ultra | RenderQuality::High)
    }

    /// Returns `true` if bloom post-processing should be enabled.
    pub fn bloom_enabled(self) -> bool {
        matches!(
            self,
            RenderQuality::Ultra | RenderQuality::High | RenderQuality::Medium
        )
    }

    /// Returns `true` if shadow maps should be rendered.
    pub fn shadows_enabled(self) -> bool {
        matches!(
            self,
            RenderQuality::Ultra | RenderQuality::High | RenderQuality::Medium
        )
    }

    /// Returns `true` if image-based lighting (IBL) should be computed.
    pub fn ibl_enabled(self) -> bool {
        matches!(self, RenderQuality::Ultra | RenderQuality::High)
    }

    /// Shadow-map resolution in texels (one side of the square depth texture).
    pub fn shadow_resolution(self) -> u32 {
        match self {
            RenderQuality::Ultra => 2048,
            RenderQuality::High => 1024,
            RenderQuality::Medium => 512,
            _ => 0,
        }
    }

    /// Recommended MSAA sample count.
    pub fn msaa_sample_count(self) -> u32 {
        match self {
            RenderQuality::Ultra => 4,
            RenderQuality::High => 2,
            _ => 1,
        }
    }

    /// Parse a quality preset from its string name (case-insensitive).
    ///
    /// Accepted values: `"ultra"`, `"high"`, `"medium"`, `"low"`, `"minimal"`.
    /// Returns `None` if the string does not match any variant.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "ultra" => Some(RenderQuality::Ultra),
            "high" => Some(RenderQuality::High),
            "medium" => Some(RenderQuality::Medium),
            "low" => Some(RenderQuality::Low),
            "minimal" => Some(RenderQuality::Minimal),
            _ => None,
        }
    }

    /// Return the canonical lowercase string name for this variant.
    pub fn as_str(self) -> &'static str {
        match self {
            RenderQuality::Ultra => "ultra",
            RenderQuality::High => "high",
            RenderQuality::Medium => "medium",
            RenderQuality::Low => "low",
            RenderQuality::Minimal => "minimal",
        }
    }
}

// ── Material component ────────────────────────────────────────────────────

use crate::color::Color;
use ferrous_ecs::component::Component;

/// High-level material component.  Attach this to any world entity to control
/// how it is shaded.
///
/// Unlike the lower-level [`MaterialDescriptor`], `Material` works entirely in
/// terms of engine types (`Color`, `RenderStyle`) and handles the
/// linear-colour conversion internally.
///
/// # Usage
/// ```rust,ignore
/// world.spawn((
///     Transform::from_position(Vec3::ZERO),
///     Material::pbr().color(Color::srgb(0.9, 0.1, 0.1)).metallic(0.0).roughness(0.3).build(),
/// ));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    /// Base/albedo colour (linear space).
    pub base_color: Color,
    /// Metallic factor (0 = dielectric, 1 = metal).
    pub metallic: f32,
    /// Roughness factor (0 = mirror, 1 = fully rough).
    pub roughness: f32,
    /// Emissive colour (linear space).
    pub emissive: Color,
    /// Emissive strength multiplier.
    pub emissive_strength: f32,
    /// Transparency mode.
    pub alpha_mode: AlphaMode,
    /// Whether the material renders from both sides.
    pub double_sided: bool,
    /// Per-material shading style override.  `None` inherits the global
    /// renderer style.
    pub style_override: Option<RenderStyle>,
}

impl Component for Material {}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color: Color::WHITE,
            metallic: 0.0,
            roughness: 0.5,
            emissive: Color::BLACK,
            emissive_strength: 0.0,
            alpha_mode: AlphaMode::Opaque,
            double_sided: false,
            style_override: None,
        }
    }
}

impl Material {
    /// Begin building a PBR material (Cook-Torrance BRDF).
    pub fn pbr() -> MaterialBuilder {
        MaterialBuilder::new()
    }

    /// Begin building a cel-shaded (toon) material.
    pub fn cel_shaded() -> MaterialBuilder {
        MaterialBuilder::new().style(RenderStyle::CelShaded {
            toon_levels: 4,
            outline_width: 0.0,
        })
    }

    /// Begin building a flat-shaded (low-poly) material.
    pub fn flat_shaded() -> MaterialBuilder {
        MaterialBuilder::new().style(RenderStyle::FlatShaded)
    }

    /// Convert this `Material` into a renderer-compatible [`MaterialDescriptor`].
    ///
    /// Call this inside the renderer's `sync_world` when you need to upload
    /// the GPU uniform; the `Material` component itself stays in the ECS.
    pub fn to_descriptor(&self) -> MaterialDescriptor {
        let c = self.base_color;
        let e = self.emissive;
        MaterialDescriptor {
            base_color: [c.r, c.g, c.b, c.a],
            emissive: [e.r, e.g, e.b],
            emissive_strength: self.emissive_strength,
            metallic: self.metallic,
            roughness: self.roughness,
            alpha_mode: self.alpha_mode.clone(),
            double_sided: self.double_sided,
            style_override: self.style_override,
            ..MaterialDescriptor::default()
        }
    }
}

// ── MaterialBuilder ───────────────────────────────────────────────────────

/// Fluent builder for [`Material`].
///
/// ```rust,ignore
/// let mat = Material::pbr()
///     .color(Color::srgb(0.8, 0.2, 0.2))
///     .metallic(0.0)
///     .roughness(0.3)
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct MaterialBuilder {
    inner: Material,
}

impl MaterialBuilder {
    /// Create a builder with default PBR settings.
    pub fn new() -> Self {
        Self {
            inner: Material::default(),
        }
    }

    /// Set the base colour (albedo).
    pub fn color(mut self, c: Color) -> Self {
        self.inner.base_color = c;
        self
    }

    /// Set the metallic factor (`0.0` = dielectric, `1.0` = fully metallic).
    pub fn metallic(mut self, v: f32) -> Self {
        self.inner.metallic = v.clamp(0.0, 1.0);
        self
    }

    /// Set the roughness factor (`0.0` = mirror-smooth, `1.0` = fully rough).
    pub fn roughness(mut self, v: f32) -> Self {
        self.inner.roughness = v.clamp(0.0, 1.0);
        self
    }

    /// Set the emissive colour and strength.
    pub fn emissive(mut self, c: Color, strength: f32) -> Self {
        self.inner.emissive = c;
        self.inner.emissive_strength = strength;
        self
    }

    /// Enable standard alpha blending (`AlphaMode::Blend`).
    pub fn alpha_blend(mut self) -> Self {
        self.inner.alpha_mode = AlphaMode::Blend;
        self
    }

    /// Enable alpha-test (`AlphaMode::Mask`) with the given cutoff threshold.
    pub fn alpha_mask(mut self, cutoff: f32) -> Self {
        self.inner.alpha_mode = AlphaMode::Mask { cutoff };
        self
    }

    /// Render both sides of every face.
    pub fn double_sided(mut self) -> Self {
        self.inner.double_sided = true;
        self
    }

    /// Override the shading style for this material.
    pub fn style(mut self, s: RenderStyle) -> Self {
        self.inner.style_override = Some(s);
        self
    }

    /// Consume the builder and return the finished [`Material`].
    pub fn build(self) -> Material {
        self.inner
    }
}

impl Default for MaterialBuilder {
    fn default() -> Self {
        Self::new()
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

    // ── Phase 4.5: Material builder tests ────────────────────────────────

    #[test]
    fn material_pbr_builder_defaults() {
        let mat = Material::pbr().build();
        assert_eq!(mat.base_color, Color::WHITE);
        assert!((mat.metallic - 0.0).abs() < 1e-6);
        assert!((mat.roughness - 0.5).abs() < 1e-6);
        assert!(mat.style_override.is_none());
    }

    #[test]
    fn material_cel_shaded_builder_sets_style() {
        let mat = Material::cel_shaded().build();
        assert!(matches!(
            mat.style_override,
            Some(RenderStyle::CelShaded { .. })
        ));
    }

    #[test]
    fn material_flat_shaded_builder_sets_style() {
        let mat = Material::flat_shaded().build();
        assert_eq!(mat.style_override, Some(RenderStyle::FlatShaded));
    }

    #[test]
    fn material_builder_color_roundtrip() {
        let c = Color::srgb(0.5, 0.2, 0.8);
        let mat = Material::pbr().color(c).build();
        assert!((mat.base_color.r - c.r).abs() < 1e-6, "r mismatch");
        assert!((mat.base_color.g - c.g).abs() < 1e-6, "g mismatch");
        assert!((mat.base_color.b - c.b).abs() < 1e-6, "b mismatch");
    }

    #[test]
    fn material_builder_metallic_roughness() {
        let mat = Material::pbr().metallic(1.0).roughness(0.1).build();
        assert!((mat.metallic - 1.0).abs() < 1e-6);
        assert!((mat.roughness - 0.1).abs() < 1e-6);
    }

    #[test]
    fn material_builder_emissive() {
        let mat = Material::pbr()
            .emissive(Color::rgb(1.0, 0.4, 0.1), 5.0)
            .build();
        assert!((mat.emissive_strength - 5.0).abs() < 1e-6);
        assert!((mat.emissive.r - 1.0).abs() < 1e-6);
    }

    #[test]
    fn material_builder_alpha_blend() {
        let mat = Material::pbr().alpha_blend().build();
        assert_eq!(mat.alpha_mode, AlphaMode::Blend);
    }

    #[test]
    fn material_builder_double_sided() {
        let mat = Material::pbr().double_sided().build();
        assert!(mat.double_sided);
    }

    #[test]
    fn material_to_descriptor_preserves_fields() {
        let mat = Material::pbr()
            .color(Color::srgb(0.8, 0.2, 0.1))
            .metallic(0.7)
            .roughness(0.3)
            .build();
        let desc = mat.to_descriptor();
        assert!((desc.metallic - 0.7).abs() < 1e-6);
        assert!((desc.roughness - 0.3).abs() < 1e-6);
        // base_color should reflect the linear-space color
        assert!((desc.base_color[0] - mat.base_color.r).abs() < 1e-6);
    }

    #[test]
    fn material_implements_component() {
        // Verify Material can be used as an ECS component (just needs to compile).
        use ferrous_ecs::prelude::Component;
        fn assert_component<T: Component>() {}
        assert_component::<Material>();
    }
}
