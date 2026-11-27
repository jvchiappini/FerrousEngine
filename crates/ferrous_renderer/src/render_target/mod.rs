#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod bloom;
pub mod color;
pub mod depth;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod hdr;
pub mod target;

#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use bloom::BloomTextures;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use hdr::HdrTexture;
pub use target::{LazyRenderTarget, RenderTarget, RenderTargetHandle};

/// Colour format of the 3-D HDR target, also used to construct the lazy
/// `Full3D` overlay 2-D pipeline slot.
///
/// Kept outside the gated `hdr` module so a Flat2D build can still name the
/// format when it builds that (never materialized) slot; no HDR texture is
/// allocated unless the 3-D renderer is compiled in.
pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
