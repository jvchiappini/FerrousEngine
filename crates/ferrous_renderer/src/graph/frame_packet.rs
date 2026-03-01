/// Data bundle assembled once per frame and passed to every `RenderPass`.
///
/// ## Design
/// `FramePacket` is intentionally **open**: the core fields (`viewport`,
/// `camera`, `scene_objects`) are fixed, but any system can attach arbitrary
/// per-frame data via [`FramePacket::insert`] / [`FramePacket::get`].
///
/// This means `ferrous_gui`, a future particle system, or any user system
/// can deposit their data without the renderer core knowing about them:
///
/// ```rust,ignore
/// // Producer (app layer):
/// packet.insert(my_gui_batch);
/// // Consumer (UiPass::execute):
/// if let Some(batch) = packet.get::<MyGuiBatch>() { ... }
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

/// A single mesh draw call, fully resolved to GPU handles.
/// Used for manually-spawned objects with the legacy dynamic-uniform path.
pub struct DrawCommand {
    pub vertex_buffer: Arc<wgpu::Buffer>,
    pub index_buffer: Arc<wgpu::Buffer>,
    pub index_count: u32,
    /// Number of vertices in the mesh (used for render statistics).
    pub vertex_count: u32,
    pub index_format: wgpu::IndexFormat,
    /// Slot index inside the renderer-wide `ModelBuffer`.
    ///
    /// `WorldPass` converts this to a byte offset via `model_buf.offset(slot)`
    /// and supplies it as the dynamic offset to `set_bind_group(1, ...)`.
    pub model_slot: usize,
}

/// One instanced draw call: all instances share the same mesh buffers and
/// their matrices are packed contiguously in the `InstanceBuffer`.
///
/// The shader reads `instances[instance_index]` from the storage buffer.
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
    /// Legacy per-object draw calls (manually-spawned objects, dynamic-uniform path).
    pub scene_objects: Vec<DrawCommand>,
    /// Instanced draw calls assembled from World entities (one per unique mesh).
    pub instanced_objects: Vec<InstancedDrawCommand>,
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
            scene_objects: Vec::new(),
            instanced_objects: Vec::new(),
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
