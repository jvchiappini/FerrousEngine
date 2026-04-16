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
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    pub view_proj: [[f32; 4]; 4],
    /// eye/camera world-space position
    pub position: [f32; 3],
    pub exposure: f32,
    pub fog_color: [f32; 3],
    pub fog_density: f32,
    pub ambient_color: [f32; 3],
    pub ambient_intensity: f32,
    /// Reserved space to reach 512-byte alignment (more future-proof)
    /// 512 - 240 = 272 bytes = 17 vec4s.
    pub _alignment_padding: [[f32; 4]; 17],
}

#[rustfmt::skip]
pub const OPENGL_TO_WGPU_MATRIX: Mat4 = Mat4::from_cols(
    glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
    glam::Vec4::new(0.0, 1.0, 0.0, 0.0),
    glam::Vec4::new(0.0, 0.0, 0.5, 0.0),
    glam::Vec4::new(0.0, 0.0, 0.5, 1.0),
);

impl CameraUniform {
    /// Create an identity/inactive uniform.
    pub fn new() -> Self {
        Self {
            view: Mat4::IDENTITY.to_cols_array_2d(),
            proj: Mat4::IDENTITY.to_cols_array_2d(),
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            position: [0.0; 3],
            exposure: 1.0,
            fog_color: [0.75, 0.8, 0.85],
            fog_density: 0.0,
            ambient_color: [0.1, 0.1, 0.1],
            ambient_intensity: 1.0,
            _alignment_padding: [[0.0; 4]; 17],
        }
    }

    /// Update the fields from a CPU-side [`Camera`] instance.
    pub fn update_view_proj(&mut self, camera: &Camera) {
        use ferrous_core::scene::camera::ProjectionType;
        
        let view = glam::Mat4::look_at_rh(camera.eye, camera.target, camera.up);
        let proj = match camera.projection_type {
            ProjectionType::Perspective => glam::Mat4::perspective_rh(
                camera.fovy,
                camera.aspect,
                camera.znear,
                camera.zfar,
            ),
            ProjectionType::Orthographic => {
                let h = camera.ortho_size;
                let w = h * camera.aspect;
                glam::Mat4::orthographic_rh(-w, w, -h, h, camera.znear, camera.zfar)
            }
        };

        // Correct depth from [-1, 1] (GL) to [0, 1] (WGPU)
        let corrected_proj = OPENGL_TO_WGPU_MATRIX * proj;
        
        self.view = view.to_cols_array_2d();
        self.proj = corrected_proj.to_cols_array_2d();
        self.view_proj = (corrected_proj * view).to_cols_array_2d();
        self.position = camera.eye.to_array();
    }
}
