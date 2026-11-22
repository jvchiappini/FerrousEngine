/// UI overlay pass â€” composites GUI quads and text on top of the scene.
///
/// Reads `ferrous_ui_render::GuiBatch` from the
/// `FramePacket` extras map (inserted by the app layer).  If it is
/// not present the pass is a no-op that frame.
///
/// The `on_resize` hook keeps the GUI projection matrix in sync without
/// needing any downcast from the `Renderer`.
#[cfg(feature = "gui")]
use wgpu::{CommandEncoder, Device, Queue, TextureView};

use ferrous_ui_render::{GuiBatch, GuiRenderer};

use crate::graph::{FramePacket, RenderPass};

pub struct UiPass {
    renderer: GuiRenderer,
    /// When `Some`, the pass clears the target to this colour before drawing
    /// the UI.  Set by the renderer when `RendererMode::Flat2D` is active
    /// so that the world / post-process passes can be skipped entirely.
    clear_color: Option<wgpu::Color>,
}

impl UiPass {
    pub fn new(renderer: GuiRenderer) -> Self {
        Self {
            renderer,
            clear_color: None,
        }
    }

    pub fn set_font_atlas(&mut self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) {
        self.renderer.set_font_atlas(view, sampler);
    }

    /// Supplies the scene colour view the GUI backdrop blur reads.
    ///
    /// Pass a `SceneTarget2d::view()` (or any sampled colour view of the same
    /// size) that already holds the current scene. `ferrous_ui_render` does not
    /// depend on `ferrous_renderer`; this is the caller-provided view.
    pub fn set_backdrop_source(&mut self, view: &wgpu::TextureView, width: u32, height: u32) {
        self.renderer.set_backdrop_source(view, width, height);
    }

    /// Enables or disables the GUI depth attachment (prompt 18 P5).
    pub fn set_ui_depth(&mut self, enabled: bool) {
        self.renderer.set_ui_depth(enabled);
    }

    /// Enables the overdraw debug view for the GUI pass (prompt 18 P5).
    pub fn set_overdraw(&mut self, enabled: bool) {
        self.renderer.set_overdraw(enabled);
    }

    /// Per-frame GPU work counters of the last GUI pass (prompt 18 P5).
    pub fn frame_counters(&self) -> ferrous_ui_render::UiFrameCounters {
        self.renderer.frame_counters()
    }

    /// Toggle the UI image bind-group cache (audit H8).  Forwarded to the
    /// underlying [`GuiRenderer`].
    pub fn set_bind_group_cache(&mut self, enabled: bool) {
        self.renderer.set_image_bind_group_cache(enabled);
    }

    /// Override the `LoadOp` for the next frame(s).  Pass `Some(color)` to
    /// clear the target before drawing (use in `Flat2D` mode); pass `None`
    /// to restore the default `LoadOp::Load` behaviour (composite on top of
    /// the rendered scene).
    pub fn set_clear_color(&mut self, color: Option<wgpu::Color>) {
        self.clear_color = color;
    }

    pub fn update_output_config(
        &mut self,
        queue: &Queue,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) {
        self.renderer
            .update_output_config(queue, width, height, format, sample_count);
    }

    /// [`RenderPass::execute`] with an optional GPU timestamp pair
    /// (prompt 18 P0).  The timing must outlive the render pass; the caller
    /// keeps it in scope around this call.
    #[allow(clippy::too_many_arguments)] // mirrors RenderPass::execute + timing
    pub fn execute_timed(
        &mut self,
        _device: &Device,
        _queue: &Queue,
        encoder: &mut CommandEncoder,
        color_view: &TextureView,
        resolve_target: Option<&TextureView>,
        _depth_view: Option<&TextureView>,
        packet: &FramePacket,
        timing: Option<&crate::gpu_timing::PassTiming>,
    ) {
        let ui_batch = packet.get::<GuiBatch>();
        let empty = GuiBatch::new();
        let batch = ui_batch.unwrap_or(&empty);
        let timestamp_writes = timing.map(|timing| timing.writes());

        if let Some(clear) = self.clear_color {
            // Even if there's no UI, we must clear the screen if a clear color is set.
            self.renderer.render_with_timestamps(
                encoder,
                color_view,
                resolve_target,
                batch,
                wgpu::LoadOp::Clear(clear),
                timestamp_writes,
            );
        } else {
            // Only render if we have a batch and no clear is required (compositing).
            if !batch.is_empty() {
                self.renderer.render_with_timestamps(
                    encoder,
                    color_view,
                    resolve_target,
                    batch,
                    wgpu::LoadOp::Load,
                    timestamp_writes,
                );
            }
        }
    }
}

impl RenderPass for UiPass {
    fn name(&self) -> &str {
        "UI Overlay Pass"
    }

    /// Keeps the GUI projection matrix in sync with the render target size.
    fn on_resize(&mut self, _device: &Device, queue: &Queue, width: u32, height: u32) {
        self.renderer.resize(queue, width, height);
    }

    fn prepare(&mut self, _device: &Device, queue: &Queue, packet: &FramePacket) {
        if let Some(batch) = packet.get::<GuiBatch>() {
            self.renderer.prepare(queue, batch);
        }
    }

    fn execute(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        color_view: &TextureView,
        resolve_target: Option<&TextureView>,
        depth_view: Option<&TextureView>,
        packet: &FramePacket,
    ) {
        self.execute_timed(
            device,
            queue,
            encoder,
            color_view,
            resolve_target,
            depth_view,
            packet,
            None,
        );
    }
}
