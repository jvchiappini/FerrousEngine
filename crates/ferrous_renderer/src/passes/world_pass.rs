/// 3-D / 2-D opaque geometry pass.
///
/// Clears color + depth, binds the camera and the shared model-matrix buffer,
/// and emits one indexed draw call per `DrawCommand` in the `FramePacket`.
///
/// ## Dynamic model buffer
///
/// All per-object model matrices live in a single `ModelBuffer`.  This pass
/// receives a reference to the current bind group + stride via
/// [`WorldPass::set_model_buffer`].  Each draw call sets only the dynamic
/// offset (4 bytes on the CPU), so the total CPU overhead for model matrix
/// binding is O(1) GPU-API calls instead of O(N).
///
/// Supports both 3-D (perspective) and 2-D (orthographic) cameras — the
/// distinction is entirely in the camera's view-projection matrix.
use std::sync::Arc;

use wgpu::{
    Color, CommandEncoder, Device, LoadOp, Operations, Queue,
    RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor,
    StoreOp, TextureView,
};

use crate::graph::{FramePacket, RenderPass};
use crate::pipeline::WorldPipeline;

pub struct WorldPass {
    pipeline:          WorldPipeline,
    camera_bind_group: Arc<wgpu::BindGroup>,
    /// Shared dynamic model-matrix bind group (one for the whole scene).
    model_bind_group:  Option<Arc<wgpu::BindGroup>>,
    /// Byte stride between matrix slots in the model buffer.
    model_stride:      u32,
    /// Sky / clear color.
    pub clear_color: Color,
}

impl WorldPass {
    pub fn new(pipeline: WorldPipeline, camera_bind_group: Arc<wgpu::BindGroup>) -> Self {
        Self {
            pipeline,
            camera_bind_group,
            model_bind_group: None,
            model_stride: 256, // safe default; overwritten by set_model_buffer
            clear_color: Color { r: 0.1, g: 0.2, b: 0.3, a: 1.0 },
        }
    }

    /// Called by `Renderer` whenever the `ModelBuffer` is created or reallocated.
    pub fn set_model_buffer(&mut self, bind_group: Arc<wgpu::BindGroup>, stride: u32) {
        self.model_bind_group = Some(bind_group);
        self.model_stride = stride;
    }
}

impl RenderPass for WorldPass {
    fn name(&self) -> &str { "World Opaque Pass" }

    fn prepare(&mut self, _device: &Device, _queue: &Queue, _packet: &FramePacket) {}

    fn execute(
        &mut self,
        _device: &Device,
        _queue: &Queue,
        encoder: &mut CommandEncoder,
        color_view: &TextureView,
        resolve_target: Option<&TextureView>,
        depth_view: Option<&TextureView>,
        packet: &FramePacket,
    ) {
        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: color_view,
                resolve_target,
                ops: Operations {
                    load:  LoadOp::Clear(self.clear_color),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: depth_view.map(|v| RenderPassDepthStencilAttachment {
                view: v,
                depth_ops: Some(Operations {
                    load:  LoadOp::Clear(1.0),
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set:  None,
            timestamp_writes:     None,
        });

        if let Some(vp) = &packet.viewport {
            rpass.set_viewport(
                vp.x as f32, vp.y as f32,
                vp.width as f32, vp.height as f32,
                0.0, 1.0,
            );
            rpass.set_scissor_rect(vp.x, vp.y, vp.width, vp.height);
        }

        rpass.set_pipeline(&self.pipeline.inner);
        rpass.set_bind_group(0, &*self.camera_bind_group, &[]);

        if let Some(model_bg) = &self.model_bind_group {
            for cmd in &packet.scene_objects {
                // Dynamic offset: move the shader's view of the buffer to slot N.
                let offset = (cmd.model_slot as u32).wrapping_mul(self.model_stride);
                rpass.set_bind_group(1, model_bg.as_ref(), &[offset]);
                rpass.set_vertex_buffer(0, cmd.vertex_buffer.slice(..));
                rpass.set_index_buffer(cmd.index_buffer.slice(..), cmd.index_format);
                rpass.draw_indexed(0..cmd.index_count, 0, 0..1);
            }
        }
    }
}

