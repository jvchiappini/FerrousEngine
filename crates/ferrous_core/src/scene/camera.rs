#![cfg(feature = "ecs")]

use crate::scene::controller::Controller;
use glam::{Mat4, Vec3};

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
    /// Create a camera with sensible defaults: positioned at `(0, 2, 5)`,
    /// looking at the origin, 45° FoV, default controller (WASD).
    pub fn new() -> Self {
        Self {
            eye: glam::Vec3::new(0.0, 2.0, 5.0),
            target: glam::Vec3::ZERO,
            ..Default::default()
        }
    }

    /// Alias for `eye` — whichever name feels more natural.
    pub fn position(&self) -> Vec3 {
        self.eye
    }

    /// Set the eye position.
    pub fn set_position(&mut self, pos: Vec3) {
        self.eye = pos;
    }

    /// Look from `eye` towards `target`.
    pub fn look_at(&mut self, eye: Vec3, target: Vec3) {
        self.eye = eye;
        self.target = target;
    }

    /// Set vertical field of view in degrees.
    pub fn set_fov_degrees(&mut self, deg: f32) {
        self.fovy = deg.to_radians();
    }

    /// Set near / far clipping planes.
    pub fn set_near_far(&mut self, near: f32, far: f32) {
        self.znear = near;
        self.zfar = far;
    }

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

// NOTE: the GPU-facing uniform type has been moved to the renderer crate.
// It used to reside here, but keeping it in `ferrous_core` forced every
// consumer of the core crate to pull in `bytemuck` (and transitively `wgpu`)
// just to access the type.  The renderer is the only component that ever
// touches the GPU data layout, so the struct now lives under
// `ferrous_renderer::resources::camera::CameraUniform` instead.  External
// code that previously imported `ferrous_core::scene::CameraUniform` will
// now fail to compile and should switch to the renderer path.
