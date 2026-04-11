//! GPU-visible camera data structures.
//!
//! This module holds the uniform layout that is pushed to the GPU each frame.
//! It was formerly defined in `ferrous_core::scene::camera::CameraUniform`, but
//! the type really belongs to the renderer (it's only ever consumed by GPU
//! code).  Moving it here removes an unnecessary dependency on `bytemuck` from
//! the core crate and clarifies the ownership boundary.

use crate::camera::Camera;
use glam::Mat4;

/// Uniform data that will be uploaded to the GPU. The shader only needs the
/// 4x4 view-projection matrix and the camera world position.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    /// eye/camera world-space position
    pub position: [f32; 3],
    pub exposure: f32, // repurposed from _pad
    pub fog_color: [f32; 3],
    pub fog_density: f32,
    /// Reserved space to reach 256-byte alignment (WebGPU requirement)
    /// 64 (mat4) + 12 (pos) + 4 (exp) + 12 (fog) + 4 (dens) = 96 bytes.
    /// 256 - 96 = 160 bytes = 40 f32s.
    pub _alignment_padding: [[f32; 4]; 10],
}

impl CameraUniform {
    /// Create an identity/inactive uniform.
    pub fn new() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            position: [0.0; 3],
            exposure: 0.5,
            fog_color: [0.75, 0.8, 0.85],
            fog_density: 0.02,
            _alignment_padding: [[0.0; 4]; 10],
        }
    }

    /// Update the fields from a CPU-side [`Camera`] instance.
    pub fn update_view_proj(&mut self, camera: &Camera) {
        self.view_proj = camera.build_view_projection_matrix().to_cols_array_2d();
        self.position = camera.eye.to_array();
    }
}
