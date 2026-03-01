/// Data bundle assembled once per frame and passed immutably to every
/// `RenderPass`.
///
/// Building a `FramePacket` on the CPU side decouples the scene/logic layer
/// from the GPU passes: passes only see what they need to render, not how the
/// scene is structured.
use std::sync::Arc;

use glam::{Mat4, Vec3};
use ferrous_gui::{GuiBatch, TextBatch};

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
    pub index_buffer:  Arc<wgpu::Buffer>,
    pub index_count:   u32,
    pub index_format:  wgpu::IndexFormat,
    /// Per-object model matrix bind group (group 1).
    pub model_bind_group: Arc<wgpu::BindGroup>,
}

// ── Viewport ──────────────────────────────────────────────────────────────────

/// Rectangular region within the render target used for 3-D content.
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
    pub viewport:      Option<Viewport>,
    pub camera:        CameraPacket,
    pub scene_objects: Vec<DrawCommand>,
    pub ui_batch:      Option<GuiBatch>,
    pub text_batch:    Option<TextBatch>,
}
