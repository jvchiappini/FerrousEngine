//! World-space transform: position, rotation (quaternion), scale.
//!
//! `Transform` is `Copy` and `Default`, making it easy to embed in any
//! struct.  Call `.matrix()` to get the combined model matrix for upload
//! to the GPU.

use ferrous_ecs::prelude::Component;
use glam::{Mat4, Quat, Vec3};

/// World-space transform component.
///
/// # Example
/// ```rust,ignore
/// use ferrous_core::Transform;
/// use glam::{Quat, Vec3};
///
/// let t = Transform::from_position(Vec3::new(1.0, 0.0, 0.0));
/// let m = t.matrix(); // ready to upload as a model uniform
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// World-space position.
    pub position: Vec3,
    /// Orientation as a unit quaternion.
    pub rotation: Quat,
    /// Non-uniform scale factor.
    pub scale: Vec3,
}

impl Component for Transform {}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl Transform {
    /// Identity transform — no translation, no rotation, uniform scale 1.
    pub const IDENTITY: Self = Self {
        position: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    /// Construct with a world-space position, identity rotation and scale.
    pub fn from_position(position: Vec3) -> Self {
        Self {
            position,
            ..Default::default()
        }
    }

    /// Construct with a position and a look-at rotation.
    ///
    /// `target` — the point to face; `up` — world-up hint (usually `Vec3::Y`).
    pub fn looking_at(position: Vec3, target: Vec3, up: Vec3) -> Self {
        let dir = (target - position).normalize_or_zero();
        let _rotation = Quat::from_rotation_arc(Vec3::NEG_Z, dir);
        // If dir is zero (position == target) keep identity rotation
        let rotation = if dir.length_squared() < 1e-10 {
            Quat::IDENTITY
        } else {
            // Use glam's look_at to avoid singularities
            Mat4::look_at_rh(position, target, up)
                .to_scale_rotation_translation()
                .1
                .inverse()
        };
        Self {
            position,
            rotation,
            scale: Vec3::ONE,
        }
    }

    /// Build the TRS model matrix (`T * R * S`).
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }

    /// Apply a translation offset in world space.
    pub fn translate(&mut self, offset: Vec3) {
        self.position += offset;
    }

    /// Rotate by `angle` radians around the given world-space axis.
    pub fn rotate_axis(&mut self, axis: Vec3, angle: f32) {
        self.rotation = Quat::from_axis_angle(axis, angle) * self.rotation;
    }

    /// Rotate around the world Y axis (yaw).
    pub fn rotate_y(&mut self, angle: f32) {
        self.rotate_axis(Vec3::Y, angle);
    }

    /// Set uniform scale.
    pub fn set_scale_uniform(&mut self, s: f32) {
        self.scale = Vec3::splat(s);
    }

    /// Forward direction in world space (`−Z` rotated by the quaternion).
    pub fn forward(&self) -> Vec3 {
        self.rotation * Vec3::NEG_Z
    }

    /// Right direction in world space.
    pub fn right(&self) -> Vec3 {
        self.rotation * Vec3::X
    }

    /// Up direction in world space.
    pub fn up(&self) -> Vec3 {
        self.rotation * Vec3::Y
    }
}

// ---------------------------------------------------------------------------
// Convenience helpers for pivoted rotations
// ---------------------------------------------------------------------------
impl Transform {
    /// Rotate the object around an arbitrary point in **world space**.
    ///
    /// The standard `matrix()` implementation builds a TRS matrix where the
    /// translation component is effectively the point that will be rotated.
    /// In other words, the object is spun around its own origin.  If you want
    /// to rotate **around a different pivot** you must move the origin before
    /// and/or after applying the rotation.  This method encapsulates that
    /// logic for you.
    ///
    /// # Parameters
    ///
    /// * `pivot` – the point in world coordinates to rotate around.  Common
    ///   choices are `Vec3::ZERO` or the centre of another object.
    /// * `axis` – axis of rotation in world space (e.g. `Vec3::Z` for a 2‑D
    ///   quad lying in the XY plane).
    /// * `angle` – rotation in radians; positive means right‑handed about the
    ///   axis.
    ///
    /// After this call both `rotation` and `position` will have been updated.
    pub fn rotate_around(&mut self, pivot: Vec3, axis: Vec3, angle: f32) {
        let rot = Quat::from_axis_angle(axis, angle);
        // rotate the orientation first
        self.rotation = rot * self.rotation;

        // then move the position around the pivot
        let rel = self.position - pivot;
        self.position = pivot + (rot * rel);
    }

    /// Rotate around the Z axis with a world-space pivot.  This is the common
    /// case for 2‑D quads and UI elements.
    pub fn rotate_around_z(&mut self, pivot: Vec3, angle: f32) {
        self.rotate_around(pivot, Vec3::Z, angle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_matrix() {
        let t = Transform::default();
        assert!((t.matrix() - Mat4::IDENTITY).abs_diff_eq(Mat4::ZERO, 1e-6));
    }

    #[test]
    fn translation_only() {
        let t = Transform::from_position(Vec3::new(1.0, 2.0, 3.0));
        let m = t.matrix();
        let (_, _, pos) = m.to_scale_rotation_translation();
        assert!((pos - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-5);
    }

    #[test]
    fn rotate_around_pivot() {
        let mut t = Transform::from_position(Vec3::new(1.0, 0.0, 0.0));
        // rotate 90° about Z around the origin; object should move to (0,1,0)
        t.rotate_around(Vec3::ZERO, Vec3::Z, std::f32::consts::FRAC_PI_2);
        assert!((t.position - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-4,
            "position={:?}", t.position);
        // After 90° Z rotation: right (X) becomes Y, so up() should point toward NEG_X
        // right() = rotation * X => after 90° around Z, X maps to Y
        let right = t.right();
        assert!((right - Vec3::Y).length() < 1e-4,
            "right={:?}", right);
    }

    #[test]
    fn rotate_around_z_helper() {
        let mut t = Transform::from_position(Vec3::new(2.0, 0.0, 0.0));
        t.rotate_around_z(Vec3::new(1.0, 0.0, 0.0), std::f32::consts::PI);
        // point 1 unit to the right of pivot should flip to the left
        assert!((t.position - Vec3::new(0.0, 0.0, 0.0)).length() < 1e-5);
    }
}
