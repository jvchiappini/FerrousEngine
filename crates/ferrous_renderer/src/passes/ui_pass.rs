/// UI overlay pass — composites GUI quads and text on top of the scene.
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

    /// Override the `LoadOp` for the next frame(s).  Pass `Some(color)` to
    /// clear the target before drawing (use in `Flat2D` mode); pass `None`
    /// to restore the default `LoadOp::Load` behaviour (composite on top of
    /// the rendered scene).
    pub fn set_clear_color(&mut self, color: Option<wgpu::Color>) {
        self.clear_color = color;
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
        _device: &Device,
        _queue: &Queue,
        encoder: &mut CommandEncoder,
        color_view: &TextureView,
        resolve_target: Option<&TextureView>,
        _depth_view: Option<&TextureView>,
        packet: &FramePacket,
    ) {
        let ui_batch = packet.get::<GuiBatch>();
        let empty = GuiBatch::new();
        let batch = ui_batch.unwrap_or(&empty);

        if let Some(clear) = self.clear_color {
            // Even if there's no UI, we must clear the screen if a clear color is set.
            self.renderer.render(
                encoder,
                color_view,
                resolve_target,
                batch,
                wgpu::LoadOp::Clear(clear),
            );
        } else {
            // Only render if we have a batch and no clear is required (compositing).
            if !batch.is_empty() {
                self.renderer.render(
                    encoder,
                    color_view,
                    resolve_target,
                    batch,
                    wgpu::LoadOp::Load,
                );
            }
        }
    }
}
