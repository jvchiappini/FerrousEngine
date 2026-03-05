//! ECS component types and scene entity types for the scene world.

use std::sync::atomic::{AtomicU64, Ordering};

use ferrous_ecs::prelude::Component;
use glam::Vec3;

use crate::scene::{MaterialDescriptor, MaterialHandle, MATERIAL_DEFAULT};
use crate::transform::Transform;

// ── ID generation ────────────────────────────────────────────────────────────

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub(super) fn next_id() -> u64 {
    ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

// ── Handle ───────────────────────────────────────────────────────────────────

/// Opaque handle referencing an entity inside a [`super::World`].
///
/// Handles are stable — despawning other entities does not invalidate
/// existing handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle(pub u64);

// ── PointLightComponent ──────────────────────────────────────────────────────

/// Component for entities that emit point light.
///
/// Collected by `sync_world` each frame and uploaded to the GPU light buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointLightComponent {
    pub color: [f32; 3],
    pub intensity: f32,
    pub radius: f32,
}

impl Component for PointLightComponent {}

impl Default for PointLightComponent {
    fn default() -> Self {
        Self { color: [1.0, 1.0, 1.0], intensity: 5.0, radius: 10.0 }
    }
}

// ── MaterialComponent ────────────────────────────────────────────────────────

/// Material handle + CPU-side descriptor for an entity.
#[derive(Debug, Clone)]
pub struct MaterialComponent {
    pub handle: MaterialHandle,
    pub descriptor: MaterialDescriptor,
}

impl Component for MaterialComponent {}

impl Default for MaterialComponent {
    fn default() -> Self {
        Self { handle: MATERIAL_DEFAULT, descriptor: MaterialDescriptor::default() }
    }
}

// ── ElementKind ──────────────────────────────────────────────────────────────

/// Geometric or logical kind of a scene entity.
#[derive(Debug, Clone)]
pub enum ElementKind {
    Cube { half_extents: Vec3 },
    Quad { width: f32, height: f32, double_sided: bool },
    Sphere { radius: f32, latitudes: u32, longitudes: u32 },
    Mesh { asset_key: String },
    PointLight { radius: f32, intensity: f32 },
    Empty,
}

impl ferrous_ecs::prelude::Component for ElementKind {}

impl Default for ElementKind {
    fn default() -> Self { ElementKind::Empty }
}

// ── Element ──────────────────────────────────────────────────────────────────

/// Complete data for one scene entity.
#[derive(Debug, Clone)]
pub struct Element {
    pub id: u64,
    pub name: String,
    pub transform: Transform,
    pub material: MaterialComponent,
    pub kind: ElementKind,
    pub visible: bool,
    pub tags: Vec<String>,
    pub render_handle: Option<usize>,
    pub point_light: Option<PointLightComponent>,
}

impl Component for Element {}

impl Element {
    pub(super) fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            transform: Transform::default(),
            material: MaterialComponent::default(),
            kind: ElementKind::default(),
            visible: true,
            tags: Vec::new(),
            render_handle: None,
            point_light: None,
        }
    }
}

