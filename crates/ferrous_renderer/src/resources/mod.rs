pub mod buffer;
pub mod environment;
pub mod instance_buffer;
pub mod light;
pub mod material;
pub mod model_buffer;
pub mod texture;
pub mod texture_registry;

pub use environment::Environment;
pub use instance_buffer::InstanceBuffer;
pub use light::{DirectionalLightUniform, LightStorageHeader, PointLightUniform, MAX_POINT_LIGHTS};
pub use material::{Material, Texture};
pub use model_buffer::ModelBuffer;

// registry exports
pub use texture_registry::{
    TextureHandle, TextureRegistry, TEXTURE_BLACK, TEXTURE_NORMAL, TEXTURE_WHITE,
};
