/// UI overlay pass — composites GUI quads and text on top of the scene.
///
/// Reads `ferrous_gui::GuiBatch` and `ferrous_gui::TextBatch` from the
/// `FramePacket` extras map (inserted by the app layer).  If neither is
/// present the pass is a no-op that frame.
///
/// The `on_resize` hook keeps the GUI projection matrix in sync without
/// needing any downcast from the `Renderer`.
#[cfg(feature = "gui")]
use wgpu::{CommandEncoder, Device, Queue, TextureView};

use ferrous_ui_render::{GuiBatch, GuiRenderer, TextBatch};

use crate::graph::{FramePacket, RenderPass};

pub struct UiPass {
    renderer: GuiRenderer,
    /// When `Some`, the pass clears the target to this colour before drawing
    /// the UI.  Set by the renderer when `RendererMode::Desktop2D` is active
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
    /// clear the target before drawing (use in `Desktop2D` mode); pass `None`
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

    fn prepare(&mut self, _device: &Device, _queue: &Queue, _packet: &FramePacket) {}

    fn execute(
        &mut self,
        _device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        color_view: &TextureView,
        resolve_target: Option<&TextureView>,
        _depth_view: Option<&TextureView>,
        packet: &FramePacket,
    ) {
        let ui_batch = packet.get::<GuiBatch>();
        let text_batch = packet.get::<TextBatch>();

        let has_ui = ui_batch.map_or(false, |b| !b.is_empty());
        let has_text = text_batch.map_or(false, |b| !b.is_empty());
        if !has_ui && !has_text {
            return;
        }

        let empty = GuiBatch::new();
        let batch = ui_batch.unwrap_or(&empty);

        match self.clear_color {
            Some(clear) => self.renderer.render_clearing(
                encoder,
                color_view,
                resolve_target,
                batch,
                queue,
                text_batch,
                clear,
            ),
            None => self.renderer.render(
                encoder,
                color_view,
                resolve_target,
                batch,
                queue,
                text_batch,
            ),
        }
    }
}
