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
        }
    }
}
