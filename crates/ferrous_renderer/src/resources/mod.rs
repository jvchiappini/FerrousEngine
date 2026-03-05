pub mod buffer;
pub mod draw_indirect;
pub mod environment;
pub mod instance_buffer;
pub mod light;
pub mod material;
pub mod shadow;
pub mod ssao;
pub mod texture;
pub mod texture_registry;

pub use draw_indirect::{
    DrawIndirectBuffer, GpuDrawIndexedIndirect, InstanceCullBuffer, InstanceCullData,
};
pub use environment::Environment;
pub use instance_buffer::InstanceBuffer;
pub use light::{DirectionalLightUniform, LightStorageHeader, PointLightUniform, MAX_POINT_LIGHTS};
pub use material::{Material, Texture};
pub use shadow::ShadowResources;
pub use ssao::SsaoResources;

// registry exports
pub use texture_registry::{
    TextureHandle, TextureRegistry, TEXTURE_BLACK, TEXTURE_NORMAL, TEXTURE_WHITE,
};
