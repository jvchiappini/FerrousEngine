/// Data bundle assembled once per frame and passed to every `RenderPass`.
///
/// ## Design
/// `FramePacket` is intentionally **open**: the core fields (`viewport`,
/// `camera`, `instanced_objects`) are fixed, but any system can attach arbitrary
/// per-frame data via [`FramePacket::insert`] / [`FramePacket::get`].
///
/// This means `ferrous_gui`, a future particle system, or any user system
/// can deposit their data without the renderer core knowing about them:
///
/// ```rust,ignore
/// // Producer (app layer):
/// packet.insert(my_gui_batch);
/// // Consumer (UiPass::execute):
/// if let Some(batch) = packet.get::<MyGuiBatch>() {
///     // handle batch here
/// }
/// ```
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Vec3};

// ── Camera ────────────────────────────────────────────────────────────────────

/// Snapshot of camera state for a single frame.
pub struct CameraPacket {
    pub view_proj: Mat4,
    pub eye: Vec3,
}

// ── 3-D scene ─────────────────────────────────────────────────────────────────

/// One instanced draw call: all instances share the same mesh buffers and
/// their matrices are packed contiguously in the `InstanceBuffer`.
///
/// The shader reads `instances[instance_index]` from the storage buffer.
#[derive(Clone)]
pub struct InstancedDrawCommand {
    pub vertex_buffer: Arc<wgpu::Buffer>,
    pub index_buffer: Arc<wgpu::Buffer>,
    pub index_count: u32,
    /// Number of vertices per mesh instance (used for render statistics).
    pub vertex_count: u32,
    pub index_format: wgpu::IndexFormat,
    /// Index of the first matrix in the `InstanceBuffer` for this batch.
    pub first_instance: u32,
    /// Number of instances in this batch.
    pub instance_count: u32,
    /// Whether the draw should disable back-face culling.
    pub double_sided: bool,
    /// Material slot for all instances in this batch.  If you need
    /// per-instance materials the caller must split them into multiple
    /// batches (current implementation doesn't support an array of materials).
    pub material_slot: usize,
    /// Representative squared distance from the camera for this batch of
    /// instances.  When building the packet we compute the maximum distance
    /// of any instance in the batch; this gives a safe ordering for
    /// translucent instanced geometry (farther batches are drawn first).
    pub distance_sq: f32,
}

// ── Viewport ──────────────────────────────────────────────────────────────────

/// Re-exported from `ferrous_core` — kept here so existing renderer-internal
/// code that imports `crate::graph::frame_packet::Viewport` still compiles.
pub use ferrous_core::Viewport;

// ── Frame packet ──────────────────────────────────────────────────────────────

/// All data a `RenderPass` may need for one frame.
pub struct FramePacket {
    pub viewport: Option<Viewport>,
    pub camera: CameraPacket,
    /// Instanced draw calls assembled from World entities (one per unique mesh).
    pub instanced_objects: Vec<InstancedDrawCommand>,
    /// Shadow-caster instanced draw calls for World entities.
    ///
    /// These reference a separate section of the shadow instance buffer
    /// (starting at `first_instance` offsets within that buffer), not the
    /// main camera-visible instance buffer.
    pub shadow_instanced_objects: Vec<InstancedDrawCommand>,
    /// Open-ended per-frame data keyed by `TypeId`.
    ///
    /// Any system inserts its batch/data here; any pass retrieves it by type.
    /// This keeps the renderer decoupled from GUI, particles, etc.
    extras: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl FramePacket {
    /// Creates an empty packet.
    pub fn new(viewport: Option<Viewport>, camera: CameraPacket) -> Self {
        Self {
            viewport,
            camera,
            instanced_objects: Vec::new(),
            shadow_instanced_objects: Vec::new(),
            extras: HashMap::new(),
        }
    }

    /// Inserts (or replaces) a typed value into the extras map.
    pub fn insert<T: Any + Send + Sync>(&mut self, val: T) {
        self.extras.insert(TypeId::of::<T>(), Box::new(val));
    }

    /// Returns a shared reference to `T`, or `None`.
    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.extras.get(&TypeId::of::<T>())?.downcast_ref::<T>()
    }

    /// Returns a mutable reference to `T`, or `None`.
    pub fn get_mut<T: Any + Send + Sync>(&mut self) -> Option<&mut T> {
        self.extras.get_mut(&TypeId::of::<T>())?.downcast_mut::<T>()
    }

    /// Returns `true` if a value of type `T` is present.
    pub fn contains<T: Any + Send + Sync>(&self) -> bool {
        self.extras.contains_key(&TypeId::of::<T>())
    }
}
