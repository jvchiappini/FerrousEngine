#[cfg(feature = "ecs")]
/// ECS component types and scene entity types for the scene world.

// `Element` type is defined later in this file; no need to import it.
use std::sync::atomic::{AtomicU64, Ordering};

use ferrous_ecs::prelude::Component;
use glam::Vec3;

use crate::scene::{MaterialDescriptor, MaterialHandle, MATERIAL_DEFAULT};
use crate::transform::Transform;
use serde::{Serialize, Deserialize};

// ── ID generation ────────────────────────────────────────────────────────────

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub(super) fn next_id() -> u64 {
    ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Marker component for entities that cast shadows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShadowCaster;

impl Component for ShadowCaster {}

/// Rendering constraint for entities that always face the camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BillboardMode {
    Spherical,
    Cylindrical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Billboard {
    pub mode: BillboardMode,
}

impl Component for Billboard {}

// ── Handle ───────────────────────────────────────────────────────────────────

/// Opaque handle referencing an entity inside a [`super::World`].
///
/// Handles are stable — despawning other entities does not invalidate
/// existing handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Handle(pub u64);

// ── PointLightComponent ──────────────────────────────────────────────────────

/// Component for entities that emit point light.
///
/// Collected by `sync_world` each frame and uploaded to the GPU light buffer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointLightComponent {
    pub color: [f32; 3],
    pub intensity: f32,
    pub radius: f32,
}

impl Component for PointLightComponent {}

impl Default for PointLightComponent {
    fn default() -> Self {
        Self {
            color: [1.0, 1.0, 1.0],
            intensity: 5.0,
            radius: 10.0,
        }
    }
}

// ── MaterialComponent ────────────────────────────────────────────────────────

/// Material handle + CPU-side descriptor for an entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialComponent {
    pub handle: MaterialHandle,
    pub descriptor: MaterialDescriptor,
}

impl Component for MaterialComponent {}

impl Default for MaterialComponent {
    fn default() -> Self {
        Self {
            handle: MATERIAL_DEFAULT,
            descriptor: MaterialDescriptor::default(),
        }
    }
}

// ── ElementKind ──────────────────────────────────────────────────────────────

/// Geometric or logical kind of a scene entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub enum ElementKind {
    // ── Existing ────────────────────────────────────────────────────────────
    Cube {
        half_extents: Vec3,
    },
    Quad {
        width: f32,
        height: f32,
        double_sided: bool,
    },
    Sphere {
        radius: f32,
        latitudes: u32,
        longitudes: u32,
    },
    Mesh {
        asset_key: String,
    },
    PointLight {
        radius: f32,
        intensity: f32,
    },
    #[default]
    Empty,

    // ── New primitives ───────────────────────────────────────────────────────

    /// A cylinder, cone (top_radius = 0), or frustum.
    Cylinder {
        radius_top: f32,
        radius_bottom: f32,
        height: f32,
        /// Number of sides around the axis.
        radial_segments: u32,
        /// Horizontal ring subdivisions on the body.
        height_segments: u32,
        /// If true, no end caps are generated.
        open_ended: bool,
    },
    /// A torus (donut shape).
    Torus {
        radius: f32,
        tube: f32,
        radial_segments: u32,
        tubular_segments: u32,
    },
    /// A flat subdivided plane lying in the XZ plane (Y = 0).
    Plane {
        width: f32,
        height: f32,
        width_segments: u32,
        height_segments: u32,
    },
    /// A capsule: cylinder body capped with hemispheres.
    Capsule {
        radius: f32,
        /// Length of the cylindrical body (not including caps).
        height: f32,
        radial_segments: u32,
        cap_segments: u32,
    },
    /// A flat filled disc in the XZ plane.
    Circle {
        radius: f32,
        segments: u32,
    },
    /// A flat ring (annulus) in the XZ plane.
    Ring {
        inner_radius: f32,
        outer_radius: f32,
        segments: u32,
        rings: u32,
    },
    /// A 3D generated text mesh.
    Text3D {
        text: String,
        font_data: Vec<u8>,
        depth: f32,
        bevel_enabled: bool,
        bevel_thickness: f32,
        bevel_size: f32,
        quality: u8,
    },
}


impl ferrous_ecs::prelude::Component for ElementKind {}


// ── Element ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Element {
    pub id: u64,
    pub name: String,
    pub transform: Transform,
    pub material: MaterialComponent,
    pub kind: ElementKind,
    pub visible: bool,
    pub tags: Vec<String>,
    #[serde(skip)]
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
