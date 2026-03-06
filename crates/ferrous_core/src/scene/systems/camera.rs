//! Camera ECS components: Camera3D, Camera3DBuilder, OrbitCamera, OrbitCameraSystem.

#![cfg(feature = "ecs")]

use ferrous_ecs::prelude::*;
use ferrous_ecs::system::System;

// ────────────────────────────────────────────────────────────────────────────
// Camera3D

/// A perspective camera component for 3-D scenes.
///
/// Spawn one entity with this component to set the active view.  The
/// renderer's sync path reads the first `Camera3D` it finds each frame.
///
/// ```rust,ignore
/// world.ecs.spawn((
///     Camera3D::looking_at(Vec3::ZERO).distance(5.0).build(),
/// ));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera3D {
    pub eye: glam::Vec3,
    pub target: glam::Vec3,
    pub fov_deg: f32,
    pub near: f32,
    pub far: f32,
}
impl Component for Camera3D {}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            eye: glam::Vec3::new(0.0, 0.0, 5.0),
            target: glam::Vec3::ZERO,
            fov_deg: 45.0,
            near: 0.1,
            far: 2000.0,
        }
    }
}

impl Camera3D {
    /// Build a camera looking at `target` from directly behind (+Z).
    pub fn looking_at(target: glam::Vec3) -> Camera3DBuilder {
        Camera3DBuilder {
            inner: Self { target, ..Self::default() },
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Camera3DBuilder

/// Fluent builder for [`Camera3D`].
pub struct Camera3DBuilder {
    inner: Camera3D,
}

impl Camera3DBuilder {
    /// Set the camera eye position explicitly.
    pub fn from(mut self, eye: glam::Vec3) -> Self {
        self.inner.eye = eye;
        self
    }

    /// Place the eye at `distance` units from the target.
    pub fn distance(mut self, d: f32) -> Self {
        let dir = if (self.inner.target - self.inner.eye).length_squared() < 1e-10 {
            glam::Vec3::Z
        } else {
            (self.inner.eye - self.inner.target).normalize()
        };
        self.inner.eye = self.inner.target + dir * d;
        self
    }

    /// Override vertical field-of-view in degrees.
    pub fn fov(mut self, deg: f32) -> Self {
        self.inner.fov_deg = deg;
        self
    }

    pub fn build(self) -> Camera3D { self.inner }
}

impl From<Camera3DBuilder> for Camera3D {
    fn from(b: Camera3DBuilder) -> Self { b.build() }
}

// ────────────────────────────────────────────────────────────────────────────
// OrbitCamera

/// Orbit / arc-ball camera controller.
///
/// Attach alongside a [`Camera3D`].  `OrbitCameraSystem` updates the eye each frame.
///
/// ```rust,ignore
/// world.ecs.spawn((
///     Camera3D::looking_at(Vec3::ZERO).build(),
///     OrbitCamera { yaw: -0.52, pitch: 0.35, distance: 5.0, target: Vec3::ZERO },
/// ));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: glam::Vec3,
}
impl Component for OrbitCamera {}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self { yaw: 0.0, pitch: 0.2, distance: 5.0, target: glam::Vec3::ZERO }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// OrbitCameraSystem

/// Updates [`Camera3D`] eye position from [`OrbitCamera`] state each frame.
///
/// Register at `Stage::Update`.
pub struct OrbitCameraSystem;

impl System for OrbitCameraSystem {
    fn name(&self) -> &'static str { "OrbitCameraSystem" }

    fn run(&mut self, world: &mut ferrous_ecs::world::World, _resources: &mut ResourceMap) {
        let updates: Vec<(ferrous_ecs::entity::Entity, glam::Vec3)> = world
            .query2::<OrbitCamera, Camera3D>()
            .map(|(e, orbit, _cam)| {
                let cy = orbit.pitch.cos();
                let sy = orbit.pitch.sin();
                let offset = glam::Vec3::new(
                    orbit.yaw.sin() * cy,
                    sy,
                    orbit.yaw.cos() * cy,
                ) * orbit.distance;
                (e, orbit.target + offset)
            })
            .collect();

        for (entity, new_eye) in updates {
            if let Some(cam) = world.get_mut::<Camera3D>(entity) {
                cam.eye = new_eye;
            }
        }
    }
}
