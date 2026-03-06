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
    /// eye/camera world-space position (padding to 16 bytes required)
    pub position: [f32; 3],
    pub _pad: f32,
}

impl CameraUniform {
    /// Create an identity/inactive uniform.
    pub fn new() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            position: [0.0; 3],
            _pad: 0.0,
        }
    }

    /// Update the fields from a CPU-side [`Camera`] instance.
    pub fn update_view_proj(&mut self, camera: &Camera) {
        self.view_proj = camera.build_view_projection_matrix().to_cols_array_2d();
        self.position = camera.eye.to_array();
    }
}
