use bytemuck::{Pod, Zeroable};

/// GPU-side uniform representing a single directional light.
///
/// Layout matches the WGSL struct used in `pbr.wgsl`:
///
/// ```wgsl
/// struct DirectionalLight {
///     direction: vec3<f32>;
///     _pad0: f32;
///     color: vec3<f32>;
///     intensity: f32;
/// };
/// ```
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Debug)]
pub struct DirectionalLightUniform {
    pub direction: [f32; 3],
    pub _pad0: f32,
    pub color: [f32; 3],
    pub intensity: f32,
}

impl Default for DirectionalLightUniform {
    fn default() -> Self {
        Self {
            // pointing down the negative Y axis (sun-like)
            direction: [0.0, -1.0, 0.0],
            _pad0: 0.0,
            color: [1.0, 1.0, 1.0],
            intensity: 3.0,
        }
    }
}
