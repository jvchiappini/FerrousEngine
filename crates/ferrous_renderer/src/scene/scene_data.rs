//! Scene push API — renderer-side data types.
//!
//! These types form the contract between the application layer and the
//! renderer.  Instead of the renderer querying the ECS directly (via
//! `sync_world`), the application layer builds a [`SceneData`] each frame and
//! calls [`crate::Renderer::set_scene`].
//!
//! This severs the `ferrous_renderer → ferrous_core::scene` dependency:
//! the renderer only knows about plain-data structs; the ECS query lives in
//! `ferrous_app` where it belongs.

use glam::{Mat4, Vec3};

use crate::scene::Aabb;

// ---------------------------------------------------------------------------
// Camera

/// CPU-side camera parameters pushed to the renderer each frame.
///
/// Replaces the direct mutation of `renderer.camera_system.camera` from
/// `sync_world`.
#[derive(Debug, Clone, Copy)]
pub struct CameraData {
    /// World-space eye position.
    pub eye: Vec3,
    /// World-space look-at target.
    pub target: Vec3,
    /// Vertical field of view in **radians**.
    pub fov_y: f32,
    /// Near clip plane distance.
    pub z_near: f32,
    /// Far clip plane distance.
    pub z_far: f32,
}

impl Default for CameraData {
    fn default() -> Self {
        Self {
            eye: Vec3::new(0.0, 2.0, 5.0),
            target: Vec3::ZERO,
            fov_y: 60_f32.to_radians(),
            z_near: 0.1,
            z_far: 1000.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Directional light

/// A single directional light pushed to the renderer each frame.
#[derive(Debug, Clone, Copy)]
pub struct DirectionalLightData {
    /// World-space direction the light is travelling (points **toward** the light).
    pub direction: Vec3,
    /// Linear RGB colour of the light.
    pub color: Vec3,
    /// Intensity multiplier (1.0 = default sun brightness).
    pub intensity: f32,
}

impl Default for DirectionalLightData {
    fn default() -> Self {
        Self {
            direction: Vec3::new(0.5, -1.0, -0.5).normalize(),
            color: Vec3::ONE,
            intensity: 1.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Render instance

/// A single renderable mesh instance pushed to the renderer.
///
/// The renderer does **not** own a mesh handle — the caller must ensure the
/// mesh is already registered (e.g. via [`crate::Renderer`]'s mesh upload
/// methods).  `mesh_id` is the index returned at registration time.
#[derive(Debug, Clone)]
pub struct RenderInstance {
    /// Index of the mesh to draw (obtained from the mesh upload API).
    pub mesh_id: usize,
    /// World-space model matrix.
    pub transform: Mat4,
    /// Object-space AABB for frustum culling.
    pub local_aabb: Aabb,
    /// Material handle index (0 = default white).
    pub material_slot: usize,
    /// When `true`, the mesh is rendered without back-face culling.
    pub double_sided: bool,
    /// When `true`, the instance is included in the shadow pass.
    pub cast_shadow: bool,
}

// ---------------------------------------------------------------------------
// SceneData

/// A complete scene description for one frame, pushed by the application.
///
/// Build this struct in the application layer (e.g. from ECS queries in
/// `ferrous_app`) and call [`crate::Renderer::set_scene`] once per frame.
///
/// `sync_world` remains available for backward compatibility; it internally
/// converts ECS state into an equivalent `SceneData` and calls `set_scene`.
#[derive(Debug, Default, Clone)]
pub struct SceneData {
    /// Camera parameters for this frame.  `None` leaves the current camera
    /// state unchanged.
    pub camera: Option<CameraData>,
    /// Directional light for this frame.  `None` leaves the current light
    /// state unchanged.
    pub directional_light: Option<DirectionalLightData>,
    /// All renderable instances for this frame.
    pub instances: Vec<RenderInstance>,
}

impl SceneData {
    /// Create an empty `SceneData`.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set camera data (builder pattern).
    #[inline]
    pub fn with_camera(mut self, camera: CameraData) -> Self {
        self.camera = Some(camera);
        self
    }

    /// Set the directional light (builder pattern).
    #[inline]
    pub fn with_directional_light(mut self, light: DirectionalLightData) -> Self {
        self.directional_light = Some(light);
        self
    }

    /// Append a render instance (builder pattern).
    #[inline]
    pub fn push_instance(mut self, instance: RenderInstance) -> Self {
        self.instances.push(instance);
        self
    }
}
