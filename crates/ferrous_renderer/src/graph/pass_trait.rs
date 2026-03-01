/// The `RenderPass` trait — every stage in the rendering graph implements this.
///
/// The two-phase design (`prepare` → `execute`) lets passes upload GPU data
/// **before** opening a `wgpu::RenderPass`, which is required because
/// `write_buffer` is not allowed while an encoder is recording a render pass.
use crate::graph::FramePacket;
use wgpu::{CommandEncoder, Device, Queue, TextureView};

pub trait RenderPass: std::any::Any {
    /// Short human-readable label used as the WGPU debug label.
    fn name(&self) -> &str;

    // ── Required for downcast ─────────────────────────────────────────────
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// Upload GPU data.  Called **before** `execute` each frame.
    ///
    /// Use this to call `queue.write_buffer(...)` or any other operation that
    /// requires the encoder to be idle.
    fn prepare(&mut self, device: &Device, queue: &Queue, packet: &FramePacket);

    /// Record draw commands into `encoder`.
    ///
    /// Implementations open their own `wgpu::RenderPass` scope here.
    /// Record draw commands into `encoder`.
    ///
    /// - `color_view`     — color attachment (MSAA texture when active)
    /// - `resolve_target` — single-sample resolve target, or `None` without MSAA
    /// - `depth_view`     — depth attachment, or `None` for passes that skip depth
    fn execute(
        &self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        color_view: &TextureView,
        resolve_target: Option<&TextureView>,
        depth_view: Option<&TextureView>,
        packet: &FramePacket,
    );
}
