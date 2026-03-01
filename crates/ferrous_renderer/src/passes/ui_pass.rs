/// UI overlay pass — composites GUI quads and text on top of the 3-D scene.
use std::cell::RefCell;

use wgpu::{CommandEncoder, Device, Queue, TextureView};

use ferrous_gui::GuiRenderer;

use crate::graph::{FramePacket, RenderPass};

pub struct UiPass {
    renderer: RefCell<GuiRenderer>,
}

impl UiPass {
    pub fn new(renderer: GuiRenderer) -> Self {
        Self { renderer: RefCell::new(renderer) }
    }

    pub fn set_font_atlas(&self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) {
        self.renderer.borrow_mut().set_font_atlas(view, sampler);
    }

    pub fn resize(&self, queue: &wgpu::Queue, width: u32, height: u32) {
        self.renderer.borrow_mut().resize(queue, width, height);
    }
}

impl RenderPass for UiPass {
    fn name(&self) -> &str { "UI Overlay Pass" }

    fn as_any(&self)         -> &dyn std::any::Any      { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any  { self }

    fn prepare(&mut self, _device: &Device, _queue: &Queue, _packet: &FramePacket) {}

    fn execute(
        &self,
        _device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        color_view: &TextureView,
        resolve_target: Option<&TextureView>,
        _depth_view: Option<&TextureView>,
        packet: &FramePacket,
    ) {
        let has_ui   = packet.ui_batch.as_ref().map_or(false, |b| !b.is_empty());
        let has_text = packet.text_batch.as_ref().map_or(false, |b| !b.is_empty());
        if !has_ui && !has_text {
            return;
        }

        let empty = ferrous_gui::GuiBatch::new();
        let batch = packet.ui_batch.as_ref().unwrap_or(&empty);

        self.renderer.borrow_mut().render(
            encoder,
            color_view,
            resolve_target,
            batch,
            queue,
            packet.text_batch.as_ref(),
        );
    }
}
