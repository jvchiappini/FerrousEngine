use glam::{Mat4, Vec3};
use crate::scene::controller::Controller;

/// Simple camera state owned by the scene.  for now we only keep track of
/// the eye position; additional parameters (target, frustum, etc.) can be
/// added later when the need arises.
/// Camera used by both renderer and editor.  The struct lives in core so that
/// applications can inspect or modify it directly; renderer-specific code still
/// owns GPU resources such as the uniform buffer.
#[derive(Debug, Clone)]
pub struct Camera {
    // --- view parameters --------------------------------------------------
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    // --- projection parameters --------------------------------------------
    pub fovy: f32,
    pub aspect: f32,
    pub znear: f32,
    pub zfar: f32,
    // --- input controller --------------------------------------------------
    pub controller: Controller,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: Vec3::ZERO,
            target: Vec3::ZERO,
            up: Vec3::Y,
            fovy: 45.0f32.to_radians(),
            aspect: 1.0,
            znear: 0.1,
            zfar: 100.0,
            controller: Controller::new(),
        }
    }
}

impl Camera {
    /// Build the combined view-projection matrix from the current parameters.
    pub fn build_view_projection_matrix(&self) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye, self.target, self.up);
        let proj = Mat4::perspective_rh(self.fovy, self.aspect, self.znear, self.zfar);
        proj * view
    }

    /// Update aspect ratio and recalc projection when viewport dimensions change.
    pub fn set_aspect(&mut self, aspect: f32) {
        self.aspect = aspect;
    }
}

/// Uniform data that will be uploaded to the GPU. The shader only needs the
/// 4x4 view-projection matrix.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    pub fn new() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
        }
    }

    pub fn update_view_proj(&mut self, camera: &Camera) {
        self.view_proj = camera.build_view_projection_matrix().to_cols_array_2d();
    }
}
