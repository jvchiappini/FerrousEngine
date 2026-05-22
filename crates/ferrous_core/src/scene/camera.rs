#![cfg(feature = "ecs")]

use crate::scene::controller::Controller;
use glam::{Mat4, Vec3};

/// Simple camera state owned by the scene.  for now we only keep track of
/// the eye position; additional parameters (target, frustum, etc.) can be
/// added later when the need arises.
/// Camera used by both renderer and editor.  The struct lives in core so that
/// applications can inspect or modify it directly; renderer-specific code still
/// owns GPU resources such as the uniform buffer.
/// Type of projection used by the camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Projection {
    Perspective { fov_y_radians: f32, aspect_ratio: f32, z_near: f32, z_far: f32 },
    Orthographic { left: f32, right: f32, bottom: f32, top: f32, z_near: f32, z_far: f32 },
}

/// Simple camera state owned by the scene.
#[derive(Debug, Clone)]
pub struct Camera {
    // --- view parameters --------------------------------------------------
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    // --- projection parameters --------------------------------------------
    pub projection: Projection,
    // --- input controller --------------------------------------------------
    pub controller: Controller,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: Vec3::ZERO,
            target: Vec3::ZERO,
            up: Vec3::Y,
            projection: Projection::Perspective {
                fov_y_radians: 45.0f32.to_radians(),
                aspect_ratio: 1.0,
                z_near: 0.1,
                z_far: 100.0,
            },
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
        if let Projection::Perspective { fov_y_radians, .. } = &mut self.projection {
            *fov_y_radians = deg.to_radians();
        }
    }

    /// Set near / far clipping planes.
    pub fn set_near_far(&mut self, near: f32, far: f32) {
        match &mut self.projection {
            Projection::Perspective { z_near, z_far, .. } => {
                *z_near = near;
                *z_far = far;
            }
            Projection::Orthographic { z_near, z_far, .. } => {
                *z_near = near;
                *z_far = far;
            }
        }
    }

    /// Build the combined view-projection matrix from the current parameters.
    pub fn build_view_projection_matrix(&self) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye, self.target, self.up);
        let proj = match self.projection {
            Projection::Perspective { fov_y_radians, aspect_ratio, z_near, z_far } => {
                Mat4::perspective_rh(fov_y_radians, aspect_ratio, z_near, z_far)
            }
            Projection::Orthographic { left, right, bottom, top, z_near, z_far } => {
                Mat4::orthographic_rh(left, right, bottom, top, z_near, z_far)
            }
        };
        proj * view
    }

    /// Update aspect ratio and recalc projection when viewport dimensions change.
    pub fn set_aspect(&mut self, aspect: f32) {
        match &mut self.projection {
            Projection::Perspective { aspect_ratio, ..} => *aspect_ratio = aspect,
            Projection::Orthographic { left, right, top, .. } => {
                let h = *top;
                let w = h * aspect;
                *left = -w;
                *right = w;
            }
        }
    }

    /// Set 2D orthographic mode (centered at 0,0) or revert to 3D perspective.
    pub fn set_mode_2d(&mut self, enabled: bool, viewport_height: Option<f32>) {
        if enabled {
            // If viewport_height is provided, scale 1 unit = 1 pixel.
            // Otherwise default logical height of 8.0 (Manim standard)
            let hh = if let Some(h) = viewport_height { h / 2.0 } else { 4.0 };
            
            let aspect = match self.projection {
                Projection::Perspective { aspect_ratio, .. } => aspect_ratio,
                Projection::Orthographic { left, right, top, bottom, .. } => (right - left) / (top - bottom),
            };
            let hw = hh * aspect;

            self.eye = Vec3::new(0.0, 0.0, 10.0);
            self.target = Vec3::ZERO;
            self.up = Vec3::Y;
            self.projection = Projection::Orthographic {
                left: -hw,
                right: hw,
                bottom: -hh,
                top: hh,
                z_near: -100.0,
                z_far: 100.0,
            };
            
            // v15: Disable controller movement in 2D mode to prevent accidental camera shifts 
            // that move the origin away from the screen center.
            self.controller.clear_bindings();
            self.controller.mouse_sensitivity = 0.0;
        } else {
            // Revert to standard 3D perspective camera
            self.eye = Vec3::new(0.0, 2.0, 5.0);
            self.target = Vec3::ZERO;
            self.up = Vec3::Y;
            let aspect = match self.projection {
                Projection::Orthographic { left, right, top, bottom, .. } => (right - left) / (top - bottom),
                _ => 1.0,
            };
            self.projection = Projection::Perspective {
                fov_y_radians: 45.0f32.to_radians(),
                aspect_ratio: aspect,
                z_near: 0.1,
                z_far: 1000.0,
            };
            self.controller = Controller::with_default_wasd();
        }
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
