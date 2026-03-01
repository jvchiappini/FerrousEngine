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
pub struct DrawCommand {
    pub vertex_buffer: Arc<wgpu::Buffer>,
    pub index_buffer: Arc<wgpu::Buffer>,
    pub index_count: u32,
    pub index_format: wgpu::IndexFormat,
    /// Slot index inside the renderer-wide `ModelBuffer`.
    ///
    /// `WorldPass` converts this to a byte offset via `model_buf.offset(slot)`
    /// and supplies it as the dynamic offset to `set_bind_group(1, ...)`.
    pub model_slot: usize,
}

// ── Viewport ──────────────────────────────────────────────────────────────────

/// Rectangular region within the render target used for 3-D / 2-D content.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

// ── Frame packet ──────────────────────────────────────────────────────────────

/// All data a `RenderPass` may need for one frame.
pub struct FramePacket {
    pub viewport: Option<Viewport>,
    pub camera: CameraPacket,
    pub scene_objects: Vec<DrawCommand>,
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
