/// The `RenderPass` trait — every stage in the rendering graph implements this.
///
/// ## Two-phase design
/// `prepare` → `execute` lets passes upload GPU data **before** opening a
/// `wgpu::RenderPass`, which is required because `write_buffer` is not allowed
/// while an encoder is recording a render pass.
///
/// ## Optional lifecycle hooks
/// `on_attach` and `on_resize` have default no-op implementations so simple
/// passes don't need to implement them.  No downcast is ever needed to call
/// renderer-level management methods.
use wgpu::{CommandEncoder, Device, Queue, TextureView};

use crate::graph::FramePacket;

pub trait RenderPass: Send + Sync + 'static {
    /// Short human-readable label used as the WGPU debug label.
    fn name(&self) -> &str;

    // ── Lifecycle hooks (all optional) ────────────────────────────────────

    /// Called once when the pass is registered with the renderer.
    ///
    /// Use this to allocate GPU resources that depend on the surface format or
    /// sample count (e.g. post-process pipelines that must match the target).
    #[allow(unused_variables)]
    fn on_attach(
        &mut self,
        device: &Device,
        queue: &Queue,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) {
    }

    /// Called whenever the render target dimensions change.
    ///
    /// Any pass that owns size-dependent GPU textures should recreate them here.
    /// This replaces the old downcast pattern entirely.
    #[allow(unused_variables)]
    fn on_resize(&mut self, device: &Device, queue: &Queue, width: u32, height: u32) {}

    // ── Required: per-frame loop ──────────────────────────────────────────

    /// Upload GPU data.  Called **before** `execute` each frame.
    ///
    /// Use this to call `queue.write_buffer(...)` or any other operation that
    /// requires the encoder to be idle.
    fn prepare(&mut self, device: &Device, queue: &Queue, packet: &FramePacket);

    /// Record draw commands into `encoder`.
    ///
    /// Implementations open their own `wgpu::RenderPass` scope here.
    ///
    /// - `color_view`     — color attachment (MSAA texture when active)
    /// - `resolve_target` — single-sample resolve target, or `None` without MSAA
    /// - `depth_view`     — depth attachment, or `None` for passes that skip depth
    fn execute(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        color_view: &TextureView,
        resolve_target: Option<&TextureView>,
        depth_view: Option<&TextureView>,
        packet: &FramePacket,
    );
}
