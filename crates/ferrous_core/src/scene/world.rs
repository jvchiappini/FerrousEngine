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

use std::collections::HashMap;
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
    /// A solid box defined by a half-extent in world units.
    Cube { half_extent: f32 },
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
        self.world.entities.insert(id, self.element);
        Handle(id)
    }
}

// ─── World ─────────────────────────────────────────────────────────────────

/// The primary scene container.
///
/// Store one `World` on your application state, mutate it in `update()`,
/// and pass it to `renderer.sync_world(&world)` once per frame.
#[derive(Debug, Default)]
pub struct World {
    entities: HashMap<u64, Element>,
}

impl World {
    /// Creates an empty world.
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
        }
    }

    // ── Spawning ───────────────────────────────────────────────────────────

    /// Begin building a new entity with the given name.
    ///
    /// ```rust,ignore
    /// let h = world.spawn("Crate")
    ///     .with_kind(ElementKind::Cube { half_extent: 0.5 })
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

    /// Convenience: spawn a unit cube at the given position and return its handle.
    pub fn spawn_cube(&mut self, name: impl Into<String>, position: Vec3) -> Handle {
        self.spawn(name)
            .with_kind(ElementKind::Cube { half_extent: 0.5 })
            .with_position(position)
            .build()
    }

    // ── Despawn ────────────────────────────────────────────────────────────

    /// Remove the entity from the world.  Returns `true` if it existed.
    pub fn despawn(&mut self, handle: Handle) -> bool {
        self.entities.remove(&handle.0).is_some()
    }

    // ── Position ───────────────────────────────────────────────────────────

    /// Overwrite the world-space position.
    pub fn set_position(&mut self, handle: Handle, pos: Vec3) {
        if let Some(e) = self.entities.get_mut(&handle.0) {
            e.transform.position = pos;
        }
    }

    /// Read the world-space position.
    pub fn position(&self, handle: Handle) -> Option<Vec3> {
        self.entities.get(&handle.0).map(|e| e.transform.position)
    }

    /// Translate by an offset.
    pub fn translate(&mut self, handle: Handle, offset: Vec3) {
        if let Some(e) = self.entities.get_mut(&handle.0) {
            e.transform.position += offset;
        }
    }

    // ── Rotation ───────────────────────────────────────────────────────────

    /// Set rotation (quaternion).
    pub fn set_rotation(&mut self, handle: Handle, rot: Quat) {
        if let Some(e) = self.entities.get_mut(&handle.0) {
            e.transform.rotation = rot;
        }
    }

    // ── Scale ──────────────────────────────────────────────────────────────

    /// Set uniform scale.
    pub fn set_scale_uniform(&mut self, handle: Handle, s: f32) {
        if let Some(e) = self.entities.get_mut(&handle.0) {
            e.transform.scale = Vec3::splat(s);
        }
    }

    // ── Color ──────────────────────────────────────────────────────────────

    /// Change the visual tint of an entity.
    pub fn set_color(&mut self, handle: Handle, color: Color) {
        if let Some(e) = self.entities.get_mut(&handle.0) {
            e.color = color;
        }
    }

    // ── Visibility ─────────────────────────────────────────────────────────

    pub fn set_visible(&mut self, handle: Handle, visible: bool) {
        if let Some(e) = self.entities.get_mut(&handle.0) {
            e.visible = visible;
        }
    }

    // ── Tags ───────────────────────────────────────────────────────────────

    /// Add a string tag to an entity.
    pub fn add_tag(&mut self, handle: Handle, tag: impl Into<String>) {
        if let Some(e) = self.entities.get_mut(&handle.0) {
            let tag = tag.into();
            if !e.tags.contains(&tag) {
                e.tags.push(tag);
            }
        }
    }

    /// Returns true if the entity has the given tag.
    pub fn has_tag(&self, handle: Handle, tag: &str) -> bool {
        self.entities
            .get(&handle.0)
            .map(|e| e.tags.iter().any(|t| t == tag))
            .unwrap_or(false)
    }

    // ── Raw element access ─────────────────────────────────────────────────

    /// Immutable reference to an entity.
    pub fn get(&self, handle: Handle) -> Option<&Element> {
        self.entities.get(&handle.0)
    }

    /// Mutable reference to an entity — use this for complex multi-field
    /// updates to avoid multiple individual method calls.
    pub fn get_mut(&mut self, handle: Handle) -> Option<&mut Element> {
        self.entities.get_mut(&handle.0)
    }

    /// Returns `true` if the world contains this handle.
    pub fn contains(&self, handle: Handle) -> bool {
        self.entities.contains_key(&handle.0)
    }

    // ── Iteration ──────────────────────────────────────────────────────────

    /// Iterate over all entities.
    pub fn iter(&self) -> impl Iterator<Item = &Element> {
        self.entities.values()
    }

    /// Mutably iterate over all entities.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Element> {
        self.entities.values_mut()
    }

    /// Iterate over all entities that carry the given tag.
    pub fn iter_tagged<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Element> {
        self.entities
            .values()
            .filter(move |e| e.tags.iter().any(|t| t == tag))
    }

    /// Iterate over `(Handle, &Element)` pairs.
    pub fn iter_with_handles(&self) -> impl Iterator<Item = (Handle, &Element)> {
        self.entities.iter().map(|(&id, e)| (Handle(id), e))
    }

    /// Total number of entities currently alive.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    // ── Renderer bridge ────────────────────────────────────────────────────

    /// Internal: set the renderer handle for an entity.  Used by
    /// `ferrous_renderer::scene::sync_world`.
    pub fn set_render_handle(&mut self, handle: Handle, rh: usize) {
        if let Some(e) = self.entities.get_mut(&handle.0) {
            e.render_handle = Some(rh);
        }
    }

    /// Internal: retrieve the renderer handle for an entity.
    pub fn render_handle(&self, handle: Handle) -> Option<usize> {
        self.entities.get(&handle.0).and_then(|e| e.render_handle)
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
    fn position_roundtrip() {
        let mut w = World::new();
        let h = w.spawn_cube("B", Vec3::ZERO);
        w.set_position(h, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(w.position(h), Some(Vec3::new(1.0, 2.0, 3.0)));
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

    #[test]
    fn model_matrix_identity_at_origin() {
        let mut w = World::new();
        let h = w.spawn_cube("M", Vec3::ZERO);
        let m = w.get(h).unwrap().transform.matrix();
        assert!((m - Mat4::IDENTITY).abs_diff_eq(Mat4::ZERO, 1e-6));
    }
}
