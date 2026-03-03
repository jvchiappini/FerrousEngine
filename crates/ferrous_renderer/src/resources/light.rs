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

/// GPU-side representation of a single point light, packed for STD430
/// alignment in a storage buffer.
///
/// Both fields are `vec4` to guarantee 16-byte alignment without padding:
/// - `position_radius`: `xyz` = world-space position, `w` = influence radius
/// - `color_intensity`: `xyz` = linear RGB color, `w` = intensity scalar
///
/// Matches the WGSL struct:
/// ```wgsl
/// struct PointLight {
///     position_radius: vec4<f32>,
///     color_intensity: vec4<f32>,
/// };
/// ```
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Debug)]
pub struct PointLightUniform {
    /// xyz = world-space position, w = influence radius (metres)
    pub position_radius: [f32; 4],
    /// xyz = linear RGB color, w = intensity multiplier
    pub color_intensity: [f32; 4],
}

impl PointLightUniform {
    /// Convenience constructor.
    pub fn new(position: [f32; 3], radius: f32, color: [f32; 3], intensity: f32) -> Self {
        Self {
            position_radius: [position[0], position[1], position[2], radius],
            color_intensity: [color[0], color[1], color[2], intensity],
        }
    }
}

/// Header prepended to the point-light storage buffer.
///
/// STD430 layout: `count` is a `u32` followed by 12 bytes of padding to
/// bring the struct to 16 bytes, then the `PointLightUniform` array follows.
///
/// Total header size: 16 bytes (matches WGSL `LightStorage` up to `lights`).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Debug)]
pub struct LightStorageHeader {
    pub count: u32,
    pub _pad: [u32; 3],
}

/// Maximum point lights supported in a single frame.
/// Keeps the storage buffer bounded and prevents accidental GPU TDRs.
pub const MAX_POINT_LIGHTS: usize = 1024;
