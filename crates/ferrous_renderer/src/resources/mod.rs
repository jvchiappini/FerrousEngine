pub mod buffer;
pub mod camera;
#[cfg(all(
    any(feature = "render-3d", not(feature = "flat2d")),
    feature = "gpu-driven"
))]
pub mod draw_indirect;
pub mod environment;
pub mod instance_buffer;
pub mod light;
pub mod material;
pub mod readback;
pub mod shadow;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod ssao;
pub mod texture;
pub mod texture_registry;

#[cfg(all(
    any(feature = "render-3d", not(feature = "flat2d")),
    feature = "gpu-driven"
))]
pub use draw_indirect::{
    DrawIndirectBuffer, GpuDrawIndexedIndirect, InstanceCullBuffer, InstanceCullData,
};
pub use environment::Environment;
pub use instance_buffer::InstanceBuffer;
pub use light::{DirectionalLightUniform, LightStorageHeader, PointLightUniform, MAX_POINT_LIGHTS};
pub use material::{Material, Texture};
pub use shadow::ShadowResources;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use ssao::SsaoResources;

pub use camera::CameraUniform;

// registry exports
pub use texture_registry::{
    TextureHandle, TextureRegistry, TEXTURE_BLACK, TEXTURE_NORMAL, TEXTURE_WHITE,
};
