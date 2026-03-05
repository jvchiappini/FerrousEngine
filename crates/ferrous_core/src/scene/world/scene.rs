//! `World` struct definition and core mutation methods.
//!
//! Spawning, despawn, position, rotation and scale live here.
//! Material, visibility, tags, access and iteration live in [`super::query`].

use ferrous_ecs::prelude::{Entity, World as EcsWorld};
use glam::{Vec3};

use crate::scene::{MaterialDescriptor, MaterialHandle, MATERIAL_DEFAULT};
use crate::transform::Transform;
use crate::color::Color;

use super::types::{
    Element, ElementKind, Handle, MaterialComponent, PointLightComponent, next_id,
};
use super::builder::EntityBuilder;

// ─── World ──────────────────────────────────────────────────────────────────

/// The primary scene container.
///
/// Store one `World` on your application state, mutate it in `update()`,
/// and pass it to `renderer.sync_world(&world)` once per frame.
#[derive(Debug)]
pub struct World {
    pub(super) entities: Vec<Option<Element>>,
    pub(super) count: usize,

    /// The ECS world for component-based queries.
    pub ecs: EcsWorld,
    /// Map from legacy `Handle` ID to ECS `Entity`.
    pub ecs_mapping: std::collections::HashMap<u64, Entity>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// Creates an empty world.
    pub fn new() -> Self {
        Self {
            entities: Vec::new(),
            count: 0,
            ecs: EcsWorld::new(),
            ecs_mapping: std::collections::HashMap::new(),
        }
    }

    // ── Spawning ────────────────────────────────────────────────────────────

    /// Begin building a new entity with the given name.
    ///
    /// ```rust,ignore
    /// let h = world.spawn("Crate")
    ///     .with_kind(ElementKind::Cube { half_extents: glam::Vec3::splat(0.5) })
    ///     .with_color(Color::rgb(0.8, 0.5, 0.2))
    ///     .build();
    /// ```
    pub fn spawn(&mut self, name: impl Into<String>) -> EntityBuilder<'_> {
        let id = next_id();
        EntityBuilder::new(self, id, name)
    }

    /// Convenience: spawn a 1×1×1 cube at the given position.
    pub fn spawn_cube(&mut self, name: impl Into<String>, position: Vec3) -> Handle {
        let he = Vec3::splat(0.5);
        self.spawn(name)
            .with_kind(ElementKind::Cube { half_extents: he })
            .with_position(position)
            .with_scale(he)
            .build()
    }

    /// Convenience: spawn a 2‑D quad at the given position.
    pub fn spawn_quad(
        &mut self,
        name: impl Into<String>,
        position: Vec3,
        width: f32,
        height: f32,
        double_sided: bool,
    ) -> Handle {
        let scale = Vec3::new(width * 0.5, height * 0.5, 1.0);
        self.spawn(name)
            .with_kind(ElementKind::Quad { width, height, double_sided })
            .with_position(position)
            .with_scale(scale)
            .build()
    }

    /// Convenience: spawn a UV sphere at the given position.
    pub fn spawn_sphere(
        &mut self,
        name: impl Into<String>,
        position: Vec3,
        radius: f32,
        segments: u32,
    ) -> Handle {
        let lat = segments.max(2);
        let lon = segments.max(3);
        self.spawn(name)
            .with_kind(ElementKind::Sphere { radius, latitudes: lat, longitudes: lon })
            .with_position(position)
            .with_scale(Vec3::splat(radius))
            .build()
    }

    /// Spawn an invisible point-light entity at `position`.
    pub fn spawn_point_light(
        &mut self,
        name: impl Into<String>,
        position: Vec3,
        color: [f32; 3],
        intensity: f32,
        radius: f32,
    ) -> Handle {
        self.spawn(name)
            .with_position(position)
            .with_point_light(PointLightComponent { color, intensity, radius })
            .invisible()
            .build()
    }

    /// Convenience: spawn a box with explicit dimensions (width, height, depth).
    pub fn spawn_box(&mut self, name: impl Into<String>, position: Vec3, size: Vec3) -> Handle {
        let he = size * 0.5;
        self.spawn(name)
            .with_kind(ElementKind::Cube { half_extents: he })
            .with_position(position)
            .with_scale(he)
            .build()
    }

    /// Convenience: spawn a mesh identified by `asset_key`.
    pub fn spawn_mesh(
        &mut self,
        name: impl Into<String>,
        asset_key: impl Into<String>,
        position: Vec3,
    ) -> Handle {
        self.spawn(name)
            .with_kind(ElementKind::Mesh { asset_key: asset_key.into() })
            .with_position(position)
            .build()
    }

    // ── Despawn ─────────────────────────────────────────────────────────────

    /// Remove the entity from the world.  Returns `true` if it existed.
    pub fn despawn(&mut self, handle: Handle) -> bool {
        let idx = handle.0 as usize;
        if idx < self.entities.len() && self.entities[idx].is_some() {
            self.entities[idx] = None;
            if let Some(entity) = self.ecs_mapping.remove(&handle.0) {
                self.ecs.despawn(entity);
            }
            self.count -= 1;
            true
        } else {
            false
        }
    }

    // ── Position ────────────────────────────────────────────────────────────

    /// Overwrite the world-space position.
    pub fn set_position(&mut self, handle: Handle, pos: Vec3) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.transform.position = pos;
            if let Some(entity) = self.ecs_mapping.get(&handle.0) {
                if let Some(t) = self.ecs.get_mut::<Transform>(*entity) {
                    t.position = pos;
                }
                if let Some(elem) = self.ecs.get_mut::<Element>(*entity) {
                    elem.transform.position = pos;
                }
            }
        }
    }

    /// Read the world-space position.
    pub fn position(&self, handle: Handle) -> Option<Vec3> {
        self.entities
            .get(handle.0 as usize)
            .and_then(|o| o.as_ref())
            .map(|e| e.transform.position)
    }

    /// Read the full transform of an entity.
    pub fn transform(&self, handle: Handle) -> Option<Transform> {
        self.entities
            .get(handle.0 as usize)
            .and_then(|o| o.as_ref())
            .map(|e| e.transform)
    }

    /// Translate by an offset.
    pub fn translate(&mut self, handle: Handle, offset: Vec3) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.transform.position += offset;
        }
    }

    // ── Rotation ────────────────────────────────────────────────────────────

    /// Set rotation (quaternion).
    pub fn set_rotation(&mut self, handle: Handle, rot: glam::Quat) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.transform.rotation = rot;
        }
    }

    /// Rotate an entity around a world-space pivot.
    pub fn rotate_around(&mut self, handle: Handle, pivot: Vec3, axis: Vec3, angle: f32) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.transform.rotate_around(pivot, axis, angle);
        }
    }

    /// Convenience: rotate around the world Z axis.
    pub fn rotate_around_z(&mut self, handle: Handle, pivot: Vec3, angle: f32) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.transform.rotate_around_z(pivot, angle);
        }
    }

    /// Rotate an entity about an arbitrary axis, preserving position.
    pub fn rotate_axis(&mut self, handle: Handle, axis: Vec3, angle: f32) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.transform.rotate_axis(axis, angle);
        }
    }

    /// Convenience: rotate about the world Y axis (yaw).
    pub fn rotate_y(&mut self, handle: Handle, angle: f32) {
        self.rotate_axis(handle, Vec3::Y, angle);
    }

    // ── Scale ───────────────────────────────────────────────────────────────

    /// Set uniform scale.
    pub fn set_scale_uniform(&mut self, handle: Handle, s: f32) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.transform.scale = Vec3::splat(s);
        }
    }

    /// Set non-uniform scale.
    pub fn set_scale(&mut self, handle: Handle, scale: Vec3) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.transform.scale = scale;
        }
    }

    /// Resize a `Cube` by changing its half-extents (and updating scale).
    pub fn set_cube_half_extents(&mut self, handle: Handle, half_extents: Vec3) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            if let ElementKind::Cube { half_extents: ref mut he } = e.kind {
                *he = half_extents;
            }
            e.transform.scale = half_extents;
        }
    }

    /// Resize a `Cube` by specifying full size (width, height, depth).
    pub fn set_cube_size(&mut self, handle: Handle, size: Vec3) {
        self.set_cube_half_extents(handle, size * 0.5);
    }
}
