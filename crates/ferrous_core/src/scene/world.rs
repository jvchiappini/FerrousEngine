//! Scene world — the primary container for all runtime objects.
//!
//! `World` uses a `HashMap` keyed by a monotonically-increasing `u64` ID so
//! that handles remain stable across insertions and removals.  This is the
//! type that the renderer, editor and game logic all share; no other crate
//! needs to implement its own scene graph.
//!
//! # Quick start
//! ```rust,ignore
//! use ferrous_core::{World, Element, Transform, Color};
//! use glam::Vec3;
//!
//! let mut world = World::new();
//!
//! let h = world.spawn("Player")
//!     .with_position(Vec3::new(0.0, 0.5, 0.0))
//!     .with_color(Color::CYAN)
//!     .build();
//!
//! world.set_position(h, Vec3::new(1.0, 0.0, 0.0));
//! world.despawn(h);
//! ```

use std::sync::atomic::{AtomicU64, Ordering};

use glam::{Quat, Vec3};

use crate::color::Color;
use crate::transform::Transform;

// ─── ID generation ─────────────────────────────────────────────────────────

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_id() -> u64 {
    ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

// ─── Handle ────────────────────────────────────────────────────────────────

/// Opaque handle referencing an entity inside a [`World`].
///
/// Handles are stable: removing other entities does not invalidate existing
/// handles.  A handle becomes invalid only after the entity it refers to is
/// despawned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle(pub u64);

// ─── Element kinds ─────────────────────────────────────────────────────────

/// The geometric or logical kind of an entity.
///
/// New variants can be added here without breaking existing code because
/// all entity-kind-specific logic is handled via `match` in individual
/// systems rather than inside `World` itself.
#[derive(Debug, Clone)]
pub enum ElementKind {
    /// A solid box defined by per-axis half-extents in world units.
    /// The full dimensions are `half_extents * 2` (width, height, depth).
    /// A solid box defined by per-axis half-extents in world units.
    /// The full dimensions are `half_extents * 2` (width, height, depth).
    Cube { half_extents: Vec3 },
    /// A flat 2‑D rectangle lying in the XY plane.  The renderer uses a
    /// unit quad mesh and applies the transform's scale to achieve the
    /// requested size; the AABB is computed from `width`/`height` so that
    /// frustum culling works even when the object is not uniformly scaled.
    ///
    /// `double_sided` controls whether the quad should be visible when the
    /// camera is looking at its back face.  This flag is propagated to the
    /// renderer and used to pick a pipeline with culling disabled.
    Quad {
        width: f32,
        height: f32,
        double_sided: bool,
    },
    /// An arbitrary triangle mesh identified by an asset path or key.
    Mesh { asset_key: String },
    /// A point light that illuminates the scene.
    PointLight { radius: f32, intensity: f32 },
    /// An empty entity used as a logical group or marker.
    Empty,
}

impl Default for ElementKind {
    fn default() -> Self {
        ElementKind::Empty
    }
}

// ─── Entity entry ──────────────────────────────────────────────────────────

/// Complete data for one scene entity.
///
/// All fields are public so both the editor and game code can read them
/// directly.  Mutations should go through `World` helper methods when a
/// method exists; use direct field access for one-off tweaks.
#[derive(Debug, Clone)]
pub struct Element {
    /// Unique, stable identifier (mirrors the HashMap key for convenience).
    pub id: u64,
    /// Human-readable label — shown in the editor's hierarchy panel.
    pub name: String,
    /// World-space transform (position, rotation, scale).
    pub transform: Transform,
    /// Visual tint / base colour used by the renderer.
    pub color: Color,
    /// Geometric / logical kind.
    pub kind: ElementKind,
    /// Whether the entity participates in rendering.
    pub visible: bool,
    /// Arbitrary string tags for custom game logic (e.g. "enemy", "trigger").
    pub tags: Vec<String>,
    /// Optional handle back into the renderer's object list (set by
    /// `Renderer::sync_world`; applications should not touch this field).
    pub render_handle: Option<usize>,
}

impl Element {
    fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            transform: Transform::default(),
            color: Color::WHITE,
            kind: ElementKind::default(),
            visible: true,
            tags: Vec::new(),
            render_handle: None,
        }
    }
}

// ─── Entity builder ────────────────────────────────────────────────────────

/// Fluent builder returned by [`World::spawn`].
///
/// Call `.build()` to insert the entity and receive its [`Handle`].
pub struct EntityBuilder<'a> {
    world: &'a mut World,
    element: Element,
}

impl<'a> EntityBuilder<'a> {
    pub fn with_position(mut self, pos: Vec3) -> Self {
        self.element.transform.position = pos;
        self
    }

    pub fn with_rotation(mut self, rot: Quat) -> Self {
        self.element.transform.rotation = rot;
        self
    }

    pub fn with_scale(mut self, scale: Vec3) -> Self {
        self.element.transform.scale = scale;
        self
    }

    pub fn with_transform(mut self, t: Transform) -> Self {
        self.element.transform = t;
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.element.color = color;
        self
    }

    pub fn with_kind(mut self, kind: ElementKind) -> Self {
        self.element.kind = kind;
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.element.tags.push(tag.into());
        self
    }

    pub fn invisible(mut self) -> Self {
        self.element.visible = false;
        self
    }

    /// Finalise the builder, insert the entity, and return its handle.
    pub fn build(self) -> Handle {
        let id = self.element.id;
        let idx = id as usize;
        if idx >= self.world.entities.len() {
            self.world.entities.resize(idx + 1, None);
        }
        self.world.entities[idx] = Some(self.element);
        self.world.count += 1;
        Handle(id)
    }
}

// ─── World ──────────────────────────────────────────────────────────────────
/// The primary scene container.
///
/// Store one `World` on your application state, mutate it in `update()`,
/// and pass it to `renderer.sync_world(&world)` once per frame.
#[derive(Debug, Default)]
pub struct World {
    entities: Vec<Option<Element>>,
    count: usize,
}

impl World {
    /// Creates an empty world.
    pub fn new() -> Self {
        Self {
            entities: Vec::new(),
            count: 0,
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
        EntityBuilder {
            world: self,
            element: Element::new(id, name),
        }
    }

    /// Convenience: spawn a 1×1×1 cube at the given position and return its handle.
    pub fn spawn_cube(&mut self, name: impl Into<String>, position: Vec3) -> Handle {
        let he = Vec3::splat(0.5);
        self.spawn(name)
            .with_kind(ElementKind::Cube { half_extents: he })
            .with_position(position)
            .with_scale(he)
            .build()
    }

    /// Convenience: spawn a 2‑D quad at the given position.  `width` and
    /// `height` are the total dimensions in world units; the spawn helper
    /// automatically sets the entity's scale accordingly so the built-in
    /// unit quad mesh (±1 coordinates) ends up the correct size.  The
    /// `double_sided` flag selects whether the quad should render from both
    /// sides or only the front face.
    pub fn spawn_quad(
        &mut self,
        name: impl Into<String>,
        position: Vec3,
        width: f32,
        height: f32,
        double_sided: bool,
    ) -> Handle {
        // scale is half extents because built-in mesh spans [-1,1]
        let scale = Vec3::new(width * 0.5, height * 0.5, 1.0);
        self.spawn(name)
            .with_kind(ElementKind::Quad {
                width,
                height,
                double_sided,
            })
            .with_position(position)
            .with_scale(scale)
            .build()
    }

    /// Convenience: spawn a box with explicit dimensions (width, height, depth)
    /// at the given position and return its handle.
    pub fn spawn_box(&mut self, name: impl Into<String>, position: Vec3, size: Vec3) -> Handle {
        let he = size * 0.5;
        self.spawn(name)
            .with_kind(ElementKind::Cube { half_extents: he })
            .with_position(position)
            .with_scale(he)
            .build()
    }

    // ── Despawn ─────────────────────────────────────────────────────────────
    /// Remove the entity from the world.  Returns `true` if it existed.
    pub fn despawn(&mut self, handle: Handle) -> bool {
        let idx = handle.0 as usize;
        if idx < self.entities.len() && self.entities[idx].is_some() {
            self.entities[idx] = None;
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
        }
    }

    /// Read the world-space position.
    pub fn position(&self, handle: Handle) -> Option<Vec3> {
        self.entities
            .get(handle.0 as usize)
            .and_then(|o| o.as_ref())
            .map(|e| e.transform.position)
    }

    /// Read the full transform (position/rotation/scale) of an entity.
    ///
    /// This is mostly a convenience for editor code; most gameplay logic only
    /// needs position or individual components.  Returning a copy keeps the
    /// world borrow-free.
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
    pub fn set_rotation(&mut self, handle: Handle, rot: Quat) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.transform.rotation = rot;
        }
    }

    /// Rotate an entity around a given world-space pivot.
    ///
    /// This is a thin wrapper around [`Transform::rotate_around`]; it updates
    /// both the `position` and `rotation` fields of the underlying
    /// transform.  If the handle is invalid or the entity has been despawned
    /// this method is a no-op.
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

    /// Rotate an entity about an arbitrary world-space axis, preserving the
    /// entity's current position (i.e. not pivoting).
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

    /// Set non-uniform scale (x, y, z).
    pub fn set_scale(&mut self, handle: Handle, scale: Vec3) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.transform.scale = scale;
        }
    }

    /// Resize a `Cube` entity by changing its half-extents (and updating scale).
    /// `half_extents` = half of (width, height, depth).
    pub fn set_cube_half_extents(&mut self, handle: Handle, half_extents: Vec3) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            if let ElementKind::Cube {
                half_extents: ref mut he,
            } = e.kind
            {
                *he = half_extents;
            }
            e.transform.scale = half_extents;
        }
    }

    /// Resize a `Cube` entity by specifying its full size (width, height, depth).
    pub fn set_cube_size(&mut self, handle: Handle, size: Vec3) {
        self.set_cube_half_extents(handle, size * 0.5);
    }

    // ── Color ───────────────────────────────────────────────────────────────
    /// Change the visual tint of an entity.
    pub fn set_color(&mut self, handle: Handle, color: Color) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.color = color;
        }
    }

    // ── Visibility ──────────────────────────────────────────────────────────
    pub fn set_visible(&mut self, handle: Handle, visible: bool) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.visible = visible;
        }
    }

    // ── Tags ────────────────────────────────────────────────────────────────
    /// Add a string tag to an entity.
    pub fn add_tag(&mut self, handle: Handle, tag: impl Into<String>) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            let tag = tag.into();
            if !e.tags.contains(&tag) {
                e.tags.push(tag);
            }
        }
    }

    /// Returns true if the entity has the given tag.
    pub fn has_tag(&self, handle: Handle, tag: &str) -> bool {
        self.entities
            .get(handle.0 as usize)
            .and_then(|o| o.as_ref())
            .map(|e| e.tags.iter().any(|t| t == tag))
            .unwrap_or(false)
    }

    // ── Raw element access ──────────────────────────────────────────────────
    /// Immutable reference to an entity.
    pub fn get(&self, handle: Handle) -> Option<&Element> {
        self.entities
            .get(handle.0 as usize)
            .and_then(|o| o.as_ref())
    }

    /// Mutable reference to an entity — use this for complex multi-field
    /// updates to avoid multiple individual method calls.
    pub fn get_mut(&mut self, handle: Handle) -> Option<&mut Element> {
        self.entities
            .get_mut(handle.0 as usize)
            .and_then(|o| o.as_mut())
    }

    /// Returns `true` if the world contains this handle.
    pub fn contains(&self, handle: Handle) -> bool {
        let idx = handle.0 as usize;
        idx < self.entities.len() && self.entities[idx].is_some()
    }

    // ── Iteration ───────────────────────────────────────────────────────────
    /// Iterate over all entities.
    pub fn iter(&self) -> impl Iterator<Item = &Element> {
        self.entities.iter().filter_map(|o| o.as_ref())
    }

    /// Mutably iterate over all entities.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Element> {
        self.entities.iter_mut().filter_map(|o| o.as_mut())
    }

    /// Iterate over all entities that carry the given tag.
    pub fn iter_tagged<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Element> {
        self.iter().filter(move |e| e.tags.iter().any(|t| t == tag))
    }

    /// Iterate over `(Handle, &Element)` pairs.
    pub fn iter_with_handles(&self) -> impl Iterator<Item = (Handle, &Element)> {
        self.entities
            .iter()
            .enumerate()
            .filter_map(|(id, o)| o.as_ref().map(|e| (Handle(id as u64), e)))
    }

    /// Total number of entities currently alive.
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Returns the capacity of the world.
    pub fn capacity(&self) -> usize {
        self.entities.len()
    }

    // ── Renderer bridge ─────────────────────────────────────────────────────
    /// Internal: set the renderer handle for an entity.  Used by
    /// `ferrous_renderer::scene::sync_world`.
    pub fn set_render_handle(&mut self, handle: Handle, rh: usize) {
        if let Some(Some(e)) = self.entities.get_mut(handle.0 as usize) {
            e.render_handle = Some(rh);
        }
    }

    /// Internal: retrieve the renderer handle for an entity.
    pub fn render_handle(&self, handle: Handle) -> Option<usize> {
        self.entities
            .get(handle.0 as usize)
            .and_then(|o| o.as_ref())
            .and_then(|e| e.render_handle)
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_and_despawn() {
        let mut w = World::new();
        let h = w.spawn_cube("A", Vec3::ZERO);
        assert!(w.contains(h));
        assert_eq!(w.len(), 1);
        assert!(w.despawn(h));
        assert!(!w.contains(h));
        assert_eq!(w.len(), 0);
    }

    #[test]
    fn spawn_quad_behavior() {
        let mut w = World::new();
        let h = w.spawn_quad("Q", Vec3::ZERO, 2.0, 4.0, false);
        assert!(w.contains(h));
        // quad has width 2, height 4; the transform scale should be half
        // extents
        if let Some(Some(e)) = w.entities.get(h.0 as usize) {
            if let ElementKind::Quad {
                width,
                height,
                double_sided,
            } = e.kind.clone()
            {
                assert_eq!(width, 2.0);
                assert_eq!(height, 4.0);
                assert!(!double_sided);
            } else {
                panic!("wrong kind");
            }
        } else {
            panic!("entity missing");
        }
        assert_eq!(w.len(), 1);
        assert!(w.despawn(h));
    }

    #[test]
    fn position_roundtrip() {
        let mut w = World::new();
        let h = w.spawn_cube("B", Vec3::ZERO);
        w.set_position(h, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(w.position(h), Some(Vec3::new(1.0, 2.0, 3.0)));
    }

    #[test]
    fn rotate_entity_about_world_origin() {
        let mut w = World::new();
        let h = w.spawn_cube("R", Vec3::new(1.0, 0.0, 0.0));
        // rotate 90° about Z around the origin
        w.rotate_around(h, Vec3::ZERO, Vec3::Z, std::f32::consts::FRAC_PI_2);
        assert_eq!(w.position(h), Some(Vec3::new(0.0, 1.0, 0.0)));
        // also verify the rotation quaternion changed
        if let Some(e) = w.get(h) {
            let forward = e.transform.forward();
            assert!((forward - Vec3::NEG_Y).length() < 1e-5);
        } else {
            panic!("missing entity");
        }
    }

    #[test]
    fn rotate_z_helper_works() {
        let mut w = World::new();
        let h = w.spawn_cube("Z", Vec3::new(2.0, 0.0, 0.0));
        w.rotate_around_z(h, Vec3::new(1.0, 0.0, 0.0), std::f32::consts::PI);
        assert_eq!(w.position(h), Some(Vec3::new(0.0, 0.0, 0.0)));
    }

    #[test]
    fn rotate_axis_wrapper() {
        let mut w = World::new();
        let h = w.spawn_cube("A", Vec3::ZERO);
        w.rotate_axis(h, Vec3::Z, std::f32::consts::FRAC_PI_2);
        // orientation should have changed but position remains zero
        assert_eq!(w.position(h), Some(Vec3::ZERO));
        if let Some(e) = w.get(h) {
            let forward = e.transform.forward();
            assert!((forward - Vec3::NEG_Y).length() < 1e-5);
        }
    }

    #[test]
    fn rotate_y_wrapper() {
        let mut w = World::new();
        let h = w.spawn_cube("B", Vec3::ZERO);
        w.rotate_y(h, std::f32::consts::FRAC_PI_2);
        if let Some(e) = w.get(h) {
            // yaw 90° should make forward = -X
            let forward = e.transform.forward();
            assert!((forward - Vec3::NEG_X).length() < 1e-5);
        } else {
            panic!("missing entity");
        }
    }

    #[test]
    fn tags() {
        let mut w = World::new();
        let h = w.spawn("C").with_tag("enemy").build();
        assert!(w.has_tag(h, "enemy"));
        assert!(!w.has_tag(h, "player"));
        let enemies: Vec<_> = w.iter_tagged("enemy").collect();
        assert_eq!(enemies.len(), 1);
    }

    #[test]
    fn handles_are_stable_after_other_despawn() {
        let mut w = World::new();
        let h1 = w.spawn_cube("X", Vec3::ZERO);
        let h2 = w.spawn_cube("Y", Vec3::ONE);
        w.despawn(h1);
        assert!(w.contains(h2));
        assert_eq!(w.position(h2), Some(Vec3::ONE));
    }
}
